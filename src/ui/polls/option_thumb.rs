//! The small picture beside a poll option (`pollOption.media`), as
//! tdesktop draws it (`history_view_poll.cpp`, `MakePollThumbnail`): a
//! rounded square at the end of the option row showing the photo, the first
//! frame of a clip, the sticker, the link's picture (a link glyph without
//! one) or the map tile of a place. A click opens it, and does not vote.

use super::super::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::state::Session;
use quill::telegram::envelope::PollOptionMedia;
use std::collections::HashMap;
use std::path::PathBuf;

/// The side of the square, tdesktop's `PollAnswerMediaSize`
/// (twice the radio button's diameter).
const THUMB: f32 = 40.;

/// What kind of picture it is, for its glyph, badge and accessible name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum ThumbKind {
    Photo,
    Video,
    Animation,
    Sticker,
    Link,
    Place,
}

impl ThumbKind {
    fn of(media: &PollOptionMedia) -> Self {
        match media {
            PollOptionMedia::Photo(_) => Self::Photo,
            PollOptionMedia::Video(_) => Self::Video,
            PollOptionMedia::Animation(_) => Self::Animation,
            PollOptionMedia::Sticker(_) => Self::Sticker,
            PollOptionMedia::Link { .. } => Self::Link,
            PollOptionMedia::Location(_) | PollOptionMedia::Venue { .. } => Self::Place,
        }
    }

    /// The glyph shown until the picture is on disk.
    fn glyph(self, media: &PollOptionMedia) -> String {
        match (self, media) {
            (Self::Sticker, PollOptionMedia::Sticker(sticker)) if !sticker.emoji.is_empty() => {
                sticker.emoji.clone()
            }
            (Self::Photo, _) => "\u{1F5BC}".into(),
            (Self::Video, _) => "\u{25B6}".into(),
            (Self::Animation, _) => "GIF".into(),
            (Self::Sticker, _) => "\u{1F642}".into(),
            (Self::Link, _) => "\u{1F517}".into(),
            (Self::Place, _) => "\u{1F4CD}".into(),
        }
    }

    pub(in crate::ui) fn action_label(self) -> &'static str {
        match self {
            Self::Photo => tr(keys::Q_MEDIA_OPEN_PHOTO),
            Self::Video => tr(keys::Q_MEDIA_PLAY_VIDEO),
            Self::Animation => tr(keys::Q_MEDIA_PLAY_ANIMATION),
            Self::Sticker => tr(keys::Q_MEDIA_VIEW_STICKER_PACK),
            Self::Link => tr(keys::POLLS_OPTION_OPEN_LINK_TITLE),
            Self::Place => tr(keys::Q_MEDIA_SHOW_ON_MAP),
        }
    }
}

/// One option's picture, ready to draw.
#[derive(Debug, Clone)]
pub(in crate::ui) struct OptionThumb {
    pub(in crate::ui) kind: ThumbKind,
    /// The picture on disk, once downloaded.
    pub(in crate::ui) path: Option<PathBuf>,
    /// Shown in place of the picture.
    pub(in crate::ui) glyph: String,
    /// The picture's own size, to decode it no larger than needed.
    dims: Option<(i32, i32)>,
}

/// Everything a poll row needs from the session to draw its options'
/// pictures and the custom emoji in their text.
#[derive(Default)]
pub(in crate::ui) struct PollMediaView {
    pub(in crate::ui) thumbs: Vec<Option<OptionThumb>>,
    pub(in crate::ui) emoji: HashMap<i64, ImageSource>,
}

impl PollMediaView {
    /// Some option of the poll has a picture.
    pub(in crate::ui) fn any_thumb(&self) -> bool {
        self.thumbs.iter().any(Option::is_some)
    }

    pub(in crate::ui) fn thumb(&self, index: usize) -> Option<&OptionThumb> {
        self.thumbs.get(index).and_then(Option::as_ref)
    }
}

/// The first of `candidates` that is downloaded and inside the media folders.
fn first_local(
    candidates: &[quill::ids::FileId],
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> Option<PathBuf> {
    candidates.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| quill::local_path::sandboxed_display_path(path, media_roots))
            .map(|path| path.to_path_buf())
    })
}

/// Resolve the pictures and emoji of `poll`'s options from the files the
/// session has.
pub(in crate::ui) fn poll_media_view(
    poll: &quill::telegram::Poll,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    session: Option<&Session>,
) -> PollMediaView {
    let mut view = PollMediaView::default();
    let stickers = session.map(|s| &s.stickers.emoji.custom_emoji_stickers);
    for option in &poll.options {
        view.emoji
            .extend(super::super::message_text::custom_emoji_paths(
                &option.entities,
                stickers,
                files,
                media_roots,
            ));
        view.thumbs.push(option.media.as_ref().map(|media| {
            let path = match media.place() {
                Some(place) => {
                    super::super::history::map_tile_path(session, &place, files, media_roots)
                }
                None => first_local(&media.thumb_candidates(), files, media_roots),
            };
            OptionThumb {
                kind: ThumbKind::of(media),
                path,
                glyph: ThumbKind::of(media).glyph(media),
                dims: match media {
                    PollOptionMedia::Sticker(sticker) => Some((sticker.width, sticker.height)),
                    _ => None,
                },
            }
        }));
    }
    view
}

/// The square for option `index` of the poll in `message_id`.
pub(in crate::ui) fn option_thumb(
    chat_id: ChatId,
    message_id: MessageId,
    index: usize,
    thumb: &OptionThumb,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let side = px(THUMB);
    let fit = if thumb.kind == ThumbKind::Sticker {
        (ObjectFit::Contain, super::super::image_budget::Fit::Contain)
    } else {
        (ObjectFit::Cover, super::super::image_budget::Fit::Cover)
    };
    let playable = matches!(thumb.kind, ThumbKind::Video | ThumbKind::Animation);
    let mut square = div()
        .id(("poll-option-thumb", message_id.0 as u64 * 64 + index as u64))
        .relative()
        .flex_none()
        .size(side)
        .rounded_md()
        .overflow_hidden()
        .bg(fill_muted())
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Button)
        .aria_label(thumb.kind.action_label())
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            // The row votes on a click; the picture only opens.
            cx.stop_propagation();
            this.open_poll_option_media(chat_id, message_id, index, cx);
        }));
    square = match &thumb.path {
        Some(path) => square.child(
            img(super::super::image_budget::sized_media(
                path,
                (side, side),
                thumb.dims,
                fit.1,
            ))
            .size_full()
            .object_fit(fit.0),
        ),
        None => square.child(
            div()
                .text_size(px(18.))
                .text_color(text_muted())
                .child(thumb.glyph.clone()),
        ),
    };
    if thumb.kind == ThumbKind::Place && thumb.path.is_some() {
        square = square.child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(px(8.))
                        .rounded_full()
                        .bg(accent())
                        .border_1()
                        .border_color(gpui_kit::white()),
                ),
        );
    }
    square
        .when(playable && thumb.path.is_some(), |this| {
            this.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(18.))
                            .rounded_full()
                            .bg(gpui_kit::black().opacity(0.55))
                            .text_color(gpui_kit::white())
                            .text_size(px(9.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("\u{25B6}"),
                    ),
            )
        })
        .into_any_element()
}
