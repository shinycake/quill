//! bot panels, callbacks, game buttons, login URLs, command menus.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session, SimilarBotsFetch};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{BotInfo, CallbackQueryAnswer, ChatKind};
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

/// M2 `ReadyRichMessage` fixture: like `apply_ready_bot_chat`, plus two
/// injected `updateNewMessage`s — a `messageRichMessage` exercising every
/// rendered block kind (headings, styled paragraphs, list, collapsible,
/// inline document, table, divider, `pageBlockButtonRow` with URL +
/// callback buttons) and a text message whose `ephemeral_content`
/// overrides the regular content for the current user.
pub(super) fn apply_ready_rich_message(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_bot_chat(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let rich_message = r#"{"@type":"updateNewMessage","message":{"id":401,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageRichMessage","message":{"@type":"richMessage","is_full":true,"is_rtl":false,"blocks":[
        {"@type":"pageBlockTitle","title":{"@type":"richTextPlain","text":"Club night"}},
        {"@type":"pageBlockParagraph","text":{"@type":"richTexts","texts":[
            {"@type":"richTextPlain","text":"Pick "},
            {"@type":"richTextBold","text":{"@type":"richTextPlain","text":"one"}},
            {"@type":"richTextPlain","text":" and "},
            {"@type":"richTextUrl","text":{"@type":"richTextPlain","text":"read more"},"url":"https://example.com","is_cached":false}]}},
        {"@type":"pageBlockList","is_ordered":false,"items":[
            {"@type":"pageBlockListItem","label":"Live set","blocks":[]},
            {"@type":"pageBlockListItem","label":"DJ set","blocks":[]}]},
        {"@type":"pageBlockDetails","header":{"@type":"richTextPlain","text":"Details"},"blocks":[
            {"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"Doors at 9pm, show at 10pm."}}],"is_open":true},
        {"@type":"pageBlockDocument","document":{"@type":"document","file_name":"setlist.pdf"},"caption":{"@type":"pageBlockCaption","text":{"@type":"richTextPlain","text":"Tonight's setlist"},"credit":{"@type":"richTextPlain","text":""}}},
        {"@type":"pageBlockPhoto","photo":{"@type":"photo","id":7770001},"caption":{"@type":"pageBlockCaption","text":{"@type":"richTextPlain","text":"Stage lights"},"credit":{"@type":"richTextPlain","text":""}},"url":"","has_spoiler":false},
        {"@type":"pageBlockVideo","video":{"@type":"video","id":7770002},"caption":{"@type":"pageBlockCaption","text":{"@type":"richTextPlain","text":"Encore clip"},"credit":{"@type":"richTextPlain","text":""}},"need_autoplay":false,"is_looped":false,"has_spoiler":false},
        {"@type":"pageBlockTable","cells":[
            [{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"A1"}},{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"B1"}}],
            [{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"A2"}},{"@type":"pageBlockTableCell","text":{"@type":"richTextPlain","text":"B2"}}]],"is_bordered":true,"is_striped":false,"has_header":false},
        {"@type":"pageBlockDivider"},
        {"@type":"pageBlockButtonRow","buttons":[
            {"@type":"inlineButton","text":{"@type":"richTextPlain","text":"Get tickets"},"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com/tickets"}},
            {"@type":"inlineButton","text":{"@type":"richTextPlain","text":"RSVP"},"style":{"@type":"buttonStyleSuccess"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}}]}
    ]}}}}"#.to_string();
    let ephemeral_message = r#"{"@type":"updateNewMessage","message":{"id":402,"chat_id":21,"is_outgoing":false,
        "content":{"@type":"messageText","text":{"@type":"formattedText","text":"public fallback","entities":[]}},
        "ephemeral_content":{"@type":"ephemeralMessageContent","can_be_saved":false,"has_timestamped_media":false,
        "content":{"@type":"messageRichMessage","message":{"@type":"richMessage","is_full":true,"is_rtl":false,"blocks":[
            {"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"Only you can see this — the ephemeral override wins."}}
        ]}}}}}"#.to_string();
    for json in [rich_message, ephemeral_message] {
        // Demo-only fixture: a parse failure is a programmer error and must
        // fail loudly, never silently produce a wrong screenshot.
        let owned = copy_and_parse(&json, seq, &dyn_sink).expect("M2 demo fixture must parse");
        session.apply(owned);
    }
}

