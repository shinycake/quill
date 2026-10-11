//! Contacts list ordering, section headers and the alphabetical index bar
//! (pure, no UI).
//!
//! Telegram Desktop's contacts box (`boxes/peer_list_controllers.cpp`,
//! `ContactsBoxController`) sorts by last seen by default and by name on
//! request. Sorted by name it shows a section header per first letter
//! (`applySectionHeaders`: a letter, or `#` for anything else), and the
//! box carries an index bar (`boxes/peer_list_section_index.cpp`) that
//! scrubs to a section and magnifies the letters near the pointer. The
//! bar shows only when there are two or more sections, and thins its
//! letters when the column is too short for all of them.

use crate::state::ContactRow;
use crate::telegram::envelope::UserStatusKind;

/// How the contacts list is ordered (tdesktop `ContactsBoxController::SortMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    /// Most recently seen first. The default.
    #[default]
    Online,
    /// By name, with a section header per first letter.
    Alphabet,
}

impl SortMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Online => Self::Alphabet,
            Self::Alphabet => Self::Online,
        }
    }

    /// Label of the button that switches *to* the other mode.
    pub fn switch_label(self) -> &'static str {
        match self {
            Self::Online => "Sort by name",
            Self::Alphabet => "Sort by last seen",
        }
    }
}

/// Sort key of a status for the last-seen order, larger is more recent.
/// An exact time keeps its timestamp; the coarse buckets rank below any
/// real time, like tdesktop's negative `onlineTill` values.
pub fn last_seen_rank(status: &UserStatusKind) -> i64 {
    match *status {
        UserStatusKind::Online => i64::MAX,
        UserStatusKind::Offline { was_online } => i64::from(was_online.max(0)),
        UserStatusKind::Recently => -2,
        UserStatusKind::LastWeek => -3,
        UserStatusKind::LastMonth => -4,
        UserStatusKind::Empty => -5,
    }
}

/// The section a name belongs to: its first letter in upper case, or `#`.
pub fn section_letter(name: &str) -> char {
    name.trim()
        .chars()
        .next()
        .filter(|c| c.is_alphabetic())
        .and_then(|c| c.to_uppercase().next())
        .unwrap_or('#')
}

/// Every word of `query` must start a word of `name` (case-insensitive),
/// like the peer list's search. An empty query matches everything.
pub fn matches_query(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    let words: Vec<&str> = name.split_whitespace().collect();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|part| words.iter().any(|word| word.starts_with(part)))
}

/// One line of the rendered list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListItem {
    Header(char),
    Contact(ContactRow),
}

/// Order and filter `rows`. A search shows a flat list, as does the
/// last-seen order; the name order adds the section headers.
pub fn arrange(rows: &[ContactRow], mode: SortMode, query: &str) -> Vec<ListItem> {
    let mut picked: Vec<&ContactRow> = rows
        .iter()
        .filter(|row| matches_query(&row.name, query))
        .collect();
    match mode {
        SortMode::Online => picked.sort_by(|a, b| {
            b.last_seen
                .cmp(&a.last_seen)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.user_id.cmp(&b.user_id))
        }),
        SortMode::Alphabet => picked.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.user_id.cmp(&b.user_id))
        }),
    }
    let headers = mode == SortMode::Alphabet && query.trim().is_empty();
    let mut items = Vec::with_capacity(picked.len() + 8);
    let mut current = None;
    for row in picked {
        if headers {
            let letter = section_letter(&row.name);
            if current != Some(letter) {
                current = Some(letter);
                items.push(ListItem::Header(letter));
            }
        }
        items.push(ListItem::Contact(row.clone()));
    }
    items
}

/// Heights used to place items without measuring the laid-out list.
pub const HEADER_HEIGHT: f32 = 28.;
pub const ROW_HEIGHT: f32 = 52.;

pub fn item_height(item: &ListItem) -> f32 {
    match item {
        ListItem::Header(_) => HEADER_HEIGHT,
        ListItem::Contact(_) => ROW_HEIGHT,
    }
}

/// A section's letter and the offset of its header from the list top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionStop {
    pub letter: char,
    pub top: f32,
}

pub fn section_stops(items: &[ListItem]) -> Vec<SectionStop> {
    let mut top = 0.;
    let mut stops = Vec::new();
    for item in items {
        if let ListItem::Header(letter) = item {
            stops.push(SectionStop {
                letter: *letter,
                top,
            });
        }
        top += item_height(item);
    }
    stops
}

