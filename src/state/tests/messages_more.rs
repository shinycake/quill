//! State reducer tests: messages (continued from messages.rs).
use super::common::*;
use super::*;
use crate::telegram::envelope::MessagesPayload;

#[test]
fn forward_messages_result_upserts_dest_and_labels_origin() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"2","is_pinned":false}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":12,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"1","is_pinned":false}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"already forwarded","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1}}}"#,
    );
    let dests: Vec<_> = session
        .forward_destinations("bo")
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(dests, vec![ChatId(12)]);
    assert!(
        session
            .forward_destinations("")
            .iter()
            .all(|c| c.supported())
    );
    let extra = session.request(RequestPurpose::ForwardMessages, Some(ChatId(12)));
    session.in_flight_forward = Some(ForwardFlight {
        extra,
        dest_chat_id: ChatId(12),
        from_chat_id: ChatId(11),
        requested: 1,
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":80,"chat_id":12,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":11}},"date":1}}}}]}}"#,
            extra.0
        ),
    );
    let result = session.last_forward.as_ref().expect("forward result");
    assert_eq!(result.dest_chat_id, ChatId(12));
    assert_eq!(result.dest_title, "Bob");
    assert_eq!(result.forwarded_ids, vec![MessageId(80)]);
    assert_eq!(result.success_label(), "Forwarded to Bob");
    let dest = session
        .histories
        .get(&12)
        .unwrap()
        .messages
        .get(&80)
        .unwrap();
    assert_eq!(
        session.forward_from_label(dest.forward_info.as_ref().unwrap()),
        "Forwarded from Alice"
    );
    let incoming = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&105)
        .unwrap();
    assert_eq!(
        session.forward_from_label(incoming.forward_info.as_ref().unwrap()),
        "Forwarded from Ada Lovelace"
    );
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn interaction_info_update_sets_chips_and_own_highlight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}}"#,
    );
    let incoming = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&101)
        .unwrap();
    assert!(incoming.can_react());
    assert_eq!(incoming.emoji_reaction_chips().len(), 1);
    assert!(!incoming.chosen_emoji("❤"));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
    );
    let updated = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&101)
        .unwrap();
    let chips = updated.emoji_reaction_chips();
    assert_eq!(chips[0].chip_label().as_deref(), Some("❤ 3"));
    assert!(chips[0].is_chosen);
    assert_eq!(chips[1].chip_label().as_deref(), Some("👍 2"));
    assert!(!chips[1].is_chosen);
    assert!(updated.chosen_emoji("❤"));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":null}"#,
    );
    let cleared = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&101)
        .unwrap();
    assert!(cleared.emoji_reaction_chips().is_empty());
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn message_is_pinned_update_and_newest_pinned() {
    let sink = Arc::new(MemorySink::new());
    let mut session = Session::new(AccountKey::primary(), sink.clone());
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":100,"chat_id":11,"is_outgoing":false,"is_pinned":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"older","entities":[]}}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"is_pinned":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
    );
    session.open_chat = Some(ChatId(11));
    let pinned = session
        .histories
        .get(&11)
        .unwrap()
        .messages
        .get(&101)
        .unwrap();
    assert!(pinned.is_pinned);
    assert!(pinned.can_pin());
    assert_eq!(
        session.open_chat_pinned_message().map(|m| m.id),
        Some(MessageId(101))
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":false}"#,
    );
    assert!(
        !session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap()
            .is_pinned
    );
    assert!(session.open_chat_pinned_message().is_none());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":100,"is_pinned":true}"#,
    );
    assert_eq!(
        session.open_chat_pinned_message().map(|m| m.id),
        Some(MessageId(100))
    );
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn update_poll_refreshes_counts_and_chosen_marks() {
    // Phase 4.2: `updatePoll` carries no chat/message id — the reducer
    // scans loaded histories and replaces the matching `poll.id` in
    // place (vote counts, percentages, chosen marks).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":106,"chat_id":15,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":12,"vote_percentage":55,"is_chosen":true},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":7,"vote_percentage":32,"is_chosen":false}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updatePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":13,"vote_percentage":56,"is_chosen":false},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":10,"vote_percentage":43,"is_chosen":true}],"total_voter_count":23,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":true,"type":{"@type":"pollTypeRegular"}}}"#,
    );
    let history = session.histories.get(&15).unwrap();
    let message = history.messages.get(&106).unwrap();
    let MessageContent::Poll(poll_content) = &message.content else {
        panic!("{:?}", message.content);
    };
    let poll = &poll_content.poll;
    assert_eq!(poll.total_voter_count, 23);
    assert_eq!(poll.options[1].voter_count, 10);
    assert_eq!(poll.options[1].vote_percentage, 43);
    assert!(!poll.options[0].is_chosen);
    assert!(poll.options[1].is_chosen);
    assert!(poll.is_closed);
}

