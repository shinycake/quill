//! The Retry strip over a thread's or topic's rows when an older page
//! failed (`OlderEnd::Failed`): paging stops there instead of asking again
//! on every render, and the strip is the way out.
use crate::ui::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// Which window of rows the strip retries.
#[derive(Clone, Copy)]
pub(in crate::ui) enum OlderRetryFor {
    Thread,
    Topic,
    /// The open Saved Messages sublist or tag filter.
    Saved,
    /// The main chat history.
    Chat,
}

/// Which window of rows the strip at the newer end retries.
#[derive(Clone, Copy)]
pub(in crate::ui) enum NewerRetryFor {
    Chat,
    Thread,
    Topic,
}

/// `list` with the strip below it while the newer page `failed`.
pub(in crate::ui) fn with_newer_retry(
    list: AnyElement,
    failed: Option<NewerRetryFor>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let Some(which) = failed else {
        return list;
    };
    div()
        .id("newer-load-failed-pane")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(list)
        .child(
            div()
                .id("newer-load-failed")
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_2()
                .py_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_CHAT_COULDN_T_LOAD_MESSAGES)),
                )
                .child(
                    Button::new("newer-retry")
                        .label(tr(keys::Q_CHAT_RETRY))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.retry_newer_page(which, cx);
                        })),
                ),
        )
        .into_any_element()
}

/// `list` with the strip above it while `failed`; the list unchanged
/// otherwise.
pub(in crate::ui) fn with_older_retry(
    list: AnyElement,
    failed: Option<OlderRetryFor>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let Some(which) = failed else {
        return list;
    };
    div()
        .id("older-load-failed-pane")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(
            div()
                .id("older-load-failed")
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_2()
                .py_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_CHAT_COULDN_T_LOAD_MESSAGES)),
                )
                .child(
                    Button::new("older-retry")
                        .label(tr(keys::Q_CHAT_RETRY))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.retry_older_page(which, cx);
                        })),
                ),
        )
        .child(list)
        .into_any_element()
}

impl QuillApp {
    fn retry_older_page(&mut self, which: OlderRetryFor, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = match which {
                OlderRetryFor::Thread => live.driver.retry_thread_older(),
                OlderRetryFor::Topic => live.driver.retry_topic_older(),
                OlderRetryFor::Saved => live.driver.retry_saved_older(),
                OlderRetryFor::Chat => live.driver.retry_history(),
            };
        }
        cx.notify();
    }

    fn retry_newer_page(&mut self, which: NewerRetryFor, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = match which {
                NewerRetryFor::Chat => live.driver.retry_history_newer(),
                NewerRetryFor::Thread => live.driver.retry_thread_newer(),
                NewerRetryFor::Topic => live.driver.retry_topic_newer(),
            };
        }
        cx.notify();
    }
}
