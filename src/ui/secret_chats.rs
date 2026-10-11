//! secret-chat lifecycle: peer layer, TTL, close confirm, key verification.

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::{Session, unix_ms_now};
use quill::telegram::envelope::{ChatKind, SecretChatState, format_ttl_setting};
use std::time::Duration;

impl QuillApp {
    /// Phase S1: the open secret chat's peer name + `secretChat.layer`
    /// (`secretChat`, schema 1.8.67 line 2816), if the open chat is a
    /// secret chat with a known record. Used for the TGX
    /// `SecretChatFeatureUnsupported` gate (round videos need layer ≥ 66).
    pub(super) fn open_secret_chat_peer_layer(&self) -> Option<(String, i32)> {
        let session = self.session()?;
        let open = session.open_chat?;
        let chat = session.chats.get(&open.0)?;
        let secret_chat_id = chat.secret_chat_id()?;
        let record = session
            .users_state
            .secret_chat_states
            .get(&secret_chat_id)?;
        let name = Self::secret_peer_name(Some(session), record.user_id, "Your contact");
        Some((name, record.layer))
    }

    /// Phase S2: the open chat is a secret chat (TGX's
    /// `ChatId.isSecret` check in the alert trigger).
    pub(super) fn open_chat_is_secret(&self) -> bool {
        let session = self.session();
        session
            .as_ref()
            .and_then(|s| s.open_chat)
            .and_then(|id| session.as_ref()?.chats.get(&id.0))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
    }

    /// Phase B1: open the "Close secret chat" confirm banner for the
    /// given secret chat.
    pub(super) fn open_close_secret_chat_confirm(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        self.chat_list.pending_close_secret_chat = Some(chat_id);
        self.connection.status_note = "confirm close secret chat".into();
        cx.notify();
    }

    /// Phase B1: cancel the "Close secret chat" confirm.
    pub(super) fn cancel_close_secret_chat(&mut self, cx: &mut Context<Self>) {
        self.chat_list.pending_close_secret_chat = None;
        self.connection.status_note = "close cancelled".into();
        cx.notify();
    }