#[test]
fn update_poll_reaches_polls_loaded_by_every_path() {
    // `updatePoll` is matched by poll id through an index: rows loaded
    // from a history page, a sent message and an edit that turned a row
    // into a poll must all be found, and a deleted row must not be.
    let poll = |id: i64, voters: i32| {
        format!(
            r#"{{"@type":"poll","id":{id},"question":{{"@type":"formattedText","text":"Q","entities":[]}},"options":[{{"@type":"pollOption","id":"a","text":{{"@type":"formattedText","text":"A","entities":[]}},"voter_count":{voters},"vote_percentage":100,"is_chosen":false}}],"total_voter_count":{voters},"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{{"@type":"pollTypeRegular"}}}}"#
        )
    };
    let poll_content = |id: i64| {
        format!(
            r#"{{"@type":"messagePoll","poll":{},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}"#,
            poll(id, 1)
        )
    };
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(15));
    let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(15)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":3,"messages":[{{"id":30,"chat_id":15,"is_outgoing":false,"content":{}}},{{"id":20,"chat_id":15,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"t","entities":[]}}}}}},{{"id":10,"chat_id":15,"is_outgoing":false,"content":{}}}]}}"#,
            extra.0,
            poll_content(1),
            poll_content(4)
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateMessageSendSucceeded","old_message_id":-5,"message":{{"id":40,"chat_id":15,"is_outgoing":true,"content":{}}}}}"#,
            poll_content(2)
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateMessageContent","chat_id":15,"message_id":20,"new_content":{}}}"#,
            poll_content(3)
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":15,"message_ids":[10],"is_permanent":true,"from_cache":false}"#,
    );
    let mut updated = 0;
    for id in 1..=4 {
        let owned = copy_and_parse(
            &format!(r#"{{"@type":"updatePoll","poll":{}}}"#, poll(id, 9)),
            &seq,
            &(sink.clone() as Arc<dyn DiagnosticSink>),
        )
        .unwrap();
        let EnvelopePayload::Messages(MessagesPayload::UpdatePoll { poll }) =
            owned.envelope.payload
        else {
            panic!("updatePoll");
        };
        updated += session.apply_update_poll(poll);
    }
    assert_eq!(updated, 3);
    let history = &session.histories[&15];
    for id in [20, 30, 40] {
        let MessageContent::Poll(content) = &history.messages[&id].content else {
            panic!("poll row {id}");
        };
        assert_eq!(content.poll.total_voter_count, 9, "row {id}");
    }
    assert!(!history.messages.contains_key(&10));
}

#[test]
fn update_poll_with_unknown_id_updates_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updatePoll","poll":{"@type":"poll","id":9999,"question":{"@type":"formattedText","text":"Ghost","entities":[]},"options":[],"total_voter_count":0,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":false,"type":{"@type":"pollTypeRegular"}}}"#,
    );
    assert!(session.histories.values().all(|h| h.messages.is_empty()));
}

