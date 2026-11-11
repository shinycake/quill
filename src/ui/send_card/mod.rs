//! The send card: files picked for the open chat, shown above the message
//! field until they are sent (tdesktop `SendFilesBox`, kept inline so the
//! chat stays in view and the field is the caption). Two columns in one
//! card that is as wide as what it holds: the photos and videos laid out
//! as the messages they will be, with the files as rows under them, and
//! next to them every send option as a labelled control. A chat too narrow
//! for both puts the options above the previews.
#![deny(clippy::indexing_slicing)]

#[cfg(feature = "demo-capture")]
mod demo;
mod layout;
mod options;
#[cfg(feature = "demo-capture")]
mod styles;
mod tiles;
mod video_preview;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

pub(crate) use video_preview::VideoPreview;
pub(in crate::ui) use video_preview::discard as discard_preview;

use crate::ui::app::QuillApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use layout::{COLUMN_GAP, CardRoom, CardShape, TrayLayout};

impl QuillApp {
    pub(in crate::ui) fn send_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let room = CardRoom::for_pane(f32::from(self.history.measure.get().pane()));
        // The two designs kept as prototypes (`styles.rs`).
        #[cfg(feature = "demo-capture")]
        let room = match styles::SendStyle::chosen() {
            styles::SendStyle::Beside => room,
            styles::SendStyle::Box => return div().into_any_element(),
            styles::SendStyle::Strip => CardRoom {
                shape: CardShape::Stacked,
                ..room
            },
        };
        let layout = TrayLayout::of(
            self.pending_attachments(),
            self.composer_group_media_effective(),
        );
        let theme = cx.theme();
        let (border, surface) = (theme.border, theme.background);
        let previews = div()
            .id("send-card-previews")
            .debug_selector(|| "send-card-previews".to_string())
            .flex()
            .flex_col()
            .gap_2()
            .min_w_0()
            .children(self.send_card_media(&layout, room, cx))
            .children(self.send_card_files(&layout, room, cx));
        let options = self.send_card_options(room.shape, cx);
        div()
            .id("send-card")
            .debug_selector(|| "send-card".to_string())
            .flex()
            .gap(px(COLUMN_GAP))
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(surface)
            .map(|card| match room.shape {
                // As wide as its columns, not the chat.
                CardShape::Beside => card
                    .self_start()
                    .max_w_full()
                    .items_start()
                    .child(previews.flex_none())
                    .child(options),
                CardShape::Stacked => card.flex_col().child(options).child(previews),
            })
            .into_any_element()
    }
}
