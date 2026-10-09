//! secret-chat lifecycle: peer layer, TTL, close confirm, key verification.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::*;
use base64::Engine as _;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{InfoPanelTarget, Session, unix_ms_now};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChatKind, SecretChatState, format_ttl_setting};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
/// Phase B1: secret chat lifecycle fixture (injected, no live Telegram) —
/// an `updateSecretChat` (Ready) arrives *before* `updateNewChat`
/// (`chatTypeSecret`), exactly as the schema guarantees (td_api.tl line
/// 10740); the chat opens with three E2E messages and the composer live.
/// The 🔒 badge shows in the chat-list row.
/// Phase S1: the new-secret-chat fixture injects NO messages — the real
/// app-rendered end-to-end encryption explainer (`secret_empty_explainer`)
/// shows for an empty secret chat, like TGX's new-secret-chat screen.
pub(super) fn apply_ready_secret_chat(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 41i64;
    let user_id = 41i64;
    let secret_chat_id = 7i32;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":{secret_chat_id},"user_id":{user_id},"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"","layer":144}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypeSecret","secret_chat_id":{secret_chat_id},"user_id":{user_id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
    ];
    // Phase S1: no fake messages are injected — the real app-rendered
    // end-to-end encryption explainer (`secret_empty_explainer`) shows for
    // an empty secret chat, like TGX's new-secret-chat screen.
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

/// Phase B2: key verification UI fixture — the Ready secret chat (id 41)
/// with Zed (user 41), but with a real deterministic 36-byte `key_hash`
/// (base64; the B1 fixture left it empty), opened with E2E history, and
/// Zed's info panel open on the "Encryption key" 12×12 fingerprint grid.
pub(super) fn apply_ready_key_verification(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 41i64;
    let user_id = 41i64;
    let secret_chat_id = 7i32;
    // Deterministic 36-byte fixture (xorshift32, seed 0x9E3779B9) — the
    // grid is fixed across captures. NOT a real key: injected demo data.
    // Built at runtime (no string literal) so the secret scanner never
    // flags the fixture as a leaked key (gitleaks false positive).
    let mut x: u32 = 0x9E3779B9;
    let mut key_hash = [0u8; 36];
    for chunk in key_hash.chunks_mut(4) {
        x ^= x.wrapping_shl(13);
        x ^= x.wrapping_shr(17);
        x ^= x.wrapping_shl(5);
        chunk.copy_from_slice(&x.to_le_bytes());
    }
    let key_hash_b64 = base64::engine::general_purpose::STANDARD.encode(key_hash);
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":{secret_chat_id},"user_id":{user_id},"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"{key_hash_b64}","layer":144}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypeSecret","secret_chat_id":{secret_chat_id},"user_id":{user_id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":401,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000100,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Tap my name above, then compare the key image.","entities":[]}}}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":402,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000160,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"If it matches on both devices, nobody else can read this.","entities":[]}}}}}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
    session.open_info_panel = Some(InfoPanelTarget::User(user_id));
}

/// Phase B3: self-destructing media fixture — a Ready *private* (1:1
/// cloud) chat with Zed (user 41), opened with an incoming photo
/// carrying a live 60s `messageSelfDestructTypeTimer`
/// (`self_destruct_in` 45s at fixture time, so the badge shows a live
/// countdown) and an outgoing
/// `messageSelfDestructTypeImmediately` ("view once") photo. Private —
/// not secret — because TDLib only accepts per-media
/// `self_destruct_type` in `chatTypePrivate` chats (schema 1.8.67
/// lines 6117/6128, "private chats only"). Injected demo data.
pub(super) fn apply_ready_self_destruct(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 11i64;
    let user_id = 41i64;
    let incoming = demo_file_json(91, &demo_thumb_png_path(), true);
    let outgoing = demo_file_json(92, &demo_thumb_png_path(), true);
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypePrivate","user_id":{user_id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":901,"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{incoming},"width":240,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"tap to view — 60s timer","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}},"self_destruct_type":{{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}},"self_destruct_in":45.0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":902,"chat_id":{chat_id},"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{outgoing},"width":240,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"one look only","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}},"self_destruct_type":{{"@type":"messageSelfDestructTypeImmediately"}},"self_destruct_in":0}}}}"#
        ),
        r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

/// Phase B4: chat TTL fixture — the Ready secret chat with Zed (id 41)
/// carrying `message_auto_delete_time` 3600 (1h self-destruct), one
/// message with a live `auto_delete_in` countdown (3595.5s at fixture
/// time, so the chip shows a decaying value), and a
/// `messageChatSetMessageAutoDeleteTime` service row. Injected demo data.
pub(super) fn apply_ready_chat_ttl(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 41i64;
    let user_id = 41i64;
    let secret_chat_id = 7i32;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":{secret_chat_id},"user_id":{user_id},"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"","layer":144}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Zed","type":{{"@type":"chatTypeSecret","secret_chat_id":{secret_chat_id},"user_id":{user_id}}},"unread_count":0,"message_auto_delete_time":3600}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"85","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":601,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000100,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"This one deletes itself an hour after you read it.","entities":[]}}}},"auto_delete_in":3595.5}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":602,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000160,"content":{{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":3600,"from_user_id":{user_id}}}}}}}"#,
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":603,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":999}},"is_outgoing":true,"date":1700000220,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Got it — timer's on.","entities":[]}}}}}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

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
        let record = session.secret_chat_states.get(&secret_chat_id)?;
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
        self.pending_close_secret_chat = Some(chat_id);
        self.status_note = "confirm close secret chat".into();
        cx.notify();
    }

    /// Phase B1: cancel the "Close secret chat" confirm.
    pub(super) fn cancel_close_secret_chat(&mut self, cx: &mut Context<Self>) {
        self.pending_close_secret_chat = None;
        self.status_note = "close cancelled".into();
        cx.notify();
    }

    /// Phase B1: confirm `closeSecretChat` (schema 1.8.67 line 15242).
    /// The state change to `secretChatStateClosed` arrives as
    /// `updateSecretChat`; the composer hides then.
    pub(super) fn confirm_close_secret_chat(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.pending_close_secret_chat.take() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .close_secret_chat(chat_id);
            self.status_note = match result {
                Ok(_) => "closing secret chat…".into(),
                Err(_) => "could not close secret chat".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: secret chat close (no live Telegram)".into();
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
            self.status_note = match result {
                Ok(_) => "creating secret chat…".into(),
                Err(_) => "could not start secret chat".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: secret chat create (no live Telegram)".into();
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
        if self.self_destruct_tick_chat == Some(chat_id) {
            return;
        }
        self.self_destruct_tick_chat = Some(chat_id);
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
                            if this.self_destruct_tick_chat == Some(chat_id) {
                                this.self_destruct_tick_chat = None;
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
        self.ttl_picker_open = false;
        self.ttl_custom_open = false;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_message_auto_delete_time(chat_id, secs);
            self.status_note = match result {
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
            self.status_note = if secs == 0 {
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