#[test]
fn can_pin_messages_rights_gate() {
    // MED1: pin rights — private chats always; creator always; admins
    // need the explicit right; group members need the member right.
    let mut private = placeholder_chat(ChatId(20));
    private.kind = ChatKind::Private {
        user_id: crate::ids::UserId(9),
    };
    assert!(private.can_pin_messages());

    let mut creator = placeholder_chat(ChatId(21));
    creator.kind = ChatKind::Supergroup {
        supergroup_id: 21,
        is_channel: false,
    };
    creator.set_member_status(ChannelMemberStatus::Creator, None);
    assert!(creator.can_pin_messages());

    let mut admin = placeholder_chat(ChatId(22));
    admin.kind = ChatKind::BasicGroup { basic_group_id: 22 };
    admin.set_member_status(ChannelMemberStatus::Administrator, None);
    assert!(
        !admin.can_pin_messages(),
        "absent rights block keeps the gate closed"
    );
    admin.set_admin_can_pin_messages(Some(true));
    assert!(admin.can_pin_messages());
    admin.set_admin_can_pin_messages(Some(false));
    assert!(!admin.can_pin_messages());

    let mut member = placeholder_chat(ChatId(23));
    member.kind = ChatKind::BasicGroup { basic_group_id: 23 };
    member.set_member_status(ChannelMemberStatus::Member, None);
    assert!(!member.can_pin_messages(), "no permissions block → closed");
    member.permissions = Some(ChatPermissions::all());
    assert!(member.can_pin_messages());
    member.permissions.as_mut().unwrap().can_pin_messages = false;
    assert!(!member.can_pin_messages());
}

#[test]
fn b1_force_reply_arms_pending_target() {
    // B1: an incoming message with `replyMarkupForceReply` arms the
    // composer's reply-to; outgoing or plain messages do not.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":306,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupForceReply","input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
    );
    assert_eq!(
        session.pending_force_reply,
        Some(ForceReplyTarget {
            chat_id: ChatId(21),
            message_id: MessageId(306),
        })
    );
    // Outgoing force-reply does not arm (bots demand replies; we don't).
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":307,"chat_id":21,"is_outgoing":true,"reply_markup":{"@type":"replyMarkupForceReply","input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"y","entities":[]}}}}"#,
    );
    assert_eq!(
        session.pending_force_reply,
        Some(ForceReplyTarget {
            chat_id: ChatId(21),
            message_id: MessageId(306),
        })
    );
    // A plain incoming message leaves the armed target alone.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":308,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"z","entities":[]}}}}"#,
    );
    assert_eq!(
        session.pending_force_reply,
        Some(ForceReplyTarget {
            chat_id: ChatId(21),
            message_id: MessageId(306),
        })
    );
}

#[test]
fn delete_account_ok_clears_mutating_without_local_teardown() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.account_mutating = true;
    session.settings.account_error = Some("stale".into());
    let extra = session.request(RequestPurpose::DeleteAccount, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.settings.account_mutating);
    assert!(session.settings.account_error.is_none());
}

#[test]
fn delete_account_error_surfaces_honestly() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.account_mutating = true;
    let extra = session.request(RequestPurpose::DeleteAccount, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PASSWORD_HASH_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(!session.settings.account_mutating);
    assert_eq!(
        session.settings.account_error.as_deref(),
        Some("Could not update the account: Telegram refused the request")
    );
}

#[test]
fn code_send_error_surfaces_honestly() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.auth_state.change_number_phone = Some("+15550199".into());
    session.auth_state.change_number_timeout = Some(60);
    session.auth_state.change_number_loading = true;
    let extra = session.request(RequestPurpose::SendPhoneNumberCode, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PHONE_NUMBER_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(!session.auth_state.change_number_loading);
    assert_eq!(
        session.auth_state.change_number_phone.as_deref(),
        Some("+15550199")
    );
    assert_eq!(session.auth_state.change_number_timeout, Some(60));
    assert_eq!(
        session.auth_state.change_number_error.as_deref(),
        Some("Could not send the verification code: Telegram refused the request")
    );
}

#[test]
fn validated_order_info_selects_first_shipping() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::ValidateOrderInfo, Some(ChatId(51)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"validatedOrderInfo","@extra":"{}","order_info_id":"oid1","shipping_options":[{{"@type":"shippingOption","id":"ship1","title":"Standard","price_parts":[{{"@type":"labeledPricePart","label":"Post","amount":100}}]}},{{"@type":"shippingOption","id":"ship2","title":"Express","price_parts":[]}}]}}"#,
            extra.0
        ),
    );
    let validated = session.payments.validated.as_ref().expect("validated");
    assert_eq!(validated.order_info_id, "oid1");
    assert_eq!(validated.shipping_options.len(), 2);
    assert_eq!(session.payments.shipping_id.as_deref(), Some("ship1"));
}

