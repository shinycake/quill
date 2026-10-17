//! The posters the story viewer walks when it was opened from the story
//! tray, ported from tdesktop's `Media::Stories::Controller`
//! (`media_stories_controller.cpp`): the cached sources list and its
//! rebuild (`rebuildCachedSourcesList`, :1628-1717), the story a poster
//! opens on (`LookupShownId`, `media_stories_sibling.cpp:37-46`, and
//! `StoriesSource::toOpen`, `data_stories.cpp:178-184`), the step across
//! poster boundaries (`subjumpFor`, :1375-1396) and when to load more of
//! the tray (`kPreloadPeersCount`, :79 and :773).
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::ChatActiveStoriesView;

/// tdesktop `kPreloadPeersCount`: load more of the tray once fewer posters
/// than this remain from the current one (itself included).
pub const PRELOAD_POSTERS: usize = 3;

/// One tray entry in tray order: who posted and whether any of their
/// stories is unread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraySource {
    pub chat_id: i64,
    pub unread: bool,
}

impl TraySource {
    pub fn of(entry: &ChatActiveStoriesView) -> Self {
        Self {
            chat_id: entry.chat_id,
            unread: entry
                .stories
                .iter()
                .any(|info| info.story_id > entry.max_read_story_id),
        }
    }
}

/// A poster in the walk and the story last shown for it in this viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Poster {
    chat_id: i64,
    shown: Option<i32>,
}

/// The walk: posters in the order the tray had when the viewer opened,
/// merged with later tray changes so the next poster never jumps around
/// as stories are read. Opened on an unread story, it keeps only posters
/// with unread stories (tdesktop `_showingUnreadSources`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayWalk {
    posters: Vec<Poster>,
    index: usize,
    unread_only: bool,
}

/// Where a step of `delta` stories goes (tdesktop `subjumpFor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryStep {
    /// To this index in the current poster's stories.
    Within(usize),
    /// Back from the first story with no poster before it: restart it.
    Restart,
    /// To the neighbour poster in this direction (-1 or 1).
    Poster(i32),
    /// Past the last story with no poster after it: the viewer closes.
    End,
}

/// The step from `index` of `count` stories. `left` / `right` say whether a
/// neighbour poster has a story to open.
pub fn plan_step(index: usize, count: usize, delta: i32, left: bool, right: bool) -> StoryStep {
    let target = index as i64 + i64::from(delta);
    if target < 0 {
        if left {
            StoryStep::Poster(-1)
        } else {
            StoryStep::Restart
        }
    } else if target >= count as i64 {
        if right {
            StoryStep::Poster(1)
        } else {
            StoryStep::End
        }
    } else {
        usize::try_from(target).map_or(StoryStep::End, StoryStep::Within)
    }
}

/// tdesktop `subjumpAvailable`: whether Prev / Next lead anywhere.
pub fn step_available(index: usize, count: usize, delta: i32, left: bool, right: bool) -> bool {
    match plan_step(index, count, delta, left, right) {
        StoryStep::Within(_) | StoryStep::Poster(_) => true,
        StoryStep::Restart | StoryStep::End => false,
    }
}

/// The story a poster opens on: the one last shown for it in this viewer
/// while it is still active, else the first unread one, else the first.
pub fn story_to_open(entry: &ChatActiveStoriesView, shown: Option<i32>) -> Option<i32> {
    let ids = || entry.stories.iter().map(|info| info.story_id);
    if let Some(shown) = shown
        && ids().any(|id| id == shown)
    {
        return Some(shown);
    }
    ids()
        .find(|id| *id > entry.max_read_story_id)
        .or_else(|| ids().next())
}

/// Whether to load more of the tray (`loadActiveStories`): fewer than
/// [`PRELOAD_POSTERS`] posters remain from `current` in tray order.
pub fn wants_more_tray(sources: &[TraySource], current: i64) -> bool {
    sources
        .iter()
        .position(|source| source.chat_id == current)
        .is_some_and(|at| sources.len() - at < PRELOAD_POSTERS)
}

impl TrayWalk {
    /// A walk opened on `current`'s story `story` from `sources` (tray
    /// order). `None` when `current` is not in the tray.
    pub fn open(
        sources: &[TraySource],
        current: i64,
        story: i32,
        unread_only: bool,
    ) -> Option<Self> {
        let mut walk = Self {
            posters: Vec::new(),
            index: 0,
            unread_only,
        };
        walk.show(sources, current, story).then_some(walk)
    }

    /// The poster shown now.
    pub fn current(&self) -> Option<i64> {
        self.posters.get(self.index).map(|poster| poster.chat_id)
    }

    /// The neighbour poster in direction `delta` and the story last shown
    /// for it, if any.
    pub fn neighbour(&self, delta: i32) -> Option<(i64, Option<i32>)> {
        let index = match delta {
            -1 => self.index.checked_sub(1)?,
            1 => self.index.checked_add(1)?,
            _ => return None,
        };
        self.posters
            .get(index)
            .map(|poster| (poster.chat_id, poster.shown))
    }

