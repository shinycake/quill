//! The top-people strip of the empty chat search (tdesktop
//! `Dialogs::TopPeersStrip`, dialogs/ui/top_peers_strip.cpp, fed by
//! `TopPeersContent` in dialogs/ui/dialogs_suggestions.cpp): a row of
//! avatars with a first name, an unread badge and an online dot, which
//! expands into a grid, is walked with the arrow keys, and has its own
//! per-person menu. Everything the strip decides without a window lives
//! here.
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;
use crate::lang::{keys, tr};
use crate::state::Session;
use crate::telegram::envelope::ChatKind;

/// One tile's width: `topPeers.photoLeft * 2 + topPeers.photo`
/// (dialogs.style: 10 + 46 + 10).
pub const TILE_WIDTH: f32 = 66.;
/// The avatar: `topPeers.photo`.
pub const PHOTO: f32 = 46.;
/// One row of tiles: `topPeers.height` (the stories strip's full height).
pub const TILE_HEIGHT: f32 = 78.;
/// `st::slideDuration`, the expand and collapse animation.
pub const EXPAND_MS: u64 = 240;

/// One person in the strip, as `TopPeersEntry` carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopPerson {
    pub chat_id: ChatId,
    pub name: String,
    pub saved_messages: bool,
    pub unread_count: i32,
    pub marked_unread: bool,
    pub muted: bool,
    pub online: bool,
}

impl TopPerson {
    /// The unread badge: the count, or a bare dot for a chat marked as
    /// unread.
    pub fn unread(&self) -> Option<(i32, bool)> {
        if self.unread_count > 0 {
            Some((self.unread_count, false))
        } else if self.marked_unread {
            Some((0, true))
        } else {
            None
        }
    }
}

/// The people to show: `getTopChats` order, minus chats the account no
/// longer knows (tdesktop skips inaccessible users).
pub fn top_people(session: &Session) -> Vec<TopPerson> {
    let search = &session.search.search;
    if search.top_chats_disabled {
        return Vec::new();
    }
    search
        .top_chats
        .iter()
        .filter_map(|id| {
            let chat = session.chats.get(&id.0)?;
            let saved = session.is_saved_messages(*id);
            let name = if saved {
                tr(keys::SAVED_MESSAGES).to_string()
            } else {
                short_name(session, &chat.kind, &chat.title)
            };
            Some(TopPerson {
                chat_id: *id,
                name,
                saved_messages: saved,
                unread_count: chat.unread_count,
                marked_unread: chat.is_marked_as_unread,
                muted: !saved && chat.is_muted(),
                online: !saved && session.chat_peer_online(chat),
            })
        })
        .collect()
}

/// `PeerData::shortName`: a user's first name.
fn short_name(session: &Session, kind: &ChatKind, title: &str) -> String {
    if let ChatKind::Private { user_id } = kind
        && let Some(user) = session.user(user_id.0)
        && !user.first_name.trim().is_empty()
    {
        return user.first_name.trim().to_string();
    }
    title.split_whitespace().next().unwrap_or(title).to_string()
}

/// How the strip is shown. The header link reads "Show more" until the
/// first toggle and "Show all" after a collapse
/// (`TopPeersStrip::setupHeader`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StripView {
    #[default]
    Initial,
    Expanded,
    Collapsed,
}

impl StripView {
    pub fn expanded(self) -> bool {
        self == Self::Expanded
    }

    pub fn toggled(self) -> Self {
        if self.expanded() {
            Self::Collapsed
        } else {
            Self::Expanded
        }
    }
}

/// Tiles per row for a strip `width` wide (`std::max(width / single, 1)`).
pub fn columns(width: f32) -> usize {
    if !width.is_finite() || width < TILE_WIDTH {
        return 1;
    }
    // Truncating like the integer division in tdesktop.
    ((width / TILE_WIDTH) as usize).max(1)
}

/// Whether the people do not fit one row, so the header offers the
/// expand link (`count * single > width + single * 2 / 3`).
pub fn overflows(count: usize, width: f32) -> bool {
    width.is_finite() && count as f32 * TILE_WIDTH > width + TILE_WIDTH * 2. / 3.
}

/// Rows of an expanded grid.
pub fn rows(count: usize, columns: usize) -> usize {
    count.max(1).div_ceil(columns.max(1))
}

/// The eased 0 to 1 progress of an expand or collapse `elapsed_ms` in
/// (`anim::easeOutQuint` over [`EXPAND_MS`]).
pub fn expand_progress(elapsed_ms: u64) -> f32 {
    let t = (elapsed_ms as f32 / EXPAND_MS as f32).clamp(0., 1.);
    1. - (1. - t).powi(5)
}

/// Whether an expand or collapse started `elapsed_ms` ago still runs.
pub fn expand_running(elapsed_ms: u64) -> bool {
    elapsed_ms < EXPAND_MS
}

/// A key the strip handles (`Qt::Key_*` in `selectByKeyboard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripKey {
    Left,
    Right,
    Up,
    Down,
}

/// What a key does to the strip's keyboard selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripMove {
    Select(usize),
    /// The selection leaves the strip upward (to the search field) or
    /// downward (to the recent list).
    Leave,
    Unchanged,
}