/// The index bar shows only with two or more sections.
pub fn index_shown(stops: &[SectionStop]) -> bool {
    stops.len() >= 2
}

/// Letters whose section overlaps the viewport `[scroll, scroll + height)`.
pub fn visible_letters(
    stops: &[SectionStop],
    total_height: f32,
    scroll: f32,
    height: f32,
) -> Vec<char> {
    stops
        .iter()
        .enumerate()
        .filter(|(i, stop)| {
            let end = stops.get(i + 1).map_or(total_height, |next| next.top);
            stop.top < scroll + height && end > scroll
        })
        .map(|(_, stop)| stop.letter)
        .collect()
}

/// Metrics of the bar (`peer_list_section_index.style`).
pub const BAR_WIDTH: f32 = 22.;
pub const BAR_APPROACH: f32 = 32.;
pub const SLOT_HEIGHT: f32 = 15.;
pub const MIN_SLOT_HEIGHT: f32 = 12.;
pub const VERTICAL_INSET: f32 = 10.;
const MAX_BOOST: f32 = 1.8;
const REACH_SLOTS: f32 = 2.2;
const BULGE_RESERVE_SLOTS: f32 = 2.5;

/// One letter drawn on the bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub letter: char,
    /// Index into the section list this slot jumps to.
    pub source: usize,
    /// Centre of the slot from the bar top.
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BarLayout {
    pub slots: Vec<Slot>,
    pub pitch: f32,
}

/// Place the letters in a bar `height` tall. When they do not fit, keep
/// every n-th one and always the last (`PeerListSectionIndex::relayout`).
pub fn bar_layout(stops: &[SectionStop], height: f32) -> BarLayout {
    let count = stops.len();
    if count == 0 || height <= 0. {
        return BarLayout::default();
    }
    let full = (height - 2. * VERTICAL_INSET).max(MIN_SLOT_HEIGHT);
    let usable = (full - BULGE_RESERVE_SLOTS * SLOT_HEIGHT).max(MIN_SLOT_HEIGHT);
    let max_fit = ((usable / MIN_SLOT_HEIGHT) as usize).max(1);
    let skip = if count <= max_fit {
        1
    } else {
        count.div_ceil(max_fit)
    };
    let mut kept: Vec<usize> = (0..count).step_by(skip).collect();
    if kept.last() != Some(&(count - 1)) {
        kept.push(count - 1);
    }
    let pitch = SLOT_HEIGHT.min(usable / kept.len() as f32);
    let block_top = VERTICAL_INSET + ((full - kept.len() as f32 * pitch) / 2.).max(0.);
    let slots = kept
        .iter()
        .enumerate()
        .map(|(d, &source)| Slot {
            letter: stops[source].letter,
            source,
            y: block_top + pitch * d as f32 + pitch / 2.,
        })
        .collect();
    BarLayout { slots, pitch }
}

/// The slot nearest to `y`, if the bar has any.
pub fn slot_at(layout: &BarLayout, y: f32) -> Option<usize> {
    layout
        .slots
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (a.y - y).abs().total_cmp(&(b.y - y).abs()))
        .map(|(i, _)| i)
}

/// Magnification of the slot at `slot_y` for a pointer at (`cursor_x`,
/// `cursor_y`) in bar coordinates; 1 when the pointer is away. The effect
/// fades with vertical distance and as the pointer leaves the column
/// sideways (`slotScale`).
pub fn fisheye_scale(slot_y: f32, pitch: f32, cursor: Option<(f32, f32)>, bar_width: f32) -> f32 {
    let Some((cursor_x, cursor_y)) = cursor else {
        return 1.;
    };
    if pitch <= 0. {
        return 1.;
    }
    let vertical = ((slot_y - cursor_y).abs() / (REACH_SLOTS * pitch)).clamp(0., 1.);
    let vertical_falloff = (std::f32::consts::FRAC_PI_2 * vertical).cos();
    let edge = bar_width - BAR_WIDTH;
    let away = ((edge - cursor_x) / BAR_APPROACH).clamp(0., 1.);
    let horizontal_falloff = 1. - away * away * (3. - 2. * away);
    1. + (MAX_BOOST - 1.) * vertical_falloff * horizontal_falloff
}

