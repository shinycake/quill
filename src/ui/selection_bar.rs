//! The bar that replaces the conversation header while messages are
//! selected: Forward N, Delete N, Copy, Download, Save, Save as HTML,
//! Unpin, Report N and Cancel (tdesktop `TopBarWidget` selection mode).
//! Each button is built in its own function, so a debug build does not
//! reserve all of them in one frame (docs/decisions/main-thread-stack.md).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// The bar, slid `slide` (`0..=1`) of the way in over the header.
    #[inline(never)]
    pub(super) fn selection_bar(
        &self,
        chat_id: ChatId,
        slide: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.frame.motion.selection.borrow().last_count;
        div()
            .id("selection-bar")
            .absolute()
            .left_0()
            .right_0()
            .h_full()
            .top(relative(-(1. - slide)))
            .occlude()
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(selection_forward_button(count, cx))
            .child(selection_delete_button(count, cx))
            .child(selection_copy_button(cx))
            .when(!self.selection_missing_files(chat_id).is_empty(), |bar| {
                bar.child(selection_download_button(cx))
            })
            .when(self.selection_has_media(chat_id), |bar| {
                bar.child(selection_save_button(cx))
            })
            .when(self.selection_html_target(chat_id).is_some(), |bar| {
                bar.child(self.selection_html_button(cx))
            })
            .when(self.selection_unpinnable(chat_id), |bar| {
                bar.child(selection_unpin_button(cx))
            })
            .when(self.selection_reportable(chat_id), |bar| {
                bar.child(selection_report_button(count, cx))
            })
            .child(div().flex_1())
            .child(selection_cancel_button(cx))
            .into_any_element()
    }
}

#[inline(never)]
fn selection_forward_button(count: usize, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-forward")
        .label(tr_with(
            keys::Q_CHAT_FORWARD_2,
            &[("count", &count.to_string())],
        ))
        .primary()
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_forward_picker(window, cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn selection_delete_button(count: usize, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-delete")
        .label(tr_with(
            keys::Q_CHAT_DELETE,
            &[("count", &count.to_string())],
        ))
        .primary()
        .on_click(cx.listener(|this, _, window, cx| {
            this.confirm_delete_selection(window, cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn selection_copy_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-copy")
        .label(tr(keys::Q_CHAT_COPY_AS_TEXT))
        .ghost()
        .tooltip(tr(keys::CONTEXT_COPY_SELECTED_ITEMS))
        .on_click(cx.listener(|this, _, _, cx| {
            this.copy_selection_text(cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn selection_download_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-download")
        .label(tr(keys::Q_CHAT_DOWNLOAD))
        .ghost()
        .tooltip(tr(keys::Q_CHAT_DOWNLOAD_SELECTED))
        .on_click(cx.listener(|this, _, _, cx| {
            this.download_selection(cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn selection_save_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-save")
        .label(tr(keys::SETTINGS_SAVE))
        .ghost()
        .tooltip(tr(keys::Q_CHAT_SAVE_SELECTED))
        .on_click(cx.listener(|this, _, _, cx| {
            this.save_selection(cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn selection_unpin_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-unpin")
        .label(tr(keys::CONTEXT_UNPIN_FROM_TOP))
        .ghost()
        .tooltip(tr(keys::Q_CHAT_UNPIN_SELECTED))
        .on_click(cx.listener(move |this, _, window, cx| {
            if let Some(draft) = this.forward_draft() {
                this.confirm_unpin(draft.from_chat_id, draft.message_ids, window, cx);
            }
        }))
        .into_any_element()
}

#[inline(never)]
fn selection_report_button(count: usize, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-report")
        .label(tr_with(
            keys::Q_CHAT_REPORT_2,
            &[("count", &count.to_string())],
        ))
        .ghost()
        .on_click(cx.listener(|this, _, window, cx| {
            this.report_selection(window, cx);
        }))
        .into_any_element()
}

#[inline(never)]
fn selection_cancel_button(cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("selection-cancel")
        .label(tr(keys::CANCEL))
        .ghost()
        .on_click(cx.listener(|this, _, window, cx| {
            this.clear_forward(window, cx);
        }))
        .into_any_element()
}