/// `TopPeersStrip::selectByKeyboard`. `selected` is the tile the keys are
/// on; `None` when the strip has no keyboard selection.
pub fn step(
    selected: Option<usize>,
    key: StripKey,
    count: usize,
    columns: usize,
    expanded: bool,
) -> StripMove {
    let Some(last) = count.checked_sub(1) else {
        return StripMove::Unchanged;
    };
    let columns = columns.max(1);
    let Some(current) = selected.map(|index| index.min(last)) else {
        return match key {
            StripKey::Left | StripKey::Right => StripMove::Select(0),
            StripKey::Up => {
                let rows = if expanded { rows(count, columns) } else { 1 };
                StripMove::Select(rows.saturating_sub(1).saturating_mul(columns).min(last))
            }
            StripKey::Down => StripMove::Select(0),
        };
    };
    match key {
        StripKey::Left if current > 0 => StripMove::Select(current - 1),
        StripKey::Right if current < last => StripMove::Select(current + 1),
        StripKey::Left | StripKey::Right => StripMove::Unchanged,
        StripKey::Up if expanded && current >= columns => StripMove::Select(current - columns),
        StripKey::Up => StripMove::Leave,
        StripKey::Down if expanded && current / columns + 1 < rows(count, columns) => {
            StripMove::Select(current.saturating_add(columns).min(last))
        }
        StripKey::Down => StripMove::Leave,
    }
}

/// The horizontal scroll that shows tile `index` in a collapsed strip
/// (`scrollToSelected`): the least change to `scroll_left`.
pub fn reveal_scroll(scroll_left: f32, viewport: f32, index: usize, max: f32) -> f32 {
    let left = index as f32 * TILE_WIDTH;
    let right = left + TILE_WIDTH;
    let next = if scroll_left > left {
        left
    } else if scroll_left + viewport < right {
        right - viewport
    } else {
        scroll_left
    };
    if max.is_finite() {
        next.clamp(0., max.max(0.))
    } else {
        next.max(0.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_never_drop_below_one() {
        assert_eq!(columns(0.), 1);
        assert_eq!(columns(f32::NAN), 1);
        assert_eq!(columns(65.), 1);
        assert_eq!(columns(330.), 5);
        assert_eq!(columns(331.), 5);
    }

    #[test]
    fn the_link_shows_only_when_a_row_overflows() {
        // Five tiles exactly fit 330; a sixth tile pokes out.
        assert!(!overflows(5, 330.));
        assert!(overflows(6, 330.));
        // A tile that sticks out by less than two thirds still fits.
        assert!(!overflows(5, 300.));
        assert!(overflows(5, 280.));
        assert!(!overflows(0, 0.));
    }

    #[test]
    fn rows_round_up() {
        assert_eq!(rows(0, 5), 1);
        assert_eq!(rows(5, 5), 1);
        assert_eq!(rows(6, 5), 2);
        assert_eq!(rows(6, 0), 6);
    }

    #[test]
    fn toggling_remembers_it_was_collapsed() {
        let view = StripView::Initial;
        assert!(!view.expanded());
        let view = view.toggled();
        assert!(view.expanded());
        let view = view.toggled();
        assert_eq!(view, StripView::Collapsed);
        assert!(view.toggled().expanded());
    }

    #[test]
    fn progress_is_eased_and_bounded() {
        assert_eq!(expand_progress(0), 0.);
        assert_eq!(expand_progress(EXPAND_MS), 1.);
        assert_eq!(expand_progress(EXPAND_MS * 4), 1.);
        assert!(expand_progress(EXPAND_MS / 4) > 0.5, "ease-out starts fast");
        assert!(expand_running(EXPAND_MS - 1));
        assert!(!expand_running(EXPAND_MS));
    }

    #[test]
    fn collapsed_strip_walks_sideways_and_leaves_vertically() {
        use StripKey::*;
        let walk = |sel, key| step(sel, key, 8, 5, false);
        assert_eq!(walk(None, Right), StripMove::Select(0));
        assert_eq!(walk(None, Down), StripMove::Select(0));
        assert_eq!(walk(None, Up), StripMove::Select(0));
        assert_eq!(walk(Some(0), Left), StripMove::Unchanged);
        assert_eq!(walk(Some(0), Right), StripMove::Select(1));
        assert_eq!(walk(Some(7), Right), StripMove::Unchanged);
        assert_eq!(walk(Some(3), Down), StripMove::Leave);
        assert_eq!(walk(Some(3), Up), StripMove::Leave);
    }

    #[test]
    fn expanded_grid_moves_by_rows() {
        use StripKey::*;
        // 8 people, 5 columns: rows of 5 and 3.
        let walk = |sel, key| step(sel, key, 8, 5, true);
        assert_eq!(
            walk(None, Up),
            StripMove::Select(5),
            "first of the last row"
        );
        assert_eq!(walk(Some(1), Down), StripMove::Select(6));
        assert_eq!(walk(Some(4), Down), StripMove::Select(7), "short last row");
        assert_eq!(walk(Some(6), Down), StripMove::Leave);
        assert_eq!(walk(Some(6), Up), StripMove::Select(1));
        assert_eq!(walk(Some(2), Up), StripMove::Leave);
    }

    #[test]
    fn an_empty_strip_ignores_keys() {
        assert_eq!(
            step(None, StripKey::Down, 0, 5, false),
            StripMove::Unchanged
        );
    }

    #[test]
    fn a_stale_selection_is_clamped() {
        assert_eq!(
            step(Some(40), StripKey::Left, 3, 5, false),
            StripMove::Select(1)
        );
    }

    #[test]
    fn reveal_scrolls_the_least() {
        // Viewport 200 wide over 66-wide tiles.
        assert_eq!(reveal_scroll(0., 200., 1, 400.), 0.);
        assert_eq!(reveal_scroll(0., 200., 3, 400.), 64.);
        assert_eq!(reveal_scroll(300., 200., 2, 400.), 132.);
        assert_eq!(reveal_scroll(0., 200., 20, 100.), 100.);
        assert_eq!(reveal_scroll(5., 200., 0, f32::NAN), 0.);
    }
}
