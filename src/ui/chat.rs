//! slow-mode helpers.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::unix_ms_now;
use quill::telegram::envelope::ChatKind;
use std::time::Duration;

impl QuillApp {
    /// Phase A1: slow-mode wait (whole seconds) for a chat, via the
    /// session gate (`Session::slow_mode_wait_secs`).
    pub(super) fn slow_mode_wait_secs_for(&self, chat_id: ChatId) -> Option<u64> {
        let session = self.session()?;
        session.slow_mode_wait_secs(chat_id, unix_ms_now())
    }

    /// Phase A1: slow-mode wait for the currently open chat, if any.
    pub(super) fn slow_mode_wait_secs(&self) -> Option<u64> {
        let chat_id = self.session()?.open_chat?;
        self.slow_mode_wait_secs_for(chat_id)
    }

    /// Phase A1: centralized slow-mode send gate. When the gate is active
    /// for `chat_id`, sets the status note and returns `true` — callers
    /// must not send. On live sessions a blocked send also re-reads the
    /// server value via `refresh_supergroup_full_info`, because the schema
    /// (1.8.67, line 2759) warns no `updateSupergroupFullInfo` fires when
    /// only the expiry changes while old and new are non-zero.
    pub(super) fn slow_mode_blocked(&mut self, chat_id: ChatId, cx: &mut Context<Self>) -> bool {
        let Some(wait) = self.slow_mode_wait_secs_for(chat_id) else {
            return false;
        };
        self.connection.set_note(StatusNote::info(tr_with(
            keys::Q_REST_SLOW_MODE_WAIT_S_BEFORE,
            &[("wait", &wait.to_string())],
        )));
        if self.live.is_some() {
            let supergroup_id = self.live.as_ref().and_then(|live| {
                let session = &live.driver.session;
                match session.chats.get(&chat_id.0)?.kind {
                    ChatKind::Supergroup {
                        supergroup_id,
                        is_channel: false,
                    } => Some(supergroup_id),
                    _ => None,
                }
            });
            if let (Some(live), Some(supergroup_id)) = (self.live.as_mut(), supergroup_id) {
                let _ = live.driver.refresh_supergroup_full_info(supergroup_id);
            }
        }
        cx.notify();
        true
    }

    /// Slow mode lets one Send make one message (tdesktop
    /// `lng_slowmode_no_many`): a Send of `messages` more is refused before
    /// anything goes, so the tray and the text stay.
    pub(super) fn slow_mode_refuses(
        &mut self,
        chat_id: ChatId,
        messages: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if messages <= 1 || !self.session().is_some_and(|s| s.slow_mode_applies(chat_id)) {
            return false;
        }
        self.connection
            .set_note(StatusNote::info(tr(keys::SLOWMODE_NO_MANY)));
        cx.notify();
        true
    }

    /// Phase A1: keep the slow-mode countdown re-rendering while the open
    /// chat is gated. At most one task per open chat (guarded by
    /// `slow_mode_tick_chat`, mirroring `spawn_voice_tick`); it exits
    /// when the gate lifts or the open chat changes. Called from
    /// `render`, which has the `&mut self` the tick needs.
    pub(super) fn ensure_slow_mode_tick(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self
            .session()
            .and_then(|session| session.open_chat)
            .filter(|id| self.slow_mode_wait_secs_for(*id).is_some())
        else {
            return;
        };
        if self.composer_ui.slow_mode_tick_chat == Some(chat_id) {
            return;
        }
        self.composer_ui.slow_mode_tick_chat = Some(chat_id);
        self.spawn_scoped(cx, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let still_open = this.session().and_then(|s| s.open_chat) == Some(chat_id);
                        let still_gated = this.slow_mode_wait_secs_for(chat_id).is_some();
                        if still_open {
                            this.notify_tick(super::app_slice::SliceKind::Composer, cx);
                        }
                        if still_open && still_gated {
                            true
                        } else {
                            if this.composer_ui.slow_mode_tick_chat == Some(chat_id) {
                                this.composer_ui.slow_mode_tick_chat = None;
                            }
                            false
                        }
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
        })
        .detach();
    }
}
