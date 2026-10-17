//! The content of a Files, Music, Voice or Links row in the shared-media
//! list (tdesktop `Overview::Layout::{Document, Voice, Link}`): a leading
//! tile and the lines [`quill::state::SharedMediaMeta`] describes.
#![deny(clippy::indexing_slicing)]

use super::accent;
use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::{SharedMediaItem, SharedMediaLines, SharedMediaMeta};
use quill::telegram::envelope::MessageSender;

/// Side of the leading tile (tdesktop `fileThumbSize` is 70px at full
/// scale; the panel is narrower).
const TILE: f32 = 40.;

impl QuillApp {
    /// The text of a meta row; none for rows that only show their label.
    pub(super) fn shared_media_row_lines(
        &self,
        item: &SharedMediaItem,
    ) -> Option<SharedMediaLines> {
        let session = self.session();
        item.meta.lines(item.date, |sender| {
            let name = session.and_then(|session| session.sender_name_and_accent(sender).0);
            name.filter(|name| !name.is_empty()).unwrap_or_else(|| {
                match sender {
                    Some(MessageSender::Chat { .. }) => tr(keys::Q_REST_A_CHANNEL),
                    _ => tr(keys::Q_REST_A_TELEGRAM_USER),
                }
                .to_string()
            })
        })
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
                .bg(accent())
                .text_color(gpui_kit::white())
                .child(lines.badge.clone()),
            SharedMediaMeta::Link { .. } => tile
                .rounded_md()
                .bg(theme.secondary)
                .text_color(theme.secondary_foreground)
                .text_base()
                .child(lines.badge.clone()),
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
                    .children(lines.links.iter().map(|link| {
                        div()
                            .text_xs()
                            .text_color(accent())
                            .truncate()
                            .child(link.clone())
                    }))
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
}
