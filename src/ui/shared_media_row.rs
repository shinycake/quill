//! The content of a Files, Music, Voice or Links row in the shared-media
//! list (tdesktop `Overview::Layout::{Document, Voice, Link}`): a leading
//! tile and the lines [`quill::state::SharedMediaMeta`] describes.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::{accent, danger, success, warning};
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_time::now_unix;
use quill::state::{LinkLine, SharedMediaItem, SharedMediaLines, SharedMediaMeta};
use quill::telegram::envelope::MessageSender;
use quill::text::LinkTarget;

/// Side of the leading tile (tdesktop `fileThumbSize` is 70px at full
/// scale; the panel is narrower).
const TILE: f32 = 40.;

impl QuillApp {
    /// A Music or Voice row's disc: pause while it plays, a cross while it
    /// downloads (a press cancels), play otherwise.
    fn shared_track_icon(&self, item: &SharedMediaItem) -> IconName {
        let message = Some(item.message_id);
        let active =
            self.playback.playing_voice == message || self.playback.playing_audio == message;
        if active && self.playback.is_playing() {
            return IconName::Pause;
        }
        let fetching = self.session().is_some_and(|session| {
            super::message_media::file_is_downloading(
                item.file,
                &session.media.files,
                &session.media.downloads,
            )
        });
        if fetching {
            IconName::X
        } else {
            IconName::Play
        }
    }

    /// The text of a meta row; none for rows that only show their label.
    pub(super) fn shared_media_row_lines(
        &self,
        item: &SharedMediaItem,
    ) -> Option<SharedMediaLines> {
        let session = self.session();
        let named = |name: Option<String>, sender: Option<MessageSender>| {
            name.filter(|name| !name.is_empty()).unwrap_or_else(|| {
                match sender {
                    Some(MessageSender::Chat { .. }) => tr(keys::Q_REST_A_CHANNEL),
                    _ => tr(keys::Q_REST_A_TELEGRAM_USER),
                }
                .to_string()
            })
        };
        item.meta.lines(
            item.date,
            now_unix(),
            |sender| {
                let name = session.and_then(|session| session.sender_name_and_accent(sender).0);
                named(name, sender)
            },
            |origin| {
                let name = session.and_then(|session| session.origin_name_and_accent(origin).0);
                named(name, None)
            },
        )
    }

    /// Tile and text of a meta row.
    pub(super) fn shared_media_row_body(
        &self,
        item: &SharedMediaItem,
        lines: SharedMediaLines,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let tile = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(TILE))
            .text_xs()
            .font_semibold();
        let tile = match &item.meta {
            SharedMediaMeta::File { .. } => tile
                .rounded_md()
                .bg(match lines.tone {
                    1 => success(),
                    2 => danger(),
                    3 => warning(),
                    _ => accent(),
                })
                .text_color(gpui_kit::white())
                .child(lines.badge.clone()),
            SharedMediaMeta::Link { .. } => tile
                .rounded_md()
                .bg(theme.secondary)
                .text_color(theme.secondary_foreground)
                .text_base()
                .child(lines.badge.clone()),
            SharedMediaMeta::Music { .. } | SharedMediaMeta::Voice { .. } => tile
                .rounded_full()
                .bg(accent())
                .text_color(gpui_kit::white())
                .child(
                    Icon::new(self.shared_track_icon(item))
                        .size(px(18.))
                        .text_color(gpui_kit::white()),
                ),
            _ => tile
                .rounded_full()
                .bg(accent())
                .text_color(gpui_kit::white())
                .text_lg()
                .child(item.glyph),
        };
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;
        div()
            .flex()
            .items_start()
            .gap_3()
            .flex_1()
            .min_w_0()
            .child(tile)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(foreground)
                            .truncate()
                            .child(lines.title),
                    )
                    .when(!lines.description.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(foreground)
                                .line_clamp(3)
                                .child(lines.description.clone()),
                        )
                    })
                    .children(
                        lines
                            .links
                            .iter()
                            .enumerate()
                            .map(|(ix, link)| self.shared_media_link_line(item, ix, link, cx)),
                    )
                    .when(!lines.status.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(lines.status.clone()),
                        )
                    })
                    .when(!lines.date.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(lines.date.clone()),
                        )
                    }),
            )
    }

    /// One listed address: its own click, through the same safety check as
    /// a link in a chat (tdesktop gives every line its own handler).
    fn shared_media_link_line(
        &self,
        item: &SharedMediaItem,
        ix: usize,
        link: &LinkLine,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let line = div()
            .text_xs()
            .text_color(accent())
            .truncate()
            .child(link.display.clone());
        let (Some(url), Some(chat_id)) = (
            link.url.clone(),
            self.session()
                .and_then(|session| session.media.shared_media.chat_id),
        ) else {
            return line.into_any_element();
        };
        let message_id = item.message_id;
        // The chat's rule: the text shown is checked against the address.
        let label = Some(link.display.clone());
        line.id(("shared-media-link", message_id.0 as u64 * 8 + ix as u64))
            .debug_selector(move || format!("shared-media-link-{ix}"))
            .role(gpui_kit::Role::Link)
            .aria_label(link.display.clone())
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.queue_link(
                    LinkTarget::Url {
                        url: url.clone(),
                        label: label.clone(),
                    },
                    (chat_id.0, message_id.0 as u64),
                    cx,
                );
            }))
            .into_any_element()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "shared_media_row_tests.rs"]
mod tests;
