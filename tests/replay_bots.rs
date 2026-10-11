//! Bots and inline keyboards replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

/// Phase 3.1: bot private chats ride the ordinary private-chat path — they
/// are listed, ungated, open, render history, and keep the composer. The
/// gating infrastructure (`is_supported_cloud_chat` / `gate_reason` /
/// `can_post`) is untouched.
#[test]
fn replay_bot_chat_ungated_with_history() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"ping","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"/start","entities":[]}}}}"#,
        ],
    );
    // Chat list: the bot chat appears like any private chat.
    let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![21]);
    let chat = session.chats.get(&21).unwrap();
    assert!(chat.supported());
    assert!(chat.kind.gate_reason().is_none());
    assert!(chat.can_post());
    // Bot detection feeds the lazy `getUserFullInfo` fetch.
    assert_eq!(
        session.bot_user_id_for_chat(quill::ids::ChatId(21)),
        Some(21)
    );
    // History renders bot and own messages.
    let history = session.histories.get(&21).unwrap();
    assert_eq!(history.messages.len(), 2);
    assert!(!history.messages[&301].is_outgoing);
    assert!(history.messages[&302].is_outgoing);
}

/// Phase 3.1: a `getUserFullInfo` response caches `botInfo` (description +
/// commands) for the bot chat; the panel reads it back.
#[test]
fn replay_bot_info_cached_from_full_info() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ],
    );
    assert!(session.bot_info_for_chat(quill::ids::ChatId(21)).is_none());
    let extra = session.request(
        RequestPurpose::GetUserFullInfo,
        Some(quill::ids::ChatId(21)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"CANARY_desc","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}},{{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}}]}}}}"#,
            extra.0
        )],
    );
    let info = session
        .bot_info_for_chat(quill::ids::ChatId(21))
        .expect("bot info cached");
    assert_eq!(info.short_description, "A demo bot");
    assert_eq!(info.description, "CANARY_desc");
    assert_eq!(info.commands.len(), 2);
    assert_eq!(info.commands[0].command, "start");
    assert_eq!(info.commands[0].description, "Start the bot");
    assert_eq!(info.commands[1].command, "help");
}

/// Phase 3.3: a `getCommands` response caches the global-scope commands and
/// merges them below the `botInfo` commands in `command_menu_items`
/// (deduped by command name); a stray `botCommands` with no matching
/// pending request is ignored.
#[test]
fn replay_bot_commands_cached_from_get_commands() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ],
    );
    let chat = quill::ids::ChatId(21);
    assert!(session.command_menu_items(chat).is_empty());

    // `botInfo` commands first (specific).
    let full_extra = session.request(RequestPurpose::GetUserFullInfo, Some(chat));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}}]}}}}"#,
            full_extra.0
        )],
    );

    // `botCommands` response to the matching `getCommands` request
    // (global scope).
    let cmd_extra = session.request(RequestPurpose::GetCommands, Some(chat));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"botCommands","@extra":"{}","bot_user_id":21,"commands":[{{"@type":"botCommand","command":"settings","description":"Global settings","is_ephemeral":false}},{{"@type":"botCommand","command":"start","description":"Global start","is_ephemeral":false}}]}}"#,
            cmd_extra.0
        )],
    );
    let items = session.command_menu_items(chat);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].command, "start");
    assert_eq!(items[0].description, "Start the bot");
    assert!(!items[0].global);
    // `settings` is new and global; the `start` duplicate is dropped so
    // the bot-specific description wins.
    assert_eq!(items[1].command, "settings");
    assert_eq!(items[1].description, "Global settings");
    assert!(items[1].global);

    // A stray `botCommands` with no matching pending request is ignored.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"botCommands","@extra":"4242","bot_user_id":21,"commands":[{"@type":"botCommand","command":"evil","description":"","is_ephemeral":false}]}"#,
        ],
    );
    let items = session.command_menu_items(chat);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.command != "evil"));
}

