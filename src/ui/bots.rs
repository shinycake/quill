//! bot panels, callbacks, game buttons, login URLs, command menus.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::lean_buttons::ghost_text_button;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::state::{RequestPurpose, Session, SimilarBotsFetch};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{BotInfo, CallbackQueryAnswer};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyBotChat` fixture (Phase 3.1): a private chat with a bot user —
/// `updateUser` marks user 21 `userTypeBot` (schema 1.8.67 line 816) —
/// opened with history, plus a `getUserFullInfo` round-trip whose
/// `userFullInfo` response caches `botInfo` (description + commands) so the
/// bot panel renders under the header. Bot chats ride the ordinary
/// private-chat path, so the composer stays visible.
pub(super) fn apply_ready_bot_chat(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(21));
    let info_extra = session.request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#
            .to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hi! I'm Demo Bot. Tap a command below to try it.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"/start","entities":[]}}}}"#
            .to_string(),
        format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"Demo Bot answers questions and shows how the info panel looks. It understands /start, /help and /ping.","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}},{{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}},{{"@type":"botCommand","command":"ping","description":"Check latency (ephemeral)","is_ephemeral":true}}]}}}}"#,
            info_extra.0,
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyBotKeyboard` fixture (Phase 3.2, extended in B1): a bot chat
/// exercising every B1 keyboard path — inline buttons (url / callback /
/// password-callback / game / user / web-app / unsupported buy), a
/// `messageGame` for the game payload, a custom keyboard with one-time
/// semantics, and a force-reply message that arms the composer.
pub(super) fn apply_ready_bot_keyboard(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(21));
    // The chat must exist before its messages arrive (mirrors
    // `apply_ready_bot_chat`); without these updates the fixture renders
    // "No chat selected".
    let chat_jsons = [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#,
    ];
    for raw in chat_jsons {
        if let Some(owned) = copy_and_parse(raw, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let bot_rows = r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Open site","type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com/"}},{"@type":"inlineKeyboardButton","text":"Vote","type":{"@type":"inlineKeyboardButtonTypeCallback","data":"dm90ZTox"}}],[{"@type":"inlineKeyboardButton","text":"Secret","type":{"@type":"inlineKeyboardButtonTypeCallbackWithPassword","data":"c2VjcmV0OjE="}},{"@type":"inlineKeyboardButton","text":"Play","type":{"@type":"inlineKeyboardButtonTypeCallbackGame"}},{"@type":"inlineKeyboardButton","text":"Bot info","type":{"@type":"inlineKeyboardButtonTypeUser","user_id":7}}],[{"@type":"inlineKeyboardButton","text":"Mini app","type":{"@type":"inlineKeyboardButtonTypeWebApp","url":"https://example.com/app"}},{"@type":"inlineKeyboardButton","text":"Buy","type":{"@type":"inlineKeyboardButtonTypeBuy"}}]]},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick an inline action:","entities":[]}}}}"#;
    let game_message = r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Play chess","type":{"@type":"inlineKeyboardButtonTypeCallbackGame"}}]]},"content":{"@type":"messageGame","game":{"@type":"game","id":"901","short_name":"chess","title":"Chess","text":{"@type":"formattedText","text":"Challenge me!","entities":[]},"description":"A classic.","photo":null,"animation":null}}}}"#;
    let login_message = r#"{"@type":"updateNewMessage","message":{"id":303,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Log in","type":{"@type":"inlineKeyboardButtonTypeLoginUrl","id":11,"url":"https://example.com/login","forward_text":"Log in to Example","bot_username":"demo_bot","request_write_access":false}}]]},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Log in to continue:","entities":[]}}}}"#;
    let keyboard_message = r#"{"@type":"updateNewMessage","message":{"id":304,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[[{"@type":"keyboardButton","text":"Yes","type":{"@type":"keyboardButtonTypeText"}},{"@type":"keyboardButton","text":"No","type":{"@type":"keyboardButtonTypeText"}}],[{"@type":"keyboardButton","text":"Share phone","type":{"@type":"keyboardButtonTypeRequestPhoneNumber"}},{"@type":"keyboardButton","text":"Mini app","type":{"@type":"keyboardButtonTypeWebApp","url":"https://example.com/app"}}]],"is_persistent":false,"resize_keyboard":true,"one_time":true,"is_personal":false,"force_reply":false,"input_field_placeholder":"Choose…"},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Choose one:","entities":[]}}}}"#;
    let force_reply_message = r#"{"@type":"updateNewMessage","message":{"id":306,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupForceReply","input_field_placeholder":"Type your name…"},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"What is your name?","entities":[]}}}}"#;
    for raw in [
        bot_rows,
        game_message,
        login_message,
        keyboard_message,
        force_reply_message,
    ] {
        if let Some(owned) = copy_and_parse(raw, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// Phase S2: the inline-bot warning banner shown above the composer
    /// before a `SwitchInline` query is inserted in a secret chat. TGX's
    /// `SecretChatContextBotAlert` copy verbatim
    /// (`app/src/main/res/values/strings.xml:2638`); single Confirm, no
    /// Cancel (TGX `ALERT_NO_CANCEL`).
    pub(super) fn inline_bot_alert_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("inline-bot-alert")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(warning_bright())
            .bg(warning_bg())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(warning_bright())
                            .child(tr(keys::Q_MEDIA_QUILL)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_primary())
                            .child(tr(keys::Q_MEDIA_PLEASE_NOTE_THAT_INLINE_BOTS)),
                    ),
            )
            .child(
                Button::new("confirm-inline-bot-alert")
                    .label(tr(keys::Q_MEDIA_CONFIRM))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.confirm_inline_bot_alert(window, cx);
                    })),
            )
    }

    /// Phase 3.2: show a `callbackQueryAnswer` in the status line (the
    /// transient feedback surface this app has). A URL answer opens in the
    /// OS browser, like message-text links; `show_alert` has no modal yet,
    /// so its text lands in the status line too.
    pub(super) fn present_callback_answer(
        &mut self,
        answer: CallbackQueryAnswer,
        cx: &mut Context<Self>,
    ) {
        if answer.url.is_empty() {
            self.connection.set_note(if answer.text.is_empty() {
                StatusNote::silent(tr(keys::Q_MEDIA_BOT_ANSWERED))
            } else {
                StatusNote::info(answer.text)
            });
            cx.notify();
        } else {
            let open = quill::platform::open_external_url(&answer.url);
            self.when_os_answers(open, cx, |this, opened, cx| {
                this.connection.set_note(if opened {
                    StatusNote::info(tr(keys::Q_MEDIA_OPENED_LINK))
                } else {
                    StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_OPEN_LINK))
                });
                cx.notify();
            });
        }
    }

    /// B1: login-URL button (`inlineKeyboardButtonTypeLoginUrl`, schema
    /// 1.8.67 line 3780). Resolves the button via `getLoginUrlInfo`
    /// (schema:12985); the answer drains in `poll_live`. TDLib errors
    /// degrade the button to a plain URL press (schema:12993 doc on
    /// `getLoginUrl`); without a live connection we open the raw URL
    /// straight away.
    pub(super) fn press_login_url(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        button_id: i64,
        url: &str,
        cx: &mut Context<Self>,
    ) {
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .send_login_url_info(chat_id, message_id, button_id, url)
        });
        match sent {
            Some(Ok(_)) => {}
            _ => {
                // Not connected, or TDLib refused the send: plain URL-button
                // behavior is the honest fallback.
                self.open_message_url(url, cx);
            }
        }
    }

    /// B1: open the password prompt for an
    /// `inlineKeyboardButtonTypeCallbackWithPassword` button press
    /// (schema 1.8.67, line 3789).
    pub(super) fn open_callback_password_dialog(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        data: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.links.callback_password_dialog = Some(CallbackPasswordDialog::new(
            window, cx, chat_id, message_id, data,
        ));
        if let Some(dialog) = &self.links.callback_password_dialog {
            dialog
                .password_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// B1: close the password prompt without submitting.
    pub(super) fn close_callback_password_dialog(&mut self, cx: &mut Context<Self>) {
        self.links.callback_password_dialog = None;
        cx.notify();
    }

    /// B1: submit the password dialog — sends
    /// `callbackQueryPayloadDataWithPassword` (schema 1.8.67, line 7740).
    /// The password is dropped after the send; a wrong password surfaces
    /// as "wrong 2-step verification password" via the error drain in
    /// `Session::apply_payload` (TDLib error 400).
    pub(super) fn submit_callback_password_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.links.callback_password_dialog.take() else {
            return;
        };
        let password = dialog.password_input.read(cx).value().to_string();
        let sent = self.live.as_mut().map(|live| {
            live.driver.send_callback_query_with_password(
                dialog.chat_id,
                dialog.message_id,
                &password,
                &dialog.data,
            )
        });
        // The password is dropped here — the TDLib send above is its only
        // use; speculative zeroization was removed (DECISIONS.md).
        drop(password);
        if !matches!(sent, Some(Ok(_))) {
            self.set_status_note(
                StatusNote::error(tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
                cx,
            );
        }
        cx.notify();
    }

    /// B1: game button (`inlineKeyboardButtonTypeCallbackGame`, schema
    /// 1.8.67 line 3792). Sends `callbackQueryPayloadGame` carrying the
    /// message's `messageGame` short name (schema:7743) — the TDLib
    /// game-launch flow; the answer may carry a URL to open. Games UI is
    /// out of this slice, so an answer URL opens in the browser.
    pub(super) fn press_game_button(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        game_short_name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(game_short_name) = game_short_name.filter(|name| !name.is_empty()) else {
            self.set_status_note(
                StatusNote::error(tr(keys::Q_MEDIA_GAME_DATA_MISSING_CANT_LAUNCH)),
                cx,
            );
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .send_game_callback_query(chat_id, message_id, &game_short_name)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.set_status_note(
                StatusNote::error(tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
                cx,
            );
        }
    }

    /// B1: user button (`inlineKeyboardButtonTypeUser`, schema 1.8.67 line
    /// 3801) — open the private chat with the user. Reuses a listed
    /// private chat when one exists; otherwise `createPrivateChat` opens
    /// it via the `CreatePrivateChat` pending purpose.
    pub(super) fn open_user_chat(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let listed = self.listed_private_chat(user_id);
        if let Some(chat_id) = listed {
            self.select_listed_chat(chat_id, window, cx);
            return;
        }
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.create_private_chat_for(user_id));
        match sent {
            Some(Ok(Some(_))) => {}
            _ => self.set_status_note(
                StatusNote::error(tr(keys::Q_MEDIA_COULDNT_OPEN_THE_USERS_CHAT)),
                cx,
            ),
        }
    }

    /// kit Phase 2 (redo): callback password hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_callback_password_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CallbackPassword, |this, _, cx| {
                this.close_callback_password_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some(dialog_state) = this.links.callback_password_dialog.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::Q_MEDIA_ENTER_2_STEP_PASSWORD,
                    )))
                    .on_close(on_close.clone());
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MEDIA_THIS_BUTTON_IS_PROTECTED_BY)),
                )
                .child(
                    Input::new(&dialog_state.password_input)
                        .aria_label(tr(keys::Q_MEDIA_TWO_STEP_VERIFICATION_PASSWORD))
                        .content_type(InputContentType::Password),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(ghost_text_button(
                    "b1-password-cancel",
                    tr(keys::CANCEL),
                    cx.listener(|this, _, window, cx| {
                        this.close_callback_password_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::CallbackPassword, window, cx);
                    }),
                ))
                .child(
                    Button::new("b1-password-submit")
                        .label(tr(keys::Q_MEDIA_SEND))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_callback_password_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallbackPassword, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MEDIA_ENTER_2_STEP_PASSWORD,
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): login URL confirm hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_login_url_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::LoginUrlConfirm, |this, _, cx| {
                this.links.login_url_confirm = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let confirm = this.links.login_url_confirm.as_ref();
            let domain = confirm
                .map(|confirm| confirm.domain.clone())
                .unwrap_or_default();
            let request_write_access = confirm
                .map(|confirm| confirm.request_write_access)
                .unwrap_or(false);
            let mut body = div().flex().flex_col().gap_3().child(
                div().text_sm().text_color(text_muted()).child(tr_with(
                    keys::Q_MEDIA_THE_BOT_WANTS_TO_OPEN,
                    &[("domain", &domain.to_string())],
                )),
            );
            if request_write_access {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MEDIA_THE_BOT_ALSO_ASKS_FOR)),
                );
            }
            let body = body.into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(ghost_text_button(
                    "b1-login-cancel",
                    tr(keys::CANCEL),
                    cx.listener(|this, _, window, cx| {
                        this.links.login_url_confirm = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::LoginUrlConfirm, window, cx);
                    }),
                ))
                .child(
                    Button::new("b1-login-open")
                        .label(tr(keys::Q_MEDIA_OPEN))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_login_url(cx);
                            this.close_kit_dialog_if_done(DialogKind::LoginUrlConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MEDIA_OPEN_LOGIN_URL,
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Phase 3.1: bot info panel rendered below the conversation header when
    /// the open chat is a bot chat (lazy `getUserFullInfo` on chat open).
    /// Slice B2 extends it into the bot profile actions: the START button
    /// (when a `t.me/<bot>?start=<param>` deep link armed
    /// `Session::bot_start_params`, per `internalLinkTypeBotStart`,
    /// schema 1.8.67 line 9399), the bot menu button
    /// (`botInfo.menu_button`, schema line 834), Restart bot
    /// (`deleteChatHistory` + `sendBotStartMessage`), Share (copies the
    /// `t.me/<username>` link, Telegram X `ProfileController.share`),
    /// Block/Unblock (`setMessageSenderBlockList`, CL3 plumbing), the
    /// privacy-policy link (`botInfo.privacy_policy_url`, schema line
    /// 2414), and the similar-bots section (`getBotSimilarBots`, schema
    /// line 11640). Returns `None` for non-bot chats.
    pub(super) fn bot_info_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        session.bot_user_id_for_chat(session.open_chat?)?;
        self.bot_info_panel_body(cx)
    }

    /// `bot_info_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn bot_info_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let open = session.open_chat?;
        let bot_id = session.bot_user_id_for_chat(open)?;
        let info: BotInfo = session.bot_info_for_chat(open)?.clone();
        let start_param = session.bots.bot_start_params.get(&open.0).cloned();
        // Telegram Desktop shows the bot card only in an empty chat (or
        // while a START link waits); once there are messages it's a normal
        // conversation, with commands behind the composer's Menu button.
        let empty = session
            .histories
            .get(&open.0)
            .is_none_or(|history| history.messages.is_empty());
        if !empty && start_param.is_none() {
            return None;
        }
        let blocked = session.chats.get(&open.0).is_some_and(|chat| chat.blocked);
        let username = session
            .users
            .get(&bot_id)
            .map(|user| user.username.clone())
            .unwrap_or_default();
        let similar = session.bots.similar_bots.get(&bot_id).cloned();
        let mut panel = div()
            .id("bot-info")
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_MEDIA_BOT)),
            );
        // The Start button lives in the bar that replaces the composer
        // (`bottom_action`), as in Telegram Desktop.
        if !info.description.is_empty() {
            panel = panel.child(div().text_sm().child(info.description.clone()));
        }
        if !info.commands.is_empty() {
            let mut row = div().id("bot-commands").flex().flex_wrap().gap_1();
            for command in &info.commands {
                let name = command.command.clone();
                let label = if command.description.is_empty() {
                    format!("/{name}")
                } else {
                    format!("/{name} — {}", command.description)
                };
                row = row.child(bot_command_button(name, label, command.is_ephemeral, cx));
            }
            panel = panel.child(row);
        }
        // Slice B2: bot menu button (`botMenuButton`, schema line 834).
        // The URL opens in the OS browser — the honest fallback used for
        // B1 web-app buttons (no in-app web view).
        if let Some(menu) = &info.menu_button
            && !menu.url.is_empty()
        {
            let text = if menu.text.is_empty() {
                tr(keys::Q_MEDIA_MENU).to_string()
            } else {
                menu.text.clone()
            };
            let url = menu.url.clone();
            panel = panel.child(bot_menu_link(text, url, cx));
        }
        // Slice bots-games: games seen in this bot's chat (`messageGame`,
        // schema 1.8.67 line 5234). Each offers Send → `inputMessageGame`
        // to the open chat (schema:6156). Only games TDLib delivered are
        // listed — short names are never invented.
        if let Some(games) = session.bots.bot_games.get(&bot_id)
            && !games.is_empty()
        {
            let mut section = div().id("bot-games").flex().flex_col().gap_1().child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_MEDIA_GAMES)),
            );
            for game in games {
                let title = if game.title.is_empty() {
                    game.short_name.clone()
                } else {
                    game.title.clone()
                };
                let short_name = game.short_name.clone();
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().child("🎮"))
                        .child(div().text_sm().flex_1().child(title))
                        .child(ghost_text_button(
                            format!("bot-send-game-{short_name}"),
                            tr(keys::Q_MEDIA_SEND),
                            cx.listener(move |this, _, _, cx| {
                                this.press_send_game(open, bot_id, &short_name, cx);
                            }),
                        )),
                );
            }
            panel = panel.child(section);
        }
        // Slice B2: profile actions row.
        let mut actions = div().id("bot-actions").flex().flex_wrap().gap_1();
        actions = actions.child(ghost_text_button(
            "bot-restart",
            tr(keys::Q_MEDIA_RESTART_BOT),
            cx.listener(move |this, _, _, cx| {
                this.open_group_confirm(open, GroupConfirmAction::RestartBot, cx);
            }),
        ));
        if !username.is_empty() {
            let link = format!("https://t.me/{username}");
            actions = actions.child(ghost_text_button(
                "bot-share",
                tr(keys::Q_MEDIA_SHARE),
                cx.listener(move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                    this.set_status_note(StatusNote::info(tr(keys::Q_MEDIA_BOT_LINK_COPIED)), cx);
                }),
            ));
        }
        actions = actions.child(ghost_text_button(
            "bot-block",
            if blocked {
                tr(keys::Q_MEDIA_UNBLOCK_BOT)
            } else {
                tr(keys::Q_MEDIA_BLOCK_BOT)
            },
            cx.listener(move |this, _, _, cx| {
                this.open_group_confirm(
                    open,
                    GroupConfirmAction::BlockUser { block: !blocked },
                    cx,
                );
            }),
        ));
        panel = panel.child(actions);
        // Slice B2: privacy policy. Schema line 2414: the link when the
        // bot published one; otherwise `/privacy` when the bot lists that
        // command; otherwise the fallback page. Read-only — there is no
        // client-side bot-privacy setting in the schema.
        if !info.privacy_policy_url.is_empty() {
            let url = info.privacy_policy_url.clone();
            panel = panel.child(ghost_text_button(
                "bot-privacy-policy",
                tr(keys::Q_MEDIA_PRIVACY_POLICY),
                cx.listener(move |this, _, _, cx| {
                    this.open_message_url(&url, cx);
                }),
            ));
        } else if info.commands.iter().any(|c| c.command == "privacy") {
            panel = panel.child(ghost_text_button(
                "bot-privacy-command",
                tr(keys::Q_MEDIA_PRIVACY_PRIVACY),
                cx.listener(move |this, _, window, cx| {
                    this.insert_bot_command("privacy", window, cx);
                }),
            ));
        } else {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_MEDIA_PRIVACY_TELEGRAM_ORG_PRIVACY_TPA)),
            );
        }
        // Slice B2: similar bots (`getBotSimilarBots`, schema line 11640;
        // Telegram X `SharedChatsController.Mode.SIMILAR_BOTS`). The
        // button fetches once; the names resolve through `Session::users`
        // and open the bot's chat.
        match &similar {
            Some(SimilarBotsFetch::Loaded(ids)) => {
                let mut row = div().id("bot-similar").flex().flex_wrap().gap_1().child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::SIMILAR_BOTS_TITLE)),
                );
                for id in ids {
                    let id = *id;
                    let name = session
                        .users
                        .get(&id)
                        .map(|user| user.display_name())
                        .unwrap_or_else(|| {
                            tr_with(keys::Q_MEDIA_BOT_2, &[("id", &id.to_string())])
                        });
                    row = row.child(ghost_text_button(
                        format!("bot-similar-{id}"),
                        name,
                        cx.listener(move |this, _, window, cx| {
                            this.open_user_chat(id, window, cx);
                        }),
                    ));
                }
                panel = panel.child(row);
            }
            _ => {
                panel = panel.child(ghost_text_button(
                    "bot-similar-fetch",
                    tr(keys::SIMILAR_BOTS_TITLE),
                    cx.listener(move |this, _, _, cx| {
                        this.fetch_similar_bots(bot_id, cx);
                    }),
                ));
            }
        }
        Some(panel.into_any_element())
    }

    /// Slice B2: START press — `sendBotStartMessage` with the deep-link
    /// parameter (schema 1.8.67, line 12216); the button clears like
    /// Telegram X's `hideActionButton` after the send.
    pub(super) fn press_bot_start(
        &mut self,
        chat_id: ChatId,
        bot_id: i64,
        parameter: String,
        cx: &mut Context<Self>,
    ) {
        let note = self.live.as_mut().map(|live| {
            match live
                .driver
                .send_bot_start_message(chat_id, bot_id, &parameter)
            {
                Ok(Some(_)) => {
                    live.driver.session.bots.bot_start_params.remove(&chat_id.0);
                    StatusNote::silent(tr(keys::Q_MEDIA_STARTING_BOT))
                }
                Ok(None) => StatusNote::silent(tr(keys::Q_MEDIA_REQUEST_ALREADY_IN_FLIGHT)),
                Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
            }
        });
        self.set_status_note(
            note.unwrap_or_else(|| {
                StatusNote::error(tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN))
            }),
            cx,
        );
    }

    /// Slice B2: fetch `getBotSimilarBots` for the open bot profile; the
    /// reducer keys the `users` answer by bot id.
    pub(super) fn fetch_similar_bots(&mut self, bot_id: i64, cx: &mut Context<Self>) {
        let note = self
            .live
            .as_mut()
            .map(|live| match live.driver.fetch_similar_bots(bot_id) {
                Ok(Some(_)) => {
                    live.driver
                        .session
                        .bots
                        .similar_bots
                        .entry(bot_id)
                        .or_insert(SimilarBotsFetch::Loading);
                    StatusNote::silent(tr(keys::Q_MEDIA_LOADING_SIMILAR_BOTS))
                }
                Ok(None) => StatusNote::silent(tr(keys::Q_MEDIA_ALREADY_LOADING)),
                Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
            });
        self.set_status_note(
            note.unwrap_or_else(|| {
                StatusNote::error(tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN))
            }),
            cx,
        );
    }

    /// Phase 3.1: insert a tapped bot command into the composer. Empty
    /// composer → the bare command; otherwise appended after a space (3.3
    /// owns the full `/` menu).
    /// Telegram Desktop's bot "Menu" button left of the composer: the bot's
    /// web menu when it has one, else the `/` command list.
    pub(super) fn bot_menu_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let open = session.open_chat?;
        let info = session.bot_info_for_chat(open)?;
        let url = info
            .menu_button
            .as_ref()
            .map(|menu| (menu.text.clone(), menu.url.clone()))
            .filter(|(_, url)| !url.is_empty());
        if url.is_none() && info.commands.is_empty() {
            return None;
        }
        let label = url
            .as_ref()
            .map(|(text, _)| text.clone())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| tr(keys::Q_MEDIA_MENU).to_string());
        Some(
            Button::new("bot-menu")
                .icon(IconName::Menu)
                .label(label)
                .small()
                .primary()
                .rounded_full()
                .tooltip(tr(keys::Q_MEDIA_BOT_MENU))
                .on_click(cx.listener(move |this, _, window, cx| match &url {
                    Some((_, url)) => this.open_bot_menu_web_app(url, cx),
                    None => {
                        this.composer.update(cx, |input, cx| {
                            input.set_value("/", window, cx);
                            input.focus(window, cx);
                            input.set_selected_range(1..1, cx);
                        });
                        this.sync_command_menu(cx);
                    }
                }))
                .into_any_element(),
        )
    }

    pub(super) fn insert_bot_command(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = quill::composer::insert_bot_command_text(&self.composer_markup(cx), command);
        self.set_composer_markup(&next, window, cx);
        self.composer.update(cx, |input, cx| {
            let end = input.value().len();
            input.set_selected_range(end..end, cx);
        });
        self.sync_command_menu(cx);
    }
}

