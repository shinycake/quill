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
#![deny(clippy::indexing_slicing)]

use super::anim_layer;
use super::app::QuillApp;
use super::sticker_player::{HeadId, Part, StickerView, ViewAsk};
use gpui_kit::*;
use quill::power_saving;
use quill::sticker_playback::player::PlayMode;
use std::path::PathBuf;

/// tdesktop `kPlayStatusLimit`.
pub(super) const PLAY_LOOPS: u32 = 2;

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

impl StatusSurface {
    /// The part of the window it shows in. A chat list row stays as it is
    /// when another chat opens (tdesktop's rows keep their emoji); the
    /// header, a sender's name and the profile column show the open chat,
    /// which tdesktop builds again for the next one.
    fn part(self) -> Part {
        match self {
            Self::ListRow => Part::Chrome,
            Self::Header | Self::MessageName | Self::Profile => Part::Chat,
        }
    }
}

/// What a status shows right now.
pub(super) enum StatusGlyph {
    /// The animation, drawn by the slice's layer (two loops, then rests).
    Layered(StickerView),
    /// A still image.
    Still(PathBuf),
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
        // Each place counts its loops from its own first showing (the
        // view's head starts with its first frame on screen).
        let head = status_head(surface, owner, id);
        let ask = ViewAsk::emoji(
            item.file_id,
            item.format,
            head,
            PlayMode::Loops(PLAY_LOOPS),
            STATUS_SIZE,
        );
        if may_animate && let Some(view) = self.sticker_view(ask, cx) {
            return Some(StatusGlyph::Layered(view));
        }
        self.panel_still(&item).map(StatusGlyph::Still)
    }
}

/// The play head of status `id` for `owner` on `surface`.
pub(super) fn status_head(surface: StatusSurface, owner: i64, id: i64) -> HeadId {
    HeadId::of(surface.part(), ("status", surface, owner, id))
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
        StatusGlyph::Layered(clip) => anim_layer::frames(clip)
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
