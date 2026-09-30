//! State reducer tests: bots.
use super::common::*;
use super::*;

#[test]
fn b2_bot_start_link_parser_only_accepts_start_links() {
    // Slice B2: `t.me/<bot>?start=<param>` parses to
    // `internalLinkTypeBotStart`'s two pieces (schema 1.8.67, line
    // 9399); anything else is not a bot-start link.
    assert_eq!(
        parse_bot_start_link("https://t.me/demo_bot?start=demo_xyz"),
        Some(("demo_bot".into(), "demo_xyz".into()))
    );
    assert_eq!(
        parse_bot_start_link("t.me/demo_bot?start=demo_xyz&foo=bar"),
        Some(("demo_bot".into(), "demo_xyz".into()))
    );
    assert_eq!(parse_bot_start_link("https://t.me/demo_bot?start="), None);
    assert_eq!(parse_bot_start_link("https://t.me/demo_bot"), None);
    assert_eq!(
        parse_bot_start_link("https://t.me/demo_bot?startattach=x"),
        None
    );
    assert_eq!(
        parse_bot_start_link("https://t.me/s/demo_bot?start=x"),
        None
    );
    assert_eq!(
        parse_bot_start_link("https://example.com/demo_bot?start=x"),
        None
    );
}

#[test]
fn b2_similar_bots_users_response_lands_by_pending_user() {
    // Slice B2: a `users` answer to our `getBotSimilarBots` fetch is
    // keyed by the pending request's `user_id`; strangers' `users`
    // payloads don't touch it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(RequestPurpose::GetBotSimilarBots, 21);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"users","@extra":"{}","total_count":2,"user_ids":[31,32]}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.similar_bots.get(&21),
        Some(SimilarBotsFetch::Loaded(ids)) if ids == &[31, 32]
    ));
}

#[test]
fn b2_start_and_similar_bots_errors_surface() {
    // Slice B2: a refused `sendBotStartMessage` / `getBotSimilarBots`
    // surfaces in `chat_action_error` — never shown as success.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::SendBotStartMessage, Some(ChatId(21)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BOT_START_FAILED"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_action_error.as_deref(),
        Some("could not start the bot (error 400)")
    );
    let extra = session.request_for_user(RequestPurpose::GetBotSimilarBots, 21);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":403,"message":"FORBIDDEN"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_action_error.as_deref(),
        Some("could not load similar bots (error 403)")
    );
}

#[test]
fn b1_login_url_info_error_degrades_to_url() {
    // B1: a refused `getLoginUrlInfo` degrades the login button to a
    // plain URL button press carrying the raw URL (schema 1.8.67 doc
    // on `getLoginUrl`). Same for a refused `getLoginUrl`.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let login_request = LoginUrlRequest {
        chat_id: ChatId(21),
        message_id: MessageId(301),
        button_id: 11,
        raw_url: "https://example.com/login".to_string(),
    };
    for purpose in [RequestPurpose::GetLoginUrlInfo, RequestPurpose::GetLoginUrl] {
        session.login_url_request = Some(login_request.clone());
        let extra = session.request(purpose, Some(ChatId(21)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BUTTON_ID_INVALID"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.last_login_url_info,
            Some(LoginUrlInfo::Failed {
                fallback_url: "https://example.com/login".to_string()
            })
        );
    }
}

#[test]
fn b1_get_login_url_http_url_opens() {
    // B1: a `getLoginUrl` answer (`httpUrl`, schema:7458) with the
    // `GetLoginUrl` pending purpose lands as `LoginUrlInfo::Open` for
    // the UI drain.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let extra = session.request(RequestPurpose::GetLoginUrl, Some(ChatId(21)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"httpUrl","@extra":"{}","url":"https://example.com/authed"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.last_login_url_info,
        Some(LoginUrlInfo::Open {
            url: "https://example.com/authed".to_string()
        })
    );
}