/// Vertical centre of each slot. At rest they sit where `bar_layout`
/// put them; with the pointer near, the magnified letters take more room
/// and the whole column re-centres around the middle of the bar.
pub fn slot_centers(layout: &BarLayout, scales: &[f32], height: f32, magnified: bool) -> Vec<f32> {
    if !magnified {
        return layout.slots.iter().map(|slot| slot.y).collect();
    }
    let expanded: f32 = scales.iter().map(|scale| scale * layout.pitch).sum();
    let mut edge = VERTICAL_INSET + (height - 2. * VERTICAL_INSET - expanded) / 2.;
    scales
        .iter()
        .map(|scale| {
            let size = scale * layout.pitch;
            let centre = edge + size / 2.;
            edge += size;
            centre
        })
        .collect()
}

/// Scroll offset that brings a section to the top, clamped to the list.
pub fn jump_offset(stop: &SectionStop, total_height: f32, viewport: f32) -> f32 {
    stop.top.min((total_height - viewport).max(0.))
}

/// Text copied by "Invite friends": the message the mobile clients share.
pub const INVITE_TEXT: &str = "Hey, I'm on Telegram. Join me: https://telegram.org/dl";

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, name: &str, last_seen: i64) -> ContactRow {
        ContactRow {
            user_id: id,
            name: name.into(),
            status_text: String::new(),
            is_online: false,
            is_contact: true,
            last_seen,
        }
    }

    fn names(items: &[ListItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                ListItem::Header(c) => format!("[{c}]"),
                ListItem::Contact(r) => r.name.clone(),
            })
            .collect()
    }

    #[test]
    fn letters_and_hash() {
        assert_eq!(section_letter("ada"), 'A');
        assert_eq!(section_letter("  émile"), 'É');
        assert_eq!(section_letter("42 Club"), '#');
        assert_eq!(section_letter("😀"), '#');
        assert_eq!(section_letter(""), '#');
    }

    #[test]
    fn letters_are_unicode_aware() {
        assert_eq!(section_letter("борис"), 'Б');
        assert_eq!(section_letter("Ёлка"), 'Ё');
        assert_eq!(section_letter("سارة"), 'س');
        assert_eq!(section_letter("ñandú"), 'Ñ');
        assert_eq!(section_letter("_hidden"), '#');
        assert_eq!(section_letter("٣ days"), '#');
    }

    #[test]
    fn last_seen_rank_orders_buckets() {
        use UserStatusKind as S;
        let ranks = [
            last_seen_rank(&S::Online),
            last_seen_rank(&S::Offline { was_online: 2000 }),
            last_seen_rank(&S::Offline { was_online: 1000 }),
            last_seen_rank(&S::Recently),
            last_seen_rank(&S::LastWeek),
            last_seen_rank(&S::LastMonth),
            last_seen_rank(&S::Empty),
        ];
        assert!(ranks.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn online_order_is_flat_and_most_recent_first() {
        let rows = [row(1, "Zed", 10), row(2, "Ada", 30), row(3, "Bo", 30)];
        let items = arrange(&rows, SortMode::Online, "");
        assert_eq!(names(&items), ["Ada", "Bo", "Zed"]);
    }

    #[test]
    fn name_order_adds_headers() {
        let rows = [
            row(1, "bob", 0),
            row(2, "Ada", 0),
            row(3, "alan", 0),
            row(4, "7even", 0),
        ];
        let items = arrange(&rows, SortMode::Alphabet, "");
        assert_eq!(
            names(&items),
            ["[#]", "7even", "[A]", "Ada", "alan", "[B]", "bob"]
        );
    }

    #[test]
    fn search_is_flat_and_matches_word_prefixes() {
        let rows = [
            row(1, "Ada Lovelace", 0),
            row(2, "Grace Hopper", 0),
            row(3, "Lovelace Fan", 0),
        ];
        let items = arrange(&rows, SortMode::Alphabet, "love");
        assert_eq!(names(&items), ["Ada Lovelace", "Lovelace Fan"]);
        assert!(arrange(&rows, SortMode::Alphabet, "ace").is_empty());
        assert_eq!(arrange(&rows, SortMode::Alphabet, "  ").len(), 3 + 3);
    }

    #[test]
    fn stops_and_visibility() {
        let rows: Vec<_> = (0..6)
            .map(|i| row(i, if i < 3 { "Ann" } else { "Bea" }, 0))
            .collect();
        let items = arrange(&rows, SortMode::Alphabet, "");
        let stops = section_stops(&items);
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].top, 0.);
        assert_eq!(stops[1].top, HEADER_HEIGHT + 3. * ROW_HEIGHT);
        assert!(index_shown(&stops));
        let total: f32 = items.iter().map(item_height).sum();
        assert_eq!(visible_letters(&stops, total, 0., 100.), ['A']);
        assert_eq!(visible_letters(&stops, total, 150., 100.), ['A', 'B']);
        assert_eq!(visible_letters(&stops, total, total - 50., 50.), ['B']);
        assert_eq!(jump_offset(&stops[1], total, 200.), total - 200.);
        assert_eq!(jump_offset(&stops[1], total, 10_000.), 0.);
    }

    #[test]
    fn single_section_hides_the_bar() {
        let rows = [row(1, "Ann", 0), row(2, "Al", 0)];
        let stops = section_stops(&arrange(&rows, SortMode::Alphabet, ""));
        assert!(!index_shown(&stops));
    }

    fn stops_of(letters: &str) -> Vec<SectionStop> {
        letters
            .chars()
            .enumerate()
            .map(|(i, letter)| SectionStop {
                letter,
                top: i as f32 * 100.,
            })
            .collect()
    }

    #[test]
    fn short_bar_keeps_every_letter_in_a_tall_column() {
        let stops = stops_of("ABCDEFGHIJ");
        let layout = bar_layout(&stops, 400.);
        assert_eq!(layout.slots.len(), 10);
        assert_eq!(layout.pitch, SLOT_HEIGHT);
        assert!(layout.slots.windows(2).all(|p| p[0].y < p[1].y));
    }

    #[test]
    fn narrow_bar_thins_letters_and_keeps_the_last() {
        let stops = stops_of("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        let layout = bar_layout(&stops, 150.);
        assert!(layout.slots.len() < 26);
        assert_eq!(layout.slots.first().unwrap().letter, 'A');
        assert_eq!(layout.slots.last().unwrap().letter, 'Z');
        let top = layout.slots.first().unwrap().y;
        let bottom = layout.slots.last().unwrap().y;
        assert!(top >= VERTICAL_INSET && bottom <= 150. - VERTICAL_INSET);
    }

    #[test]
    fn empty_or_flat_bar_has_no_slots() {
        assert!(bar_layout(&[], 300.).slots.is_empty());
        assert!(bar_layout(&stops_of("AB"), 0.).slots.is_empty());
    }

    #[test]
    fn scrubbing_picks_the_nearest_slot() {
        let layout = bar_layout(&stops_of("ABCD"), 300.);
        let target = layout.slots[2];
        assert_eq!(slot_at(&layout, target.y + 1.), Some(2));
        assert_eq!(slot_at(&layout, -50.), Some(0));
        assert_eq!(slot_at(&layout, 9_999.), Some(3));
        assert_eq!(slot_at(&BarLayout::default(), 5.), None);
    }

    #[test]
    fn magnified_column_stays_centred_and_ordered() {
        let layout = bar_layout(&stops_of("ABCDEF"), 300.);
        let flat = vec![1.; 6];
        let rest = slot_centers(&layout, &flat, 300., false);
        assert_eq!(rest[0], layout.slots[0].y);
        let scales = [1., 1.2, 1.8, 1.2, 1., 1.];
        let spread = slot_centers(&layout, &scales, 300., true);
        assert!(spread.windows(2).all(|p| p[0] < p[1]));
        let middle = (spread[0] + spread[5]) / 2.;
        assert!((middle - 150.).abs() < layout.pitch);
    }

    #[test]
    fn fisheye_peaks_under_the_pointer_and_fades() {
        let width = BAR_WIDTH + BAR_APPROACH;
        let near = fisheye_scale(100., 15., Some((width - 4., 100.)), width);
        let far = fisheye_scale(200., 15., Some((width - 4., 100.)), width);
        let sideways = fisheye_scale(100., 15., Some((0., 100.)), width);
        assert!((near - MAX_BOOST).abs() < 1e-4);
        assert!((far - 1.).abs() < 1e-4);
        assert!(sideways < 1.05);
        assert_eq!(fisheye_scale(100., 15., None, width), 1.);
    }
}