#[test]
fn sender_identity_survives_history_and_search() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":710,"chat_id":11,"sender_id":{"@type":"messageSenderUser","user_id":42},"content":{"@type":"messageText","text":{"text":"Hello","entities":[]}}}}"#,
    );
    let message = &session.histories[&11].messages[&710];
    assert_eq!(message.sender, Some(MessageSender::User { user_id: 42 }));
    let searched = SearchMessageHit::from_parsed(&crate::telegram::envelope::parse_message(&serde_json::json!({"id":711,"chat_id":11,"sender_id":{"@type":"messageSenderChat","chat_id":99},"content":{"@type":"messageText","text":{"text":"Hello","entities":[]}}})).unwrap()).into_history();
    assert_eq!(searched.sender, Some(MessageSender::Chat { chat_id: 99 }));
}

fn scheduled_entry(id: i64, send_date: i32) -> ParsedMessage {
    use crate::telegram::envelope::MessageSchedulingState;
    ParsedMessage {
        sender: None,
        id: MessageId(id),
        chat_id: ChatId(7),
        date: 0,
        is_outgoing: true,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date }),
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text("later".into()),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    }
}

#[test]
fn reschedule_ok_rewrites_the_scheduled_entry() {
    use crate::composer::ComposerScheduling;
    use crate::telegram::envelope::MessageSchedulingState;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.scheduled_messages.push(scheduled_entry(70, 1000));
    session.scheduled_messages.push(scheduled_entry(71, 2000));
    let extra = session.request(
        RequestPurpose::Messages(MessagesPurpose::EditMessageSchedulingState {
            message_id: MessageId(70),
            scheduling: ComposerScheduling::SendAtDate(5000),
        }),
        Some(ChatId(7)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(
        session.scheduled_messages[0].scheduling_state,
        Some(MessageSchedulingState::SendAtDate { send_date: 5000 })
    );
    assert_eq!(
        session.scheduled_messages[1].scheduling_state,
        Some(MessageSchedulingState::SendAtDate { send_date: 2000 })
    );
}

#[test]
fn send_now_ok_drops_the_scheduled_entry() {
    use crate::composer::ComposerScheduling;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.scheduled_messages.push(scheduled_entry(70, 1000));
    session.scheduled_messages.push(scheduled_entry(71, 2000));
    let extra = session.request(
        RequestPurpose::Messages(MessagesPurpose::EditMessageSchedulingState {
            message_id: MessageId(70),
            scheduling: ComposerScheduling::None,
        }),
        Some(ChatId(7)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    let ids: Vec<i64> = session.scheduled_messages.iter().map(|m| m.id.0).collect();
    assert_eq!(ids, vec![71]);
}

#[test]
fn scheduling_edit_error_keeps_the_entry_and_surfaces() {
    use crate::composer::ComposerScheduling;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.scheduled_messages.push(scheduled_entry(70, 1000));
    let extra = session.request(
        RequestPurpose::Messages(MessagesPurpose::EditMessageSchedulingState {
            message_id: MessageId(70),
            scheduling: ComposerScheduling::None,
        }),
        Some(ChatId(7)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_ID_INVALID"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.scheduled_messages.len(), 1);
    let err = session.resend_error.expect("error surfaced");
    assert!(err.contains("Could not send the message now"), "{err}");
}

#[test]
fn has_scheduled_messages_tracks_chat_and_update() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"g","type":{"@type":"chatTypePrivate","user_id":14},"unread_count":0,"has_scheduled_messages":true}}"#,
    );
    assert!(session.chat_has_scheduled_messages(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatHasScheduledMessages","chat_id":14,"has_scheduled_messages":false}"#,
    );
    assert!(!session.chat_has_scheduled_messages(ChatId(14)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatHasScheduledMessages","chat_id":15,"has_scheduled_messages":true}"#,
    );
    assert!(session.chat_has_scheduled_messages(ChatId(15)));
}
