//! The send card: files picked for the open chat, shown above the message
//! field until they are sent (tdesktop `SendFilesBox`, kept inline so the
//! chat stays in view and the field is the caption). A header with what is
//! being sent, the choices that differ from the defaults and the options
//! menu; the photos and videos laid out as the messages they will be; the
//! files as rows.
#![deny(clippy::indexing_slicing)]

#[cfg(feature = "demo-capture")]
mod demo;
mod header;
mod layout;
mod tiles;
mod video_preview;

pub(crate) use video_preview::VideoPreview;
pub(in crate::ui) use video_preview::discard as discard_preview;

use crate::ui::app::QuillApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use layout::TrayLayout;

impl QuillApp {
    pub(in crate::ui) fn send_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let layout = TrayLayout::of(
            self.pending_attachments(),
            self.composer_group_media_effective(),
        );
        let theme = cx.theme();
        let (border, surface) = (theme.border, theme.background);
        div()
            .id("send-card")
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(surface)
            .child(self.send_card_header(cx))
            .children(self.send_card_media(&layout, cx))
            .children(self.send_card_files(&layout, cx))
            .into_any_element()
    }
}
