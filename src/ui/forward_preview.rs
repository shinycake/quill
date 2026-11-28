//! What a forward carries, drawn small: the pictures of its media and the
//! "from" and "what" lines of tdesktop's `ForwardPanel`
//! (`history/view/controls/history_view_forward_panel.cpp`). The forward
//! bar shows them beside its buttons and the forward box above its chat
//! list, so the chats are chosen with the forwarded messages in view.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::share_box::forward_bar_lines;

/// The forward box's pictures (tdesktop's 32 px reply preview is the
/// bar's; the box has room for larger ones).
const BOX_THUMB: f32 = 44.;
/// The bar's pictures (`st::historyReplyPreview`).
const BAR_THUMB: f32 = 32.;

impl QuillApp {
    /// Who the forwarded messages are from and what they are: the sender
    /// line ("Sender names removed" when hidden) and the first message's
    /// preview, or the count when there are several.
    pub(super) fn forward_lines(&self) -> (String, String) {
        let Some(draft) = self.forward_draft() else {
            return (String::new(), String::new());
        };
        let Some(session) = self.session() else {
            return (String::new(), String::new());
        };
        let senders = session.forward_bar_senders(draft.from_chat_id, &draft.message_ids);
        let preview = draft
            .message_ids
            .first()
            .map(|id| session.forward_bar_preview(draft.from_chat_id, *id))
            .unwrap_or_default();
        forward_bar_lines(&senders, draft.count(), draft.send_copy, &preview)
    }

    /// Up to `most` pictures of the forwarded media, `size` square, then
    /// "+N" for the messages they leave out. `None` when nothing forwarded
    /// has a picture (text, files without a preview, spoilers).
    fn forward_thumbs(&self, key: &'static str, size: f32, most: usize, cx: &App) -> Option<Div> {
        let draft = self.forward_draft()?;
        let thumbs: Vec<AnyElement> = draft
            .message_ids
            .iter()
            .filter_map(|id| {
                let thumb = self.message_thumb(draft.from_chat_id, *id, key, size, cx)?;
                let selector = format!("{key}-{}", id.0);
                Some(
                    div()
                        .debug_selector(move || selector.clone())
                        .child(thumb)
                        .into_any_element(),
                )
            })
            .take(most)
            .collect();
        if thumbs.is_empty() {
            return None;
        }
        let rest = draft.count().saturating_sub(thumbs.len());
        let theme = cx.theme();
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .children(thumbs)
                .when(rest > 0, |this| {
                    this.child(
                        div()
                            .h(px(size))
                            .min_w(px(size))
                            .px_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .bg(theme.muted)
                            .text_xs()
                            .font_medium()
                            .text_color(theme.muted_foreground)
                            .child(format!("+{rest}")),
                    )
                }),
        )
    }

    /// The forward bar's pictures, left of its lines.
    pub(super) fn forward_bar_thumbs(&self, cx: &App) -> Option<Div> {
        self.forward_thumbs("forward-bar-thumb", BAR_THUMB, 3, cx)
    }

    /// The card above the forward box's search: the forwarded messages'
    /// pictures and the bar's two lines, in the reply-quote look the bar
    /// has (accent edge, subtle fill).
    pub(super) fn forward_box_preview(&self, cx: &App) -> Option<AnyElement> {
        self.forward_draft()?;
        let (from, text) = self.forward_lines();
        let theme = cx.theme();
        let thumbs = self.forward_thumbs("forward-box-thumb", BOX_THUMB, 4, cx);
        Some(
            div()
                .debug_selector(|| "forward-box-preview".to_string())
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .rounded_md()
                .border_l_2()
                .border_color(theme.primary)
                .bg(theme.muted.opacity(0.5))
                .children(thumbs)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .font_medium()
                                .text_color(theme.primary)
                                .child(super::bidi_line::name_line(from)),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(super::bidi_line::one_line_plain(text)),
                        ),
                )
                .into_any_element(),
        )
    }
}
