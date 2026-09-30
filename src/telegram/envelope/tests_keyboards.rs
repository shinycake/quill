use super::*;

#[test]
fn bot_commands_parsed_from_get_commands_response() {
    // `botCommands` (schema 1.8.67 line 829): the `getCommands`
    // response — `bot_user_id:int53` plus a bare
    // `vector<botCommand>`, parsed with the same `parse_bot_command`
    // as `botInfo.commands`.
    let json = r#"{"@type":"botCommands","@extra":"9","bot_user_id":21,"commands":[{"@type":"botCommand","command":"settings","description":"Tweak the bot","is_ephemeral":false},{"@type":"botCommand","command":"help","description":"","is_ephemeral":false}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::BotCommands {
            bot_user_id,
            commands,
        } => {
            assert_eq!(bot_user_id.0, 21);
            assert_eq!(commands.len(), 2);
            assert_eq!(commands[0].command, "settings");
            assert_eq!(commands[0].description, "Tweak the bot");
            assert_eq!(commands[1].command, "help");
            assert_eq!(commands[1].description, "");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn bot_commands_keep_ephemeral_flag() {
    // `botCommand.is_ephemeral` (schema 1.8.67 line 826) must survive
    // decoding — the command menu icon reads it. Absent → false (older
    // payloads).
    let json = r#"{"@type":"botCommands","@extra":"9","bot_user_id":21,"commands":[{"@type":"botCommand","command":"secret","description":"Only you see this","is_ephemeral":true},{"@type":"botCommand","command":"start","description":"Start"}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::BotCommands { commands, .. } => {
            assert_eq!(commands.len(), 2);
            assert!(commands[0].is_ephemeral);
            assert!(!commands[1].is_ephemeral);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn bot_commands_parsed_without_command_list() {
    // Missing/null `commands` degrades to an empty list rather than a
    // parse failure — the menu then simply shows no global rows.
    let json = r#"{"@type":"botCommands","@extra":"9","bot_user_id":21}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::BotCommands {
            bot_user_id,
            commands,
        } => {
            assert_eq!(bot_user_id.0, 21);
            assert!(commands.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn bot_commands_rejects_missing_bot_user_id() {
    // `bot_user_id:int53` is a required schema field (line 829) —
    // a response without it is a parse error, not a cache entry
    // under user id 0.
    let json = r#"{"@type":"botCommands","@extra":"9","commands":[]}"#;
    assert!(parse_envelope(json).is_err());
}

#[test]
fn inline_keyboard_parsed_from_reply_markup() {
    // `replyMarkupInlineKeyboard` (schema 1.8.67 line 3855):
    // `rows` is a vector of rows; `inlineKeyboardButton` (line 3828)
    // carries `text`, `style:ButtonStyle`, `type`.
    let json = r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Open","icon_custom_emoji_id":0,"style":{"@type":"buttonStylePrimary"},"type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com"}},{"@type":"inlineKeyboardButton","text":"Tap me","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}}],[{"@type":"inlineKeyboardButton","text":"Search","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleLink"},"type":{"@type":"inlineKeyboardButtonTypeSwitchInline","query":"pic","target_chat":{"@type":"targetChatCurrent"}}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick one","entities":[]}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let ReplyMarkup::InlineKeyboard(keyboard) = message.reply_markup.expect("reply_markup")
            else {
                panic!("expected inline keyboard");
            };
            assert_eq!(keyboard.rows.len(), 2);
            assert_eq!(keyboard.rows[0].len(), 2);
            let open = &keyboard.rows[0][0];
            assert_eq!(open.text, "Open");
            assert_eq!(open.style, InlineKeyboardButtonStyle::Primary);
            assert_eq!(
                open.kind,
                InlineKeyboardButtonType::Url {
                    url: "https://example.com".to_string()
                }
            );
            let tap = &keyboard.rows[0][1];
            assert_eq!(tap.style, InlineKeyboardButtonStyle::Default);
            assert_eq!(
                tap.kind,
                InlineKeyboardButtonType::Callback {
                    data: vec![1, 2, 3]
                }
            );
            let search = &keyboard.rows[1][0];
            assert_eq!(search.style, InlineKeyboardButtonStyle::Link);
            assert_eq!(
                search.kind,
                InlineKeyboardButtonType::SwitchInline {
                    query: "pic".to_string(),
                    target: InlineKeyboardTargetChat::Current,
                }
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn inline_keyboard_tolerates_unknown_types() {
    // Unknown button `@type`, unknown `style`, missing `type`, and a
    // non-keyboard `reply_markup` must never crash the parse; unknown
    // buttons become `Unknown` (rendered disabled) and other markups are
    // ignored.
    let json = r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Mystery","style":{"@type":"buttonStyleFuture"},"type":{"@type":"inlineKeyboardButtonTypeQuantum"}}],[{"@type":"inlineKeyboardButton","text":"No type here"}],"not an array"],"force_reply":true},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let ReplyMarkup::InlineKeyboard(keyboard) = message.reply_markup.expect("reply_markup")
            else {
                panic!("expected inline keyboard");
            };
            // The `"not an array"` row is skipped; tolerance never crashes.
            assert_eq!(keyboard.rows.len(), 2);
            let mystery = &keyboard.rows[0][0];
            assert_eq!(mystery.style, InlineKeyboardButtonStyle::Default);
            assert_eq!(
                mystery.kind,
                InlineKeyboardButtonType::Unknown {
                    type_name: "inlineKeyboardButtonTypeQuantum".to_string()
                }
            );
            assert!(matches!(
                keyboard.rows[1][0].kind,
                InlineKeyboardButtonType::Unknown { .. }
            ));
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":303,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[],"is_persistent":false,"resize_keyboard":false,"one_time":false,"is_personal":false,"force_reply":false,"input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            // B1: custom keyboards are parsed now (not ignored).
            assert!(matches!(
                message.reply_markup,
                Some(ReplyMarkup::ShowKeyboard(_))
            ));
        }
        other => panic!("{other:?}"),
    }
    // Absent / null `reply_markup` → None.
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":304,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            assert!(message.reply_markup.is_none());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn callback_query_answer_parsed() {
    // `getCallbackQueryAnswer` response (schema 1.8.67 line 7747).
    let env = parse_envelope(
            r#"{"@type":"callbackQueryAnswer","@extra":"9","text":"Done!","show_alert":false,"url":""}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::CallbackQueryAnswer(answer) => {
            assert_eq!(answer.text, "Done!");
            assert!(!answer.show_alert);
            assert!(answer.url.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn b1_show_keyboard_parsed() {
    // B1: `replyMarkupShowKeyboard` (schema 1.8.67, line 3850).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":305,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[[{"@type":"keyboardButton","text":"Yes","type":{"@type":"keyboardButtonTypeText"}},{"@type":"keyboardButton","text":"Contact","type":{"@type":"keyboardButtonTypeRequestPhoneNumber"}}]],"is_persistent":true,"resize_keyboard":true,"one_time":true,"is_personal":false,"force_reply":false,"input_field_placeholder":"Pick"},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            match message.reply_markup.expect("show keyboard markup") {
                ReplyMarkup::ShowKeyboard(keyboard) => {
                    assert_eq!(keyboard.rows.len(), 1);
                    assert_eq!(keyboard.rows[0].len(), 2);
                    assert_eq!(keyboard.rows[0][0].kind, KeyboardButtonType::Text);
                    assert_eq!(
                        keyboard.rows[0][1].kind,
                        KeyboardButtonType::RequestPhoneNumber
                    );
                    assert!(keyboard.is_persistent);
                    assert!(keyboard.resize_keyboard);
                    assert!(keyboard.one_time);
                    assert_eq!(keyboard.placeholder, "Pick");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn b1_force_reply_and_remove_keyboard_parsed() {
    // `replyMarkupForceReply` (schema:3840), `replyMarkupRemoveKeyboard`
    // (schema:3835).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":306,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupForceReply","input_field_placeholder":"Reply…"},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            assert!(matches!(
                message.reply_markup,
                Some(ReplyMarkup::ForceReply { .. })
            ));
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":307,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupRemoveKeyboard","is_personal":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            assert!(matches!(
                message.reply_markup,
                Some(ReplyMarkup::RemoveKeyboard)
            ));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn b1_reply_markup_demands_reply_cases() {
    assert!(reply_markup_demands_reply(&ReplyMarkup::ForceReply {
        placeholder: String::new()
    }));
    assert!(reply_markup_demands_reply(&ReplyMarkup::ShowKeyboard(
        ReplyKeyboard {
            rows: vec![],
            is_persistent: false,
            resize_keyboard: false,
            one_time: false,
            is_personal: false,
            force_reply: true,
            placeholder: String::new(),
        }
    )));
    assert!(!reply_markup_demands_reply(&ReplyMarkup::ShowKeyboard(
        ReplyKeyboard {
            rows: vec![],
            is_persistent: false,
            resize_keyboard: false,
            one_time: false,
            is_personal: false,
            force_reply: false,
            placeholder: String::new(),
        }
    )));
    assert!(!reply_markup_demands_reply(&ReplyMarkup::InlineKeyboard(
        InlineKeyboard {
            rows: vec![],
            force_reply: false,
        }
    )));
    assert!(!reply_markup_demands_reply(&ReplyMarkup::RemoveKeyboard));
}

#[test]
fn b1_login_url_info_parsed() {
    // `getLoginUrlInfo` answers (schema 1.8.67, line 12985).
    let env = parse_envelope(
        r#"{"@type":"loginUrlInfoOpen","@extra":"11","url":"https://example.com/authed"}"#,
    )
    .unwrap();
    match env.payload {
        EnvelopePayload::LoginUrlInfo(LoginUrlInfo::Open { url }) => {
            assert_eq!(url, "https://example.com/authed");
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(
            r#"{"@type":"loginUrlInfoRequestConfirmation","@extra":"12","domain":"example.com","bot_user_id":21,"request_write_access":true}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::LoginUrlInfo(LoginUrlInfo::RequestConfirmation {
            domain,
            request_write_access,
        }) => {
            assert_eq!(domain, "example.com");
            assert!(request_write_access);
        }
        other => panic!("{other:?}"),
    }
    // B1: `getLoginUrl` answers arrive as `httpUrl` (schema:7458) — the
    // pre-existing `EnvelopePayload::HttpUrl` variant, which
    // `Session::apply_payload` maps to `LoginUrlInfo::Open` when the
    // pending purpose is `GetLoginUrl`.
    let env =
        parse_envelope(r#"{"@type":"httpUrl","@extra":"13","url":"https://example.com/authed2"}"#)
            .unwrap();
    match env.payload {
        EnvelopePayload::HttpUrl { url } => {
            assert_eq!(url, "https://example.com/authed2");
        }
        other => panic!("{other:?}"),
    }
}