/// One command on the bot card. Ephemeral commands (schema 1.8.67
/// `botCommand`, line 826) carry the same eye-off icon as the `/` menu:
/// their result is only visible to the sender. Built in a frame of its own,
/// like `lean_buttons`.
fn bot_command_button(
    name: String,
    label: String,
    ephemeral: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut button = Button::new(format!("bot-command-{name}"))
        .label(label)
        .ghost();
    if ephemeral {
        button = button.icon(Icon::new(IconName::EyeOff));
    }
    button
        .on_click(cx.listener(move |this, _, window, cx| {
            this.insert_bot_command(&name, window, cx);
        }))
        .into_any_element()
}

/// The bot card's menu button; its URL opens in the OS browser.
fn bot_menu_link(text: String, url: String, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("bot-menu-button")
        .label(text)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_message_url(&url, cx);
        }))
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    CallbackPassword => DialogSpec::new(
        2400,
        |app| app.links.callback_password_dialog.is_some(),
        QuillApp::build_callback_password_dialog,
    )
    .closes(|app, _, cx| app.close_callback_password_dialog(cx)),

    LoginUrlConfirm => DialogSpec::new(
        2500,
        |app| app.links.login_url_confirm.is_some(),
        QuillApp::build_login_url_confirm_dialog,
    )

    .closes(|app, _, cx| { app.links.login_url_confirm = None; cx.notify(); }),
}