    /// Show `story` of `current`, merging the tray's current order into the
    /// walk (tdesktop `rebuildCachedSourcesList`). Returns `false`, leaving
    /// the walk as it was, when `current` is not in the tray.
    pub fn show(&mut self, sources: &[TraySource], current: i64, story: i32) -> bool {
        let Some(at) = sources.iter().position(|s| s.chat_id == current) else {
            return false;
        };
        self.posters
            .retain(|poster| sources.iter().any(|s| s.chat_id == poster.chat_id));
        match self.posters.iter().position(|p| p.chat_id == current) {
            Some(index) => self.index = index,
            None => self.posters.clear(),
        }
        let keep =
            |source: &TraySource| !self.unread_only || source.unread || source.chat_id == current;
        if self.posters.is_empty() {
            self.posters = sources
                .iter()
                .filter(|s| keep(s))
                .map(|s| Poster {
                    chat_id: s.chat_id,
                    shown: None,
                })
                .collect();
            self.index = self
                .posters
                .iter()
                .position(|p| p.chat_id == current)
                .unwrap_or(0);
        } else {
            let (before, after) = sources.split_at(at);
            for source in before.iter().rev() {
                if keep(source) && !self.has(source.chat_id) {
                    self.posters.insert(
                        0,
                        Poster {
                            chat_id: source.chat_id,
                            shown: None,
                        },
                    );
                    self.index += 1;
                }
            }
            for source in after.iter().skip(1) {
                if keep(source) && !self.has(source.chat_id) {
                    self.posters.push(Poster {
                        chat_id: source.chat_id,
                        shown: None,
                    });
                }
            }
        }
        if let Some(poster) = self.posters.get_mut(self.index) {
            poster.shown = Some(story);
        }
        true
    }

    fn has(&self, chat_id: i64) -> bool {
        self.posters.iter().any(|p| p.chat_id == chat_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{StoryInfoView, StoryListView};

    fn entry(chat: i64, ids: &[i32], max_read: i32) -> ChatActiveStoriesView {
        ChatActiveStoriesView {
            chat_id: chat,
            list: Some(StoryListView::Main),
            order: 1,
            max_read_story_id: max_read,
            stories: ids
                .iter()
                .map(|&story_id| StoryInfoView {
                    story_id,
                    date: story_id,
                    is_for_close_friends: false,
                    is_live: false,
                })
                .collect(),
        }
    }

    fn src(chat_id: i64, unread: bool) -> TraySource {
        TraySource { chat_id, unread }
    }

    fn order(walk: &TrayWalk) -> Vec<i64> {
        walk.posters.iter().map(|p| p.chat_id).collect()
    }

    #[test]
    fn steps_cross_poster_boundaries_like_subjump_for() {
        assert_eq!(plan_step(1, 3, 1, true, true), StoryStep::Within(2));
        assert_eq!(plan_step(2, 3, 1, true, true), StoryStep::Poster(1));
        assert_eq!(plan_step(2, 3, 1, true, false), StoryStep::End);
        assert_eq!(plan_step(0, 3, -1, true, true), StoryStep::Poster(-1));
        assert_eq!(plan_step(0, 3, -1, false, true), StoryStep::Restart);
        assert!(!step_available(0, 3, -1, false, true));
        assert!(step_available(2, 3, 1, false, true));
        assert!(!step_available(2, 3, 1, true, false));
    }

    #[test]
    fn a_poster_opens_on_its_shown_then_first_unread_then_first_story() {
        let poster = entry(7, &[3, 5, 8], 5);
        assert_eq!(story_to_open(&poster, Some(3)), Some(3));
        assert_eq!(story_to_open(&poster, None), Some(8));
        assert_eq!(story_to_open(&poster, Some(4)), Some(8), "a gone story");
        assert_eq!(story_to_open(&entry(7, &[3, 5], 5), None), Some(3));
        assert_eq!(story_to_open(&entry(7, &[], 0), None), None);
    }

    #[test]
    fn opened_on_an_unread_story_the_walk_skips_read_posters() {
        let sources = [src(1, true), src(2, false), src(3, true), src(4, false)];
        let walk = TrayWalk::open(&sources, 2, 9, true).expect("in the tray");
        assert_eq!(order(&walk), vec![1, 2, 3], "the current poster stays");
        assert_eq!(walk.neighbour(-1), Some((1, None)));
        assert_eq!(walk.neighbour(1), Some((3, None)));
        let walk = TrayWalk::open(&sources, 2, 9, false).expect("in the tray");
        assert_eq!(order(&walk), vec![1, 2, 3, 4]);
        assert!(TrayWalk::open(&sources, 99, 1, false).is_none());
    }

    #[test]
    fn the_walk_keeps_its_order_as_the_tray_reorders() {
        let mut walk = TrayWalk::open(&[src(1, true), src(2, true), src(3, true)], 1, 10, false)
            .expect("in the tray");
        // Reading poster 1 moves it to the end of the tray; the walk keeps
        // it first, so its neighbour stays poster 2.
        let read = [src(2, true), src(3, true), src(1, false)];
        assert!(walk.show(&read, 2, 20));
        assert_eq!(order(&walk), vec![1, 2, 3]);
        assert_eq!(walk.current(), Some(2));
        assert_eq!(
            walk.neighbour(-1),
            Some((1, Some(10))),
            "back to the shown story"
        );
        // A new poster before the current one goes in front, one after it
        // at the end; a poster that left the tray leaves the walk.
        let grown = [src(5, true), src(2, true), src(4, true), src(1, false)];
        assert!(walk.show(&grown, 2, 21));
        assert_eq!(order(&walk), vec![5, 1, 2, 4]);
        assert_eq!(walk.current(), Some(2));
        assert_eq!(walk.neighbour(1), Some((4, None)));
        // A current poster missing from the tray leaves the walk alone.
        assert!(!walk.show(&[src(5, true)], 2, 21));
        assert_eq!(order(&walk), vec![5, 1, 2, 4]);
    }

    #[test]
    fn more_of_the_tray_loads_when_fewer_than_three_posters_remain() {
        let sources = [src(1, true), src(2, true), src(3, true), src(4, true)];
        assert!(!wants_more_tray(&sources, 2));
        assert!(wants_more_tray(&sources, 3));
        assert!(wants_more_tray(&sources, 4));
        assert!(!wants_more_tray(&sources, 99));
    }
}