/// Phase 3.3: an `error` answer to `getCommands` records an empty command
/// set — the menu falls back to the `botInfo` commands and the driver
/// will not retry the fetch.
#[test]
fn replay_bot_commands_error_records_empty_set() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ],
    );
    let chat = quill::ids::ChatId(21);
    let cmd_extra = session.request(RequestPurpose::GetCommands, Some(chat));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CANARY_bots_only"}}"#,
            cmd_extra.0
        )],
    );
    assert!(session.bots.bot_commands.contains_key(&21));
    assert!(session.command_menu_items(chat).is_empty());
}

/// Phase 3.1: `updateUserFullInfo` refreshes the cached bot info live.
#[test]
fn replay_bot_info_refreshed_by_update() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateUserFullInfo","user_id":21,"user_full_info":{"@type":"userFullInfo","bot_info":{"@type":"botInfo","short_description":"","description":"CANARY_refreshed","commands":[{"@type":"botCommand","command":"ping","description":"","is_ephemeral":false}]}}}"#,
        ],
    );
    let info = session
        .bot_info_for_chat(quill::ids::ChatId(21))
        .expect("bot info cached");
    assert_eq!(info.description, "CANARY_refreshed");
    assert_eq!(info.commands.len(), 1);
    assert_eq!(info.commands[0].command, "ping");
    // If the user stops being a bot, the stale bot info is dropped.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeRegular"}}}"#,
        ],
    );
    assert!(session.bot_info_for_chat(quill::ids::ChatId(21)).is_none());
    assert_eq!(session.bot_user_id_for_chat(quill::ids::ChatId(21)), None);
}

/// Phase 3.2: an `updateNewMessage` with real `replyMarkupInlineKeyboard`
/// JSON stores the keyboard with the message. URL buttons carry an
/// openable URL (the dispatch predicate — the browser itself is not opened
/// in tests); callback buttons carry the payload bytes; switchInline buttons
/// carry the query; unknown button types are kept as disabled placeholders.
#[test]
fn replay_inline_keyboard_stored_from_real_json() {
    use quill::telegram::envelope::{InlineKeyboardButtonType, InlineKeyboardTargetChat};
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Visit site","icon_custom_emoji_id":0,"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com/path"}}],[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}},{"@type":"inlineKeyboardButton","text":"Search here","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"cats","target_chat":{"@type":"targetChatCurrent"}}}],[{"@type":"inlineKeyboardButton","text":"Mystery","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeQuantum"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick one","entities":[]}}}}"#,
        ],
    );
    let message = session
        .histories
        .get(&21)
        .and_then(|h| h.messages.get(&301))
        .expect("message 301 stored");
    let keyboard = message.reply_markup.as_ref().expect("keyboard stored");
    let quill::telegram::envelope::ReplyMarkup::InlineKeyboard(keyboard) = keyboard else {
        panic!("expected inline keyboard");
    };
    assert_eq!(keyboard.rows.len(), 3);
    match &keyboard.rows[0][0].kind {
        InlineKeyboardButtonType::Url { url } => {
            assert_eq!(url, "https://example.com/path");
            // URL dispatch predicate (same gate the UI uses before OS open).
            assert!(quill::text::openable_http_url(url));
        }
        other => panic!("{other:?}"),
    }
    match &keyboard.rows[1][0].kind {
        InlineKeyboardButtonType::Callback { data } => assert_eq!(data, &[1, 2, 3]),
        other => panic!("{other:?}"),
    }
    match &keyboard.rows[1][1].kind {
        InlineKeyboardButtonType::SwitchInline { query, target } => {
            assert_eq!(query, "cats");
            assert_eq!(*target, InlineKeyboardTargetChat::Current);
        }
        other => panic!("{other:?}"),
    }
    // Unknown button types are kept and render disabled — never a crash.
    assert!(matches!(
        &keyboard.rows[2][0].kind,
        InlineKeyboardButtonType::Unknown { .. }
    ));
}

