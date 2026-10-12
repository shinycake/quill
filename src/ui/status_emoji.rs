//! Premium emoji statuses that move (tdesktop `Ui::PeerBadge`,
//! `HistoryView::Message` from-name status, `Info::Profile::Badge`).
//!
//! tdesktop wraps a status in `LimitedLoopsEmoji` with `kPlayStatusLimit =
//! 2`: the animation plays twice from the moment it is first shown, then
//! rests on its first frame. "Autoplay in premium status" in Battery and
//! animations (`PowerSaving::kEmojiStatus`) keeps every status on its first
//! frame. Each place a status is drawn counts its own two loops, as each
//! tdesktop row or message view owns its emoji instance.
//!
//! Where the content is built inside a slice with an animation layer (the
//! chat list, the conversation) the animation is drawn by that layer, so its
//! frames never rebuild the slice. Anywhere else the status stays a still.

use super::anim_layer::{self, LayeredClip};
use super::app::QuillApp;
use super::sticker_playback::PlaybackSize;
use gpui_kit::*;
use quill::power_saving;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// tdesktop `kPlayStatusLimit`.
pub(super) const PLAY_LOOPS: u32 = 2;
/// First-shown times kept before the oldest half is dropped.
const MAX_REMEMBERED: usize = 2048;

/// Where a status is drawn; each place (and owner) plays its own loops.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum StatusSurface {
    /// A chat list row (owner: the chat).
    ListRow,
    /// The open chat's header (owner: the chat).
    Header,
    /// A group message's sender name (owner: the message).
    MessageName,
    /// The profile panel (owner: the peer).
    Profile,
}

type Key = (StatusSurface, i64, i64);

thread_local! {
    static FIRST_SHOWN: RefCell<HashMap<Key, Instant>> = RefCell::new(HashMap::new());
}

/// What a status shows right now.
pub(super) enum StatusGlyph {
    /// The animation, drawn by the slice's layer (two loops, then rests).
    Layered(LayeredClip),
    /// A still image.
    Still(PathBuf),
}

/// When `key` was first shown with a decoded animation (now, the first
/// time).
fn first_shown(key: Key, now: Instant) -> Instant {
    FIRST_SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        if shown.len() >= MAX_REMEMBERED && !shown.contains_key(&key) {
            let mut times: Vec<Instant> = shown.values().copied().collect();
            times.sort_unstable();
            let cut = times[times.len() / 2];
            shown.retain(|_, at| *at > cut);
        }
        *shown.entry(key).or_insert(now)
    })
}

impl QuillApp {
    /// The glyph of status `id` for `owner` on `surface`: the animation when
    /// it may play here, else the still, `None` while the file is missing
    /// (the caller then shows the star).
    pub(super) fn status_glyph(
        &self,
        surface: StatusSurface,
        owner: i64,
        id: i64,
        cx: &mut Context<Self>,
    ) -> Option<StatusGlyph> {
        let item = self
            .session()?
            .stickers
            .emoji
            .custom_emoji_stickers
            .iter()
            .find(|item| item.custom_emoji_id == Some(id))?
            .clone();
        let may_animate = !power_saving::on(power_saving::Flag::EmojiStatus)
            && anim_layer::current().is_some()
            && (self.slices.in_sidebar() || self.slices.in_conversation());
        if may_animate
            && let Some(mut clip) =
                self.layered_clip(item.file_id, item.format, PlaybackSize::Emoji, false, cx)
        {
            let shown = first_shown((surface, owner, id), Instant::now());
            clip.started = clip.started.max(shown);
            clip.loops = Some(PLAY_LOOPS);
            return Some(StatusGlyph::Layered(clip));
        }
        self.panel_still(&item).map(StatusGlyph::Still)
    }
}

impl QuillApp {
    /// What follows a group message's sender name (tdesktop
    /// `Message::paintFromName`): the sender's animated status, or the
    /// Premium star in the name's color at 45% opacity until the status
    /// file is there.
    pub(super) fn sender_status_element(
        &self,
        sender: Option<&super::history_row::SenderLabel>,
        message_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let label = sender?;
        let id = label.status?;
        let color = label
            .accent
            .map(super::chat_theme::peer_name_color)
            .unwrap_or_else(|| Hsla::from(super::chat_theme::text_bright()))
            .opacity(STAR_ALPHA);
        let glyph = (id != 0)
            .then(|| self.status_glyph(StatusSurface::MessageName, message_id, id, cx))
            .flatten();
        Some(match glyph {
            Some(glyph) => status_element(glyph, STATUS_SIZE),
            None => div()
                .flex_none()
                .text_size(px(12.))
                .line_height(px(STATUS_SIZE))
                .text_color(color)
                .child("\u{2605}")
                .into_any_element(),
        })
    }
}

/// Side of the status after a sender's name.
const STATUS_SIZE: f32 = 14.;
/// tdesktop paints the name's star with alpha 115 of 255.
const STAR_ALPHA: f32 = 115. / 255.;

/// The glyph as an element `size` px square.
pub(super) fn status_element(glyph: StatusGlyph, size: f32) -> AnyElement {
    match glyph {
        StatusGlyph::Layered(clip) => anim_layer::frames(clip, 30)
            .size(px(size))
            .flex_none()
            .into_any_element(),
        StatusGlyph::Still(path) => img(path)
            .size(px(size))
            .aspect_square()
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::{FIRST_SHOWN, Instant, MAX_REMEMBERED, StatusSurface, first_shown};

    #[test]
    fn a_status_counts_its_loops_from_its_first_showing() {
        let key = (StatusSurface::ListRow, 7, 99);
        let t0 = Instant::now();
        let first = first_shown(key, t0);
        assert_eq!(first, t0);
        let later = t0 + std::time::Duration::from_secs(30);
        assert_eq!(first_shown(key, later), t0, "kept across redraws");
        assert_eq!(
            first_shown((StatusSurface::Header, 7, 99), later),
            later,
            "another surface starts afresh"
        );
    }

    #[test]
    fn the_remembered_times_stay_bounded() {
        let t0 = Instant::now();
        for i in 0..(MAX_REMEMBERED as i64 + 10) {
            first_shown(
                (StatusSurface::MessageName, i, 1),
                t0 + std::time::Duration::from_millis(i as u64),
            );
        }
        let len = FIRST_SHOWN.with(|shown| shown.borrow().len());
        assert!(len <= MAX_REMEMBERED, "{len}");
    }
}
