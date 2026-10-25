//! Opening the read-only chat preview from a long press on a chat-list
//! row (slice CL); closing it lives in `close_chat_preview.rs`.

use super::*;

impl QuillApp {
    /// Slice CL: start a long press on a chat-list row — after
    /// `CHAT_PREVIEW_LONG_PRESS` without a release the read-only peek
    /// preview opens; a quicker release stays a plain click.
    pub(in crate::ui) fn begin_chat_preview_press(
        &mut self,
        chat_id: ChatId,
        anchor: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let started = Instant::now();
        self.chat_list.preview_press = Some((chat_id, started));
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Self::CHAT_PREVIEW_LONG_PRESS)
                .await;
            let _ = this.update(cx, |this, cx| {
                // Still the same press (no release, no newer press)?
                if this.chat_list.preview_press == Some((chat_id, started)) {
                    this.open_chat_preview(chat_id, anchor, cx);
                }
            });
        })
        .detach();
    }

    /// Slice CL: open the read-only peek preview for a chat — recent
    /// messages in a floating panel beside the row. Never opens the
    /// chat and never marks anything read.
    pub(in crate::ui) fn open_chat_preview(
        &mut self,
        chat_id: ChatId,
        anchor: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.chat_list.menu = None;
        self.chat_list.preview = Some(ChatPreviewState { chat_id, anchor });
        // Live fetch for unopened chats; already-loaded history renders
        // immediately. Demo/no-driver sessions skip the network.
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_chat_preview_history(chat_id);
        }
        cx.notify();
    }
}
