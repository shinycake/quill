//! slow-mode helpers.

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::unix_ms_now;
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind};
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
        self.connection.status_note = format!("Slow mode: wait {wait}s before sending");
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
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let still_open = this.session().and_then(|s| s.open_chat) == Some(chat_id);
                        let still_gated = this.slow_mode_wait_secs_for(chat_id).is_some();
                        if still_open && still_gated {
                            cx.notify();
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

    /// Phase A1: whether the viewer may change slow mode in a supergroup:
    /// creator, or administrator with the explicit `can_restrict_members`
    /// right (`setChatSlowModeDelay` requirement, schema 1.8.67 line
    /// 13551). Shared by the info-panel selector and `set_slow_mode_delay`
    /// (defense in depth — the request must never fire unauthorized).
    pub(super) fn can_change_slow_mode(&self, supergroup_id: i64) -> bool {
        let session = match self.session() {
            Some(session) => session,
            None => return false,
        };
        let status = session.supergroup_own_status(supergroup_id);
        status == Some(ChannelMemberStatus::Creator)
            || (status == Some(ChannelMemberStatus::Administrator)
                && session.supergroup_can_restrict_members(supergroup_id))
    }

    /// Phase A1: admin slow-mode control (`setChatSlowModeDelay`, TDLib
    /// 1.8.67 line 13551) from the group info panel. The new delay
    /// arrives via `updateSupergroupFullInfo`; the panel re-renders then.
    pub(super) fn set_slow_mode_delay(
        &mut self,
        chat_id: ChatId,
        slow_mode_delay: i32,
        cx: &mut Context<Self>,
    ) {
        let supergroup_id = self
            .session()
            .and_then(|s| match s.chats.get(&chat_id.0)?.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => Some(supergroup_id),
                _ => None,
            });
        if supergroup_id.is_none_or(|id| !self.can_change_slow_mode(id)) {
            self.connection.status_note = "slow mode needs the restrict-members admin right".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .set_chat_slow_mode_delay(chat_id, slow_mode_delay)
            {
                Ok(_) => self.connection.status_note = "slow mode updated".into(),
                Err(_) => self.connection.status_note = "could not change slow mode".into(),
            }
        } else {
            self.connection.status_note = "slow mode needs a live connection (demo)".into();
        }
        cx.notify();
    }
}