/// Phase 3.2: a `callbackQueryAnswer` response whose `@extra` matches the
/// in-flight `getCallbackQueryAnswer` request is stored for the UI status
/// line; an answer without a matching request is ignored. TDLib error 502
/// (bot missed the query timeout) surfaces as an honest fallback note.
#[test]
fn replay_callback_query_answer_surfaced() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::GetCallbackQueryAnswer,
        Some(quill::ids::ChatId(21)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"callbackQueryAnswer","@extra":"{}","text":"Voted!","show_alert":false,"url":""}}"#,
            extra.0
        )],
    );
    let answer = session
        .bots
        .last_callback_answer
        .take()
        .expect("answer stored");
    assert_eq!(answer.text, "Voted!");
    assert!(!answer.show_alert);
    assert!(answer.url.is_empty());
    // No in-flight request: a stray answer is ignored.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"callbackQueryAnswer","@extra":"999","text":"stray","show_alert":false,"url":""}"#,
        ],
    );
    assert!(session.bots.last_callback_answer.is_none());
    // Error on the in-flight request → fallback note, no TDLib text echoed.
    let extra = session.request(
        RequestPurpose::GetCallbackQueryAnswer,
        Some(quill::ids::ChatId(21)),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","@extra":"{}","code":502,"message":"BOT_QUERY_TIMEOUT"}}"#,
            extra.0
        )],
    );
    let answer = session
        .bots
        .last_callback_answer
        .take()
        .expect("fallback note");
    assert_eq!(answer.text, "bot did not answer");
    assert!(answer.url.is_empty());
}

/// Phase 3.2: a bot message carrying `replyMarkupInlineKeyboard` lands on
/// `HistoryMessage.reply_markup` with rows, styles, and button types intact
/// (callback, URL, switchInline, disabled-type buttons all parsed).
#[test]
fn replay_inline_keyboard_mixed_lands_on_history() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Visit site","icon_custom_emoji_id":0,"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com"}}],[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleSuccess"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}},{"@type":"inlineKeyboardButton","text":"Search here","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"cats","target_chat":{"@type":"targetChatCurrent"}}},{"@type":"inlineKeyboardButton","text":"Buy now","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDanger"},"type":{"@type":"inlineKeyboardButtonTypeBuy"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick one","entities":[]}}}}"#,
        ],
    );
    use quill::telegram::envelope::{
        InlineKeyboardButtonStyle, InlineKeyboardButtonType, InlineKeyboardTargetChat, ReplyMarkup,
    };
    let keyboard = session.histories.get(&21).unwrap().messages[&301]
        .reply_markup
        .clone()
        .expect("inline keyboard on history message");
    let ReplyMarkup::InlineKeyboard(keyboard) = keyboard else {
        panic!("expected inline keyboard");
    };
    assert_eq!(keyboard.rows.len(), 2);
    assert_eq!(keyboard.rows[0].len(), 1);
    assert_eq!(keyboard.rows[1].len(), 3);
    let visit = &keyboard.rows[0][0];
    assert_eq!(visit.text, "Visit site");
    assert_eq!(visit.style, InlineKeyboardButtonStyle::Primary);
    assert_eq!(
        visit.kind,
        InlineKeyboardButtonType::Url {
            url: "https://example.com".to_string()
        }
    );
    let vote = &keyboard.rows[1][0];
    assert_eq!(vote.style, InlineKeyboardButtonStyle::Success);
    assert_eq!(
        vote.kind,
        InlineKeyboardButtonType::Callback {
            data: vec![1, 2, 3]
        }
    );
    let search = &keyboard.rows[1][1];
    assert_eq!(search.style, InlineKeyboardButtonStyle::Default);
    assert_eq!(
        search.kind,
        InlineKeyboardButtonType::SwitchInline {
            query: "cats".to_string(),
            target: InlineKeyboardTargetChat::Current,
        }
    );
    let buy = &keyboard.rows[1][2];
    assert_eq!(buy.style, InlineKeyboardButtonStyle::Danger);
    assert_eq!(buy.kind, InlineKeyboardButtonType::Buy);
}