    /// Phase B1: confirm `closeSecretChat` (schema 1.8.67 line 15242).
    /// The state change to `secretChatStateClosed` arrives as
    /// `updateSecretChat`; the composer hides then.
    pub(super) fn confirm_close_secret_chat(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.chat_list.pending_close_secret_chat.take() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .close_secret_chat(chat_id);
            self.connection.status_note = match result {
                Ok(_) => "closing secret chat…".into(),
                Err(_) => "could not close secret chat".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo: secret chat close (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase S1: eligibility for a new secret chat — non-bot user, not
    /// yourself. Shared by the profile button and the "New secret chat"
    /// contact picker.
    pub(super) fn can_start_secret_chat_with(session: &Session, user_id: i64) -> bool {
        session.user(user_id).is_some_and(|u| !u.is_bot)
            && session.my_user_id.is_none_or(|me| me != user_id)
    }

    /// Phase B1: `createNewSecretChat` from a user profile. The new chat
    /// opens when its `updateNewChat` arrives; the state (Pending →
    /// Ready) arrives as `updateSecretChat`.
    pub(super) fn start_secret_chat_for_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .start_secret_chat(user_id);
            self.connection.status_note = match result {
                Ok(_) => "creating secret chat…".into(),
                Err(_) => "could not start secret chat".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo: secret chat create (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase B3: keep the self-destruct countdown badges re-rendering
    /// while the open chat has a live `self_destruct_in` timer. At most
    /// one task per open chat (guarded by `self_destruct_tick_chat`,
    /// mirroring the Phase A1 slow-mode tick); it exits when no timer is
    /// live or the open chat changes. Called from `render`, which has the
    /// `&mut self` the tick needs.
    pub(super) fn ensure_self_destruct_tick(&mut self, cx: &mut Context<Self>) {
        let now_ms = unix_ms_now();
        let Some(chat_id) = self
            .session()
            .and_then(|session| session.open_chat)
            .filter(|_| {
                self.session()
                    .is_some_and(|session| session.open_chat_has_live_self_destruct(now_ms))
            })
        else {
            return;
        };
        if self.history.self_destruct_tick_chat == Some(chat_id) {
            return;
        }
        self.history.self_destruct_tick_chat = Some(chat_id);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let now_ms = unix_ms_now();
                        let still_open = this.session().and_then(|s| s.open_chat) == Some(chat_id);
                        let still_live = this.session().is_some_and(|session| {
                            session.open_chat_has_live_self_destruct(now_ms)
                        });
                        if still_open && still_live {
                            cx.notify();
                            true
                        } else {
                            if this.history.self_destruct_tick_chat == Some(chat_id) {
                                this.history.self_destruct_tick_chat = None;
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

    /// Phase B4: apply a chat TTL choice (`setChatMessageAutoDeleteTime`,
    /// schema 1.8.67 line 13454). Live: the driver validates the value
    /// rule and sends; the new value arrives as
    /// `updateChatMessageAutoDeleteTime` (no optimistic state change).
    /// Demo: apply the same update through the reducer so the screenshot
    /// fixture shows the new timer immediately.
    pub(super) fn apply_chat_ttl(&mut self, chat_id: ChatId, secs: i32, cx: &mut Context<Self>) {
        self.notify.ttl_picker_open = false;
        self.notify.ttl_custom_open = false;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_message_auto_delete_time(chat_id, secs);
            self.connection.status_note = match result {
                Ok(_) if secs == 0 => "turning off timer…".into(),
                Ok(_) => "setting timer…".into(),
                Err(_) => "could not change timer".into(),
            };
            cx.notify();
            return;
        }
        if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.message_auto_delete_time = secs;
            }
            self.connection.status_note = if secs == 0 {
                "timer off".into()
            } else {
                format!("timer {}", format_ttl_setting(secs))
            };
            cx.notify();
        }
    }

    /// Phase S1: "Custom notification settings for the secret chat with
    /// {name}." (TGX `NotificationChannelSecretChat`) — `Some` only when
    /// the open chat is a secret chat with a known peer.
    pub(super) fn secret_notif_label(session: Option<&Session>) -> Option<String> {
        let session = session?;
        let open = session.open_chat?;
        let chat = session.chats.get(&open.0)?;
        let ChatKind::Secret { user_id, .. } = &chat.kind else {
            return None;
        };
        let name = Self::secret_peer_name(Some(session), user_id.0, "your contact");
        Some(format!(
            "Custom notification settings for the Secret Chat with {name}."
        ))
    }

    /// Phase B1: "Close secret chat" confirm banner, styled like the
    /// delete confirm. Closing is permanent — the chat can never send
    /// again once `secretChatStateClosed` lands.
    ///
    /// Phase S1: distinct TGX copy per secret-chat state
    /// (`DeleteSecretChatPendingConfirm` / `DeleteSecretChatClosedConfirm` /
    /// `DeleteSecretChatConfirm`).
    pub(super) fn close_secret_chat_confirm_banner(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (title, body, confirm_label) = self.secret_close_copy();
        div()
            .id("close-secret-confirm")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(danger())
            .bg(danger_bg())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(danger())
                            .child(title),
                    )
                    .child(div().text_sm().text_color(text_primary()).child(body)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Button::new("cancel-close-secret").label("Cancel").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.cancel_close_secret_chat(cx);
                        }),
                    ))
                    .child(
                        Button::new("confirm-close-secret")
                            .label(confirm_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_close_secret_chat(cx);
                            })),
                    ),
            )
    }

    /// Phase S1: close-confirm copy keyed on the secret chat's state, with
    /// the peer's display name (TGX `DeleteSecretChat{Pending,Closed,}`
    /// `Confirm` strings, verbatim).
    pub(super) fn secret_close_copy(&self) -> (String, String, &'static str) {
        let session = self.session();
        let user_id = self
            .chat_list
            .pending_close_secret_chat
            .and_then(|id| session.as_ref()?.chats.get(&id.0))
            .and_then(|chat| match &chat.kind {
                ChatKind::Secret { user_id, .. } => Some(user_id.0),
                _ => None,
            });
        let name = user_id.map_or_else(
            || "your contact".to_string(),
            |id| Self::secret_peer_name(session, id, "your contact"),
        );
        let state = self
            .chat_list
            .pending_close_secret_chat
            .and_then(|id| session.as_ref()?.chats.get(&id.0))
            .and_then(|chat| chat.secret_state.clone());
        match state {
            Some(SecretChatState::Pending) => (
                "Cancel this secret chat?".to_string(),
                format!("Are you sure you want to cancel the secret chat with {name}?"),
                "Cancel chat",
            ),
            Some(SecretChatState::Closed) => (
                "Delete this secret chat?".to_string(),
                format!(
                    "Are you sure you want to delete the secret chat with {name}? \
                     This action cannot be undone."
                ),
                "Delete",
            ),
            _ => (
                "Close this secret chat?".to_string(),
                format!(
                    "Are you sure you want to delete the secret chat with {name}? \
                     All chat history will be deleted forever. This action cannot be undone."
                ),
                "Close chat",
            ),
        }
    }

    /// Phase S1: peer display name for a secret chat — the user's
    /// `display_name()`, empty-filtered, with a caller-chosen fallback
    /// (the fallback wording varies by context, so it stays a parameter).
    pub(super) fn secret_peer_name(
        session: Option<&Session>,
        user_id: i64,
        fallback: &str,
    ) -> String {
        session
            .and_then(|s| s.user(user_id))
            .map(|u| u.display_name())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    }

    /// Phase S1: "Waiting for {name} to get online…" header subtitle for a
    /// Pending secret chat (TGX `AwaitingEncryption`). `None` for anything
    /// else — the composer note covers the bottom of the pane, this covers
    /// the header like TGX's chat subtitle.
    pub(super) fn secret_pending_subtitle(&self, chat_id: ChatId) -> Option<String> {
        let session = self.session()?;
        let chat = session.chats.get(&chat_id.0)?;
        let ChatKind::Secret { user_id, .. } = &chat.kind else {
            return None;
        };
        if chat.secret_state != Some(SecretChatState::Pending) {
            return None;
        }
        let name = Self::secret_peer_name(Some(session), user_id.0, "your contact");
        Some(format!("Waiting for {name} to get online…"))
    }

    /// Phase B1: the composer note for a secret chat that can't send —
    /// Pending ("Waiting for X to come online…", schema 1.8.67 line 2797:
    /// "waiting for the other user to get online"), Closed ("Secret chat
    /// closed"), or still resolving ("Loading secret chat…"). `None`
    /// when the open chat is not a secret chat or is Ready (the composer
    /// shows then).
    pub(super) fn secret_composer_note(&self) -> Option<String> {
        let session = self.session()?;
        let open = session.open_chat?;
        let chat = session.chats.get(&open.0)?;
        let ChatKind::Secret { user_id, .. } = &chat.kind else {
            return None;
        };
        match &chat.secret_state {
            Some(SecretChatState::Ready) => None,
            Some(SecretChatState::Pending) => {
                let name = Self::secret_peer_name(Some(session), user_id.0, "your contact");
                Some(format!("🔒 Waiting for {name} to come online…"))
            }
            Some(SecretChatState::Closed) => Some("🔒 Secret chat closed".to_string()),
            Some(SecretChatState::Unknown(_)) | None => Some("🔒 Loading secret chat…".to_string()),
        }
    }

    /// Phase S1: empty-secret-chat end-to-end encryption explainer (TGX
    /// `MessagesHolder` TYPE_SECRET_CHAT_INFO: "Secret Chats" header plus
    /// the four `EncryptedDescription` bullets).
    pub(super) fn secret_empty_explainer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bullets = [
            "Use end-to-end encryption",
            "Leave no trace on our servers",
            "Have a self-destruct timer",
            "Do not allow forwarding",
        ];
        let mut list = div().flex().flex_col().gap_1();
        for bullet in bullets {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("• {bullet}")),
            );
        }
        div()
            .id("secret-e2e-explainer")
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .child(div().text_lg().font_semibold().child("🔒 Secret chats"))
            .child(list)
    }
}
