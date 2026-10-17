//! Story viewer preloading, after tdesktop's `Controller::preloadNext`
//! (`media/stories/media_stories_controller.cpp`) and `StoryPreload`
//! (`data/data_story.cpp`, `data/data_media_preload.cpp`).

use crate::connect::{STORY_PRELOAD_PRIORITIES, THUMB_DOWNLOAD_PRIORITY};
use crate::ids::FileId;
use crate::settings::AUTO_DOWNLOAD_PHOTO;
use crate::state::Session;
use crate::story_viewer::{StoryViewer, StoryViewerKind};
use crate::telegram::envelope::StoryContentView;

/// tdesktop `kPreloadNextMediaCount`. Its loop is `[index + 1, min(index +
/// 3, count))`, so it preloads two stories ahead, not three.
const NEXT_MEDIA_COUNT: usize = 3;
/// tdesktop `kPreloadPreviousMediaCount`.
const PREVIOUS_MEDIA_COUNT: usize = 1;
/// `kDefaultPreloadPrefix`: video bytes preloaded when the story has no
/// `preload_prefix_size`.
const DEFAULT_PREFIX: i64 = 4 * 1024 * 1024;
/// `kMaxAllowedPreloadPrefix`: a larger server hint is ignored.
const MAX_PREFIX: i64 = 6 * 1024 * 1024;

/// Viewer positions to preload, in tdesktop's order: the next stories
/// first, then the previous one.
pub fn preload_indices(index: usize, count: usize) -> Vec<usize> {
    let till = index.saturating_add(NEXT_MEDIA_COUNT).min(count);
    let from = index.saturating_sub(PREVIOUS_MEDIA_COUNT);
    (index + 1..till).chain((from..index).rev()).collect()
}

/// One `downloadFile` of a preload. `limit` 0 downloads the whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreloadRequest {
    pub file_id: FileId,
    pub priority: i32,
    pub limit: i64,
}

/// What to download for the stories around the viewer's current one.
/// Photos honour the auto-download settings (tdesktop `PhotoPreload::
/// Should`); a video fetches its thumbnail and a prefix of the clip, which
/// tdesktop does regardless of the video auto-download setting. Data saver
/// pauses everything.
pub fn plan(session: &Session, viewer: &StoryViewer) -> Vec<PreloadRequest> {
    if session.settings.media_prefs.data_saver {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (rank, item) in viewer.preload_neighbors().into_iter().enumerate() {
        let priority = STORY_PRELOAD_PRIORITIES
            .get(rank)
            .copied()
            .unwrap_or(THUMB_DOWNLOAD_PRIORITY);
        match item.kind {
            StoryViewerKind::Photo => {
                if session.auto_download_allowed(item.chat_id, AUTO_DOWNLOAD_PHOTO) {
                    out.push(PreloadRequest {
                        file_id: item.download_file_id,
                        priority,
                        limit: 0,
                    });
                }
            }
            StoryViewerKind::Video => {
                if let Some(thumb) = item.display_file_ids.first() {
                    out.push(PreloadRequest {
                        file_id: *thumb,
                        priority,
                        limit: 0,
                    });
                }
                if let Some(clip) = item.video_file_id {
                    let hint = session
                        .stories
                        .stories
                        .get(&(item.chat_id.0, item.story_id))
                        .map_or(0, |story| match story.content {
                            StoryContentView::Video {
                                preload_prefix_size,
                                ..
                            } => i64::from(preload_prefix_size),
                            _ => 0,
                        });
                    let size = session
                        .media
                        .files
                        .get(&clip.0)
                        .map_or(0, |file| file.size.max(file.expected_size));
                    out.push(PreloadRequest {
                        file_id: clip,
                        priority,
                        limit: video_prefix(hint, size),
                    });
                }
            }
            StoryViewerKind::Live | StoryViewerKind::Unsupported => {}
        }
    }
    out
}

/// tdesktop `ChoosePreloadPrefix`: the server hint when it is below 6 MiB,
/// else 4 MiB, never more than the file.
fn video_prefix(hint: i64, size: i64) -> i64 {
    let prefix = if hint > 0 && hint < MAX_PREFIX {
        hint
    } else {
        DEFAULT_PREFIX
    };
    if size > 0 { prefix.min(size) } else { prefix }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::{AccountKey, ChatId};
    use crate::story_viewer::StoryViewerItem;
    use std::sync::Arc;

    fn session() -> Session {
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        Session::new(AccountKey::primary(), sink)
    }

    fn item(story_id: i32, kind: StoryViewerKind, file: i32, clip: Option<i32>) -> StoryViewerItem {
        StoryViewerItem {
            chat_id: ChatId(7),
            story_id,
            kind,
            display_file_ids: vec![FileId(file)],
            download_file_id: FileId(file),
            caption: String::new(),
            caption_entities: Vec::new(),
            duration_label: None,
            duration_secs: None,
            video_file_id: clip.map(FileId),
            is_live: false,
            live_call: None,
            areas: Vec::new(),
        }
    }

    fn viewer() -> StoryViewer {
        StoryViewer::open(
            vec![
                item(1, StoryViewerKind::Photo, 10, None),
                item(2, StoryViewerKind::Photo, 20, None),
                item(3, StoryViewerKind::Video, 30, Some(31)),
                item(4, StoryViewerKind::Photo, 40, None),
            ],
            1,
        )
    }

    #[test]
    fn plan_orders_next_before_previous_below_the_current_priority() {
        let mut session = session();
        session.settings.media_prefs.auto_download_private = AUTO_DOWNLOAD_PHOTO;
        let plan = plan(&session, &viewer());
        let files: Vec<_> = plan.iter().map(|r| r.file_id.0).collect();
        // Story 3 (thumb, clip prefix), story 4, story 1.
        assert_eq!(files, vec![30, 31, 40, 10]);
        let priorities: Vec<_> = plan.iter().map(|r| r.priority).collect();
        assert_eq!(priorities, vec![7, 7, 6, 5]);
        assert!(
            priorities
                .iter()
                .all(|p| *p < crate::connect::USER_DOWNLOAD_PRIORITY)
        );
    }

    #[test]
    fn plan_preloads_only_a_video_prefix() {
        let session = session();
        let clip = plan(&session, &viewer())
            .into_iter()
            .find(|r| r.file_id == FileId(31))
            .expect("clip prefix requested");
        assert_eq!(clip.limit, DEFAULT_PREFIX);
    }

    #[test]
    fn data_saver_pauses_preloading() {
        let mut session = session();
        session.settings.media_prefs.data_saver = true;
        assert!(plan(&session, &viewer()).is_empty());
    }

    #[test]
    fn preloads_two_ahead_then_one_behind() {
        assert_eq!(preload_indices(2, 10), vec![3, 4, 1]);
    }

    #[test]
    fn edges_have_no_neighbors_beyond_the_list() {
        assert_eq!(preload_indices(0, 10), vec![1, 2]);
        assert_eq!(preload_indices(9, 10), vec![8]);
        assert_eq!(preload_indices(0, 1), Vec::<usize>::new());
        assert_eq!(preload_indices(0, 0), Vec::<usize>::new());
    }

    #[test]
    fn video_prefix_follows_the_server_hint_and_file_size() {
        assert_eq!(video_prefix(0, 0), DEFAULT_PREFIX);
        assert_eq!(video_prefix(0, 1_000), 1_000);
        assert_eq!(video_prefix(500_000, 9_000_000), 500_000);
        assert_eq!(video_prefix(7_000_000, 9_000_000), DEFAULT_PREFIX);
    }
}