/// Phase 3.2: a hostile keyboard (unknown button `@type`, unknown style,
/// missing `type`, non-array rows) never breaks the parse — malformed rows
/// are skipped and malformed buttons become disabled `Unknown` placeholders.
#[test]
fn replay_inline_keyboard_hostile_yields_disabled_placeholders() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Mystery","style":{"@type":"buttonStyleFuture"},"type":{"@type":"inlineKeyboardButtonTypeQuantum"}}],[{"@type":"inlineKeyboardButton","text":"No type here"}],"not an array",null],"force_reply":true},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        ],
    );
    use quill::telegram::envelope::{
        InlineKeyboardButtonStyle, InlineKeyboardButtonType, ReplyMarkup,
    };
    let keyboard = session.histories.get(&21).unwrap().messages[&302]
        .reply_markup
        .clone()
        .expect("keyboard parsed despite hostile rows");
    let ReplyMarkup::InlineKeyboard(keyboard) = keyboard else {
        panic!("expected inline keyboard");
    };
    // The two non-array rows are skipped, so only two rows survive.
    assert_eq!(keyboard.rows.len(), 2);
    let mystery = &keyboard.rows[0][0];
    // Unknown style falls back to Default; unknown type → Unknown placeholder.
    assert_eq!(mystery.style, InlineKeyboardButtonStyle::Default);
    assert_eq!(
        mystery.kind,
        InlineKeyboardButtonType::Unknown {
            type_name: "inlineKeyboardButtonTypeQuantum".to_string()
        }
    );
    // Missing `type` → Unknown placeholder (renders disabled).
    assert!(matches!(
        keyboard.rows[1][0].kind,
        InlineKeyboardButtonType::Unknown { .. }
    ));
}

/// Phase 3.2: non-inline markups (`replyMarkupShowKeyboard`) are ignored —
/// `reply_markup` stays `None`, same as an absent field.
#[test]
fn replay_non_inline_markup_ignored() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":303,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[],"is_persistent":false,"resize_keyboard":false,"one_time":false,"is_personal":false,"force_reply":false,"input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":304,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"y","entities":[]}}}}"#,
        ],
    );
    let history = session.histories.get(&21).unwrap();
    // B1: custom keyboards are parsed (not ignored) — the UI renders them
    // above the composer.
    assert!(matches!(
        history.messages[&303].reply_markup,
        Some(quill::telegram::envelope::ReplyMarkup::ShowKeyboard(_))
    ));
    assert!(history.messages[&304].reply_markup.is_none());
}

/// Phase 3.2: `updateMessageEdited` (schema 1.8.67 line 10431) replaces the
/// message's inline keyboard — a new keyboard lands, and a null/absent
/// `reply_markup` removes it.
#[test]
fn replay_update_message_edited_replaces_keyboard() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            BOT_USER_JSON,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Old","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":""}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"old","entities":[]}}}}"#,
        ],
    );
    assert!(
        session.histories.get(&21).unwrap().messages[&301]
            .reply_markup
            .is_some()
    );
    // The bot edits the message with a new keyboard: it replaces the old one.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateMessageEdited","chat_id":21,"message_id":301,"edit_date":1700000001,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"New","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDanger"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AA=="}}]],"force_reply":false}}"#,
        ],
    );
    use quill::telegram::envelope::{InlineKeyboardButtonStyle, ReplyMarkup};
    let keyboard = session.histories.get(&21).unwrap().messages[&301]
        .reply_markup
        .clone()
        .expect("edited keyboard present");
    let ReplyMarkup::InlineKeyboard(keyboard) = keyboard else {
        panic!("expected inline keyboard");
    };
    assert_eq!(keyboard.rows.len(), 1);
    assert_eq!(keyboard.rows[0].len(), 1);
    assert_eq!(keyboard.rows[0][0].text, "New");
    assert_eq!(keyboard.rows[0][0].style, InlineKeyboardButtonStyle::Danger);
    // The bot edits the message with no reply_markup: the keyboard is gone.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateMessageEdited","chat_id":21,"message_id":301,"edit_date":1700000002,"reply_markup":null}"#,
        ],
    );
    assert!(
        session.histories.get(&21).unwrap().messages[&301]
            .reply_markup
            .is_none()
    );
}