/// `ReadyBotCommandMenu` fixture (Phase 3.3): like `apply_ready_bot_chat`,
/// plus a global-scope `botCommands` response through the real
/// `getCommands` reducer path, so the `/` menu shows the bot-specific
/// commands and a "Global" section below.
pub(super) fn apply_ready_bot_command_menu(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_bot_chat(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let cmd_extra = session.request(RequestPurpose::GetCommands, Some(ChatId(21)));
    let json = format!(
        r#"{{"@type":"botCommands","@extra":"{}","bot_user_id":21,"commands":[{{"@type":"botCommand","command":"settings","description":"Tweak the bot (ephemeral)","is_ephemeral":true}}]}}"#,
        cmd_extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

pub(super) fn apply_ready_bot_profile(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_bot_chat(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    // Armed deep-link start parameter (t.me/demo_bot?start=demo_xyz).
    session.bot_start_params.insert(21, "demo_xyz".to_string());
    let info_extra = session.request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
    let jsons = [
        // Bot gains a username so Share renders; similar bots get names.
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","usernames":{"@type":"usernames","active_usernames":["demo_bot"],"disabled_usernames":[],"editable_username":"demo_bot","collectible_usernames":[]},"type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#
            .to_string(),
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Echo","usernames":{"@type":"usernames","active_usernames":["echo_bot"],"disabled_usernames":[],"editable_username":"echo_bot","collectible_usernames":[]},"type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#
            .to_string(),
        r#"{"@type":"updateUser","user":{"id":32,"first_name":"Foxtrot","usernames":{"@type":"usernames","active_usernames":["foxtrot_bot"],"disabled_usernames":[],"editable_username":"foxtrot_bot","collectible_usernames":[]},"type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#
            .to_string(),
        format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"Demo Bot answers questions and shows how the info panel looks. It understands /start, /help and /ping.","menu_button":{{"@type":"botMenuButton","text":"Open app","url":"https://example.com/app"}},"privacy_policy_url":"https://example.com/privacy","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}},{{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}}]}}}}"#,
            info_extra.0,
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // Similar bots, through the real `getBotSimilarBots` reducer path.
    let similar_extra = session.request_for_user(RequestPurpose::GetBotSimilarBots, 21);
    let json = format!(
        r#"{{"@type":"users","@extra":"{}","total_count":2,"user_ids":[31,32]}}"#,
        similar_extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
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
                            .child("Quill"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_primary())
                            .child(
                                "Please note that inline bots are provided by third-party developers. \
                                 For the bot to work, the symbols you type after the bot's username \
                                 are sent to the respective developer.",
                            ),
                    ),
            )
            .child(
                Button::new("confirm-inline-bot-alert")
                    .label("Confirm")
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
            self.status_note = if answer.text.is_empty() {
                "bot answered".into()
            } else {
                answer.text
            };
        } else {
            self.status_note = if quill::platform::open_external_url(&answer.url) {
                "opened link".into()
            } else {
                "could not open link".into()
            };
        }
        cx.notify();
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
        self.callback_password_dialog = Some(CallbackPasswordDialog::new(
            window, cx, chat_id, message_id, data,
        ));
        if let Some(dialog) = &self.callback_password_dialog {
            dialog
                .password_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// B1: close the password prompt without submitting.
    pub(super) fn close_callback_password_dialog(&mut self, cx: &mut Context<Self>) {
        self.callback_password_dialog = None;
        cx.notify();
    }

    /// B1: submit the password dialog — sends
    /// `callbackQueryPayloadDataWithPassword` (schema 1.8.67, line 7740).
    /// The password is dropped after the send; a wrong password surfaces
    /// as "wrong 2-step verification password" via the error drain in
    /// `Session::apply_payload` (TDLib error 400).
    pub(super) fn submit_callback_password_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.callback_password_dialog.take() else {
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
            self.set_status_note("Couldn't reach Telegram; try again.", cx);
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
            self.set_status_note("Game data missing; can't launch.", cx);
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .send_game_callback_query(chat_id, message_id, &game_short_name)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.set_status_note("Couldn't reach Telegram; try again.", cx);
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
        let listed = self.live.as_ref().and_then(|live| {
            live.driver
                .session
                .chats
                .values()
                .filter(|chat| chat.supported())
                .find(|chat| {
                    matches!(&chat.kind, ChatKind::Private { user_id: peer } if peer.0 == user_id)
                })
                .map(|chat| chat.id)
        });
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
            _ => self.set_status_note("Couldn't open the user's chat.", cx),
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
            let Some(dialog_state) = this.callback_password_dialog.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Enter 2-step password"))
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
                        .child("This button is protected by your two-step verification password."),
                )
                .child(
                    Input::new(&dialog_state.password_input)
                        .aria_label("Two-step verification password")
                        .content_type(InputContentType::Password),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("b1-password-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_callback_password_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallbackPassword, window, cx);
                        })),
                )
                .child(
                    Button::new("b1-password-submit")
                        .label("Send")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_callback_password_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallbackPassword, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Enter 2-step password"))
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
                this.login_url_confirm = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let confirm = this.login_url_confirm.as_ref();
            let domain = confirm
                .map(|confirm| confirm.domain.clone())
                .unwrap_or_default();
            let request_write_access = confirm
                .map(|confirm| confirm.request_write_access)
                .unwrap_or(false);
            let mut body = div().flex().flex_col().gap_3().child(
                div().text_sm().text_color(text_muted()).child(format!(
                    "The bot wants to open a login URL for {domain}. Open it in your browser?"
                )),
            );
            if request_write_access {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("The bot also asks for permission to send you messages."),
                );
            }
            let body = body.into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("b1-login-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.login_url_confirm = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::LoginUrlConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("b1-login-open")
                        .label("Open")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_login_url(cx);
                            this.close_kit_dialog_if_done(DialogKind::LoginUrlConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Open login URL?"))
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
        let open = session.open_chat?;
        let bot_id = session.bot_user_id_for_chat(open)?;
        let info: BotInfo = session.bot_info_for_chat(open)?.clone();
        let start_param = session.bot_start_params.get(&open.0).cloned();
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
        let similar = session.similar_bots.get(&bot_id).cloned();
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
                    .child("Bot"),
            );
        // Slice B2: START button — pressing sends `sendBotStartMessage`
        // with the deep-link parameter (schema 1.8.67, line 12216;
        // Telegram X shows it until the chat gains messages).
        if let Some(parameter) = start_param {
            panel = panel.child(
                Button::new("bot-start")
                    .label("START")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.press_bot_start(open, bot_id, parameter.clone(), cx);
                    })),
            );
        }
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
                // Ephemeral commands (schema 1.8.67 `botCommand`,
                // line 826) carry the same eye-off icon as the `/` menu:
                // their result is only visible to the sender.
                let mut button = Button::new(format!("bot-command-{name}"))
                    .label(label)
                    .ghost();
                if command.is_ephemeral {
                    button = button.icon(Icon::new(IconName::EyeOff));
                }
                row = row.child(button.on_click(cx.listener(move |this, _, window, cx| {
                    this.insert_bot_command(&name, window, cx);
                })));
            }
            panel = panel.child(row);
        }
        // Slice B2: bot menu button (`botMenuButton`, schema line 834).
        // The URL opens in the OS browser — the honest fallback used for
        // B1 web-app buttons (no in-app web view).
        if let Some(menu) = &info.menu_button {
            if !menu.url.is_empty() {
                let text = if menu.text.is_empty() {
                    "Menu".to_string()
                } else {
                    menu.text.clone()
                };
                let url = menu.url.clone();
                panel = panel.child(Button::new("bot-menu-button").label(text).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.open_message_url(&url, cx);
                    }),
                ));
            }
        }
        // Slice bots-games: games seen in this bot's chat (`messageGame`,
        // schema 1.8.67 line 5234). Each offers Send → `inputMessageGame`
        // to the open chat (schema:6156). Only games TDLib delivered are
        // listed — short names are never invented.
        if let Some(games) = session.bot_games.get(&bot_id)
            && !games.is_empty()
        {
            let mut section = div().id("bot-games").flex().flex_col().gap_1().child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Games"),
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
                        .child(
                            Button::new(format!("bot-send-game-{short_name}"))
                                .label("Send")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.press_send_game(open, bot_id, &short_name, cx);
                                })),
                        ),
                );
            }
            panel = panel.child(section);
        }
        // Slice B2: profile actions row.
        let mut actions = div().id("bot-actions").flex().flex_wrap().gap_1();
        actions = actions.child(
            Button::new("bot-restart")
                .label("Restart bot")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_group_confirm(open, GroupConfirmAction::RestartBot, cx);
                })),
        );
        if !username.is_empty() {
            let link = format!("https://t.me/{username}");
            actions = actions.child(Button::new("bot-share").label("Share").ghost().on_click(
                cx.listener(move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                    this.set_status_note("bot link copied", cx);
                }),
            ));
        }
        actions = actions.child(
            Button::new("bot-block")
                .label(if blocked { "Unblock bot" } else { "Block bot" })
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_group_confirm(
                        open,
                        GroupConfirmAction::BlockUser { block: !blocked },
                        cx,
                    );
                })),
        );
        panel = panel.child(actions);
        // Slice B2: privacy policy. Schema line 2414: the link when the
        // bot published one; otherwise `/privacy` when the bot lists that
        // command; otherwise the fallback page. Read-only — there is no
        // client-side bot-privacy setting in the schema.
        if !info.privacy_policy_url.is_empty() {
            let url = info.privacy_policy_url.clone();
            panel = panel.child(
                Button::new("bot-privacy-policy")
                    .label("Privacy policy")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_message_url(&url, cx);
                    })),
            );
        } else if info.commands.iter().any(|c| c.command == "privacy") {
            panel = panel.child(
                Button::new("bot-privacy-command")
                    .label("Privacy (/privacy)")
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.insert_bot_command("privacy", window, cx);
                    })),
            );
        } else {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Privacy: telegram.org/privacy-tpa"),
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
                        .child("Similar bots"),
                );
                for id in ids {
                    let id = *id;
                    let name = session
                        .users
                        .get(&id)
                        .map(|user| user.display_name())
                        .unwrap_or_else(|| format!("Bot {id}"));
                    row = row.child(
                        Button::new(format!("bot-similar-{id}"))
                            .label(name)
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_user_chat(id, window, cx);
                            })),
                    );
                }
                panel = panel.child(row);
            }
            _ => {
                panel = panel.child(
                    Button::new("bot-similar-fetch")
                        .label("Similar bots")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.fetch_similar_bots(bot_id, cx);
                        })),
                );
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
                    live.driver.session.bot_start_params.remove(&chat_id.0);
                    "starting bot…".to_string()
                }
                Ok(None) => "request already in flight".to_string(),
                Err(_) => "Couldn't reach Telegram; try again.".to_string(),
            }
        });
        self.set_status_note(
            note.as_deref()
                .unwrap_or("Couldn't reach Telegram; try again."),
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
                        .similar_bots
                        .entry(bot_id)
                        .or_insert(SimilarBotsFetch::Loading);
                    "loading similar bots…".to_string()
                }
                Ok(None) => "already loading".to_string(),
                Err(_) => "Couldn't reach Telegram; try again.".to_string(),
            });
        self.set_status_note(
            note.as_deref()
                .unwrap_or("Couldn't reach Telegram; try again."),
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
            .unwrap_or_else(|| "Menu".to_string());
        Some(
            Button::new("bot-menu")
                .icon(IconName::Menu)
                .label(label)
                .small()
                .primary()
                .rounded_full()
                .tooltip("Bot menu")
                .on_click(cx.listener(move |this, _, window, cx| match &url {
                    Some((_, url)) => this.open_message_url(url, cx),
                    None => {
                        this.composer.update(cx, |input, cx| {
                            input.set_value("/", window, cx);
                            input.focus(window, cx);
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
        self.composer.update(cx, |input, cx| {
            let next =
                quill::composer::insert_bot_command_text(&input.value().to_string(), command);
            input.set_value(next, window, cx);
        });
        self.sync_command_menu(cx);
    }
}