#[test]
fn b1_active_custom_keyboard_rules() {
    // B1: newest `replyMarkupShowKeyboard` wins; a newer
    // `replyMarkupRemoveKeyboard` clears; a dismissed one-time
    // keyboard stays hidden.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let show = |id: i64| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":21,"is_outgoing":false,"reply_markup":{{"@type":"replyMarkupShowKeyboard","rows":[[{{"@type":"keyboardButton","text":"Yes","type":{{"@type":"keyboardButtonTypeText"}}}}]],"is_persistent":false,"resize_keyboard":false,"one_time":true,"is_personal":false,"force_reply":false,"input_field_placeholder":""}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}}}"#
        )
    };
    let remove = |id: i64| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":21,"is_outgoing":false,"reply_markup":{{"@type":"replyMarkupRemoveKeyboard","is_personal":false}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}}}"#
        )
    };
    apply_json(&mut session, &seq, &sink, &show(401));
    apply_json(&mut session, &seq, &sink, &show(402));
    let messages = &session.histories.get(&21).expect("history").messages;
    let none: HashSet<(i64, i64)> = HashSet::new();
    // Newest show-keyboard message wins.
    let active = active_custom_keyboard(messages, &none).expect("keyboard");
    assert_eq!(active.1, MessageId(402));
    // A dismissed one-time keyboard stays hidden.
    let dismissed: HashSet<(i64, i64)> = [(21, 402)].into_iter().collect();
    assert!(active_custom_keyboard(messages, &dismissed).is_none());
    // A newer remove-keyboard clears everything.
    apply_json(&mut session, &seq, &sink, &remove(403));
    let messages = &session.histories.get(&21).expect("history").messages;
    assert!(active_custom_keyboard(messages, &none).is_none());
}

/// Bots slice: a `searchPublicChat` answer for `@botname` resolves
/// the bot user id; the cached `is_inline` flag rides along. The
/// answer is a raw `chat` object (not `updateNewChat`) — this is the
/// shape TDLib actually returns for `searchPublicChat`.
#[test]
fn resolve_inline_bot_private_chat_resolves_with_cached_inline_flag() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.users.insert(
        77,
        ParsedUser {
            id: 77,
            username: "gif".into(),
            is_bot: true,
            is_inline: true,
            ..Default::default()
        },
    );
    let extra = session.request(RequestPurpose::ResolveInlineBot { generation: 1 }, None);
    session.inline_bot_resolve = Some(InlineBotResolve::Resolving {
        username: "gif".into(),
        generation: 1,
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chat","@extra":"{}","id":5,"title":"gif","type":{{"@type":"chatTypePrivate","user_id":77}},"unread_count":0}}"#,
            extra.0,
        ),
    );
    assert_eq!(
        session.inline_bot_resolve,
        Some(InlineBotResolve::Resolved {
            username: "gif".into(),
            user_id: 77,
            is_inline: Some(true),
        })
    );
}

/// Bots slice: any public username resolves to a private chat — a
/// cached non-bot must fail the resolve instead of being treated as
/// a bot.
#[test]
fn resolve_inline_bot_non_bot_username_fails() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.users.insert(
        78,
        ParsedUser {
            id: 78,
            username: "alice".into(),
            is_bot: false,
            ..Default::default()
        },
    );
    let extra = session.request(RequestPurpose::ResolveInlineBot { generation: 1 }, None);
    session.inline_bot_resolve = Some(InlineBotResolve::Resolving {
        username: "alice".into(),
        generation: 1,
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewChat","@extra":"{}","chat":{{"id":6,"title":"alice","type":{{"@type":"chatTypePrivate","user_id":78}},"unread_count":0}}}}"#,
            extra.0,
        ),
    );
    assert!(
        matches!(
            &session.inline_bot_resolve,
            Some(InlineBotResolve::Failed { reason, .. }) if reason.contains("not a bot")
        ),
        "{:?}",
        session.inline_bot_resolve
    );
}

/// Bots slice: a stale `searchPublicChat` answer (an older
/// generation — the user kept typing a new username) must not
/// overwrite the newer resolution.
#[test]
fn resolve_inline_bot_stale_answer_ignored() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::ResolveInlineBot { generation: 1 }, None);
    // A newer resolve already replaced the slot.
    session.inline_bot_resolve = Some(InlineBotResolve::Resolving {
        username: "gifs".into(),
        generation: 2,
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewChat","@extra":"{}","chat":{{"id":5,"title":"gif","type":{{"@type":"chatTypePrivate","user_id":77}},"unread_count":0}}}}"#,
            extra.0,
        ),
    );
    assert_eq!(
        session.inline_bot_resolve,
        Some(InlineBotResolve::Resolving {
            username: "gifs".into(),
            generation: 2,
        })
    );
}

/// Bots slice: a failed `searchPublicChat` (unknown username) lands
/// an honest hint in the resolve slot.
#[test]
fn resolve_inline_bot_error_fails_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::ResolveInlineBot { generation: 3 }, None);
    session.inline_bot_resolve = Some(InlineBotResolve::Resolving {
        username: "nosuchbot".into(),
        generation: 3,
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"USERNAME_NOT_OCCUPIED"}}"#,
            extra.0,
        ),
    );
    assert!(
        matches!(
            &session.inline_bot_resolve,
            Some(InlineBotResolve::Failed { username, .. }) if username == "nosuchbot"
        ),
        "{:?}",
        session.inline_bot_resolve
    );
}
