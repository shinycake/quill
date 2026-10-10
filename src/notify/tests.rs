use super::*;
use crate::telegram::envelope::{MessageContent, TextContent};

fn test_message(chat_id: i64, id: i64, outgoing: bool, text: &str) -> ParsedMessage {
    ParsedMessage {
        sender: None,
        id: MessageId(id),
        chat_id: ChatId(chat_id),
        date: 0,
        is_outgoing: outgoing,
        is_pinned: false,
        media_album_id: 0,
        author_signature: None,
        ephemeral: None,
        topic_id: None,
        thread_id: None,
        content: MessageContent::Text(TextContent::plain(text)),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
        scheduling_state: None,
        can_retry: false,
        send_state: Default::default(),
    }
}

fn input<'a>(
    message: &'a ParsedMessage,
    app_active: bool,
    open_chat: Option<ChatId>,
) -> NotifyInput<'a> {
    NotifyInput {
        message,
        chat_title: Some("Ada"),
        chat_muted: false,
        last_read_inbox_message_id: Some(MessageId(0)),
        open_chat,
        app_active,
        hide_previews: false,
        chat_preview_allowed: true,
        pinned_allowed: true,
        contact_joined_allowed: true,
    }
}

#[test]
fn background_incoming_message_notifies_with_preview() {
    let message = test_message(7, 42, false, "hello there");
    let notification = decide_notify(&input(&message, false, None)).expect("notify");
    assert_eq!(notification.chat_id, ChatId(7));
    assert_eq!(notification.title, "Ada");
    assert_eq!(notification.body, "hello there");
}

#[test]
fn foreground_other_chat_still_notifies() {
    // "or the chat isn't open": app active but a different chat is open.
    let message = test_message(7, 42, false, "hello");
    let notification = decide_notify(&input(&message, true, Some(ChatId(9)))).expect("notify");
    assert_eq!(notification.title, "Ada");
}

#[test]
fn background_open_chat_still_notifies() {
    // The chat is "open" server-side but the user isn't looking at the app.
    let message = test_message(7, 42, false, "hello");
    let notification = decide_notify(&input(&message, false, Some(ChatId(7)))).expect("notify");
    assert_eq!(notification.title, "Ada");
}

#[test]
fn foreground_open_chat_does_not_notify() {
    let message = test_message(7, 42, false, "hello");
    assert!(decide_notify(&input(&message, true, Some(ChatId(7)))).is_none());
}

#[test]
fn outgoing_message_does_not_notify() {
    let message = test_message(7, 42, true, "hello");
    assert!(decide_notify(&input(&message, false, None)).is_none());
}

#[test]
fn muted_chat_does_not_notify() {
    let message = test_message(7, 42, false, "hello");
    let mut ctx = input(&message, false, None);
    ctx.chat_muted = true;
    assert!(decide_notify(&ctx).is_none());
}

#[test]
fn already_read_message_does_not_notify() {
    let message = test_message(7, 42, false, "hello");
    let mut ctx = input(&message, false, None);
    ctx.last_read_inbox_message_id = Some(MessageId(42));
    assert!(decide_notify(&ctx).is_none());
    // Strictly newer than the read marker still notifies.
    let newer = test_message(7, 43, false, "hello");
    let mut ctx = input(&newer, false, None);
    ctx.last_read_inbox_message_id = Some(MessageId(42));
    assert!(decide_notify(&ctx).is_some());
}

#[test]
fn unknown_chat_does_not_notify() {
    let message = test_message(7, 42, false, "hello");
    let mut ctx = input(&message, false, None);
    ctx.chat_title = None;
    assert!(decide_notify(&ctx).is_none());
}

#[test]
fn hidden_previews_show_generic_body() {
    let message = test_message(7, 42, false, "secret text");
    let mut ctx = input(&message, false, None);
    ctx.hide_previews = true;
    let notification = decide_notify(&ctx).expect("notify");
    assert_eq!(notification.title, "Ada");
    assert_eq!(notification.body, GENERIC_BODY);
}

#[test]
fn per_chat_preview_disabled_shows_generic_body() {
    let message = test_message(7, 42, false, "secret text");
    let mut ctx = input(&message, false, None);
    ctx.chat_preview_allowed = false;
    let notification = decide_notify(&ctx).expect("notify");
    assert_eq!(notification.body, GENERIC_BODY);
}

#[test]
fn empty_preview_falls_back_to_generic_body() {
    let message = test_message(7, 42, false, "   ");
    let notification = decide_notify(&input(&message, false, None)).expect("notify");
    assert_eq!(notification.body, GENERIC_BODY);
}

#[test]
fn locked_notifications_reveal_no_sender_or_text() {
    let mut queue = Vec::new();
    coalesce_notification(
        &mut queue,
        OsNotification {
            chat_id: ChatId(7),
            title: "Ada".into(),
            body: "my secret".into(),
        },
    );
    let shown = queue[0].for_locked_display();
    assert_eq!(shown.chat_id, ChatId(7));
    assert!(!shown.title.contains("Ada") && !shown.body.contains("secret"));
    coalesce_notification(
        &mut queue,
        OsNotification {
            chat_id: ChatId(7),
            title: "Ada".into(),
            body: "more".into(),
        },
    );
    assert_eq!(queue[0].for_locked_display().body, "2 new messages");
}

#[test]
fn coalesce_bursts_into_summary() {
    let mut queue = Vec::new();
    let first = OsNotification {
        chat_id: ChatId(7),
        title: "Ada".into(),
        body: "one".into(),
    };
    coalesce_notification(&mut queue, first);
    coalesce_notification(
        &mut queue,
        OsNotification {
            chat_id: ChatId(7),
            title: "Ada".into(),
            body: "two".into(),
        },
    );
    coalesce_notification(
        &mut queue,
        OsNotification {
            chat_id: ChatId(8),
            title: "Noor".into(),
            body: "hi".into(),
        },
    );
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].count, 2);
    assert_eq!(queue[0].for_display().body, "2 new messages");
    assert_eq!(queue[0].for_display().title, "Ada");
    assert_eq!(queue[1].count, 1);
    assert_eq!(queue[1].for_display().body, "hi");
}

#[test]
fn linux_command_shape() {
    let notification = OsNotification {
        chat_id: ChatId(7),
        title: "Ada".into(),
        body: "hello".into(),
    };
    let cmd = linux_notify_send_command(&notification);
    assert_eq!(cmd.program, "notify-send");
    assert_eq!(
        cmd.args,
        vec![
            "--app-name=Quill",
            "--wait",
            "--action=default=Open",
            "--action=reply=Reply",
            "--action=read=Mark as read",
            "--",
            "Ada",
            "hello",
        ]
    );
    assert!(cmd.report_click);
}

#[test]
fn notification_tag_roundtrips() {
    let tag = notification_tag("primary", ChatId(-1001234));
    assert_eq!(tag, "account:primary:chat:-1001234");
    assert_eq!(
        parse_notification_tag(&tag),
        Some(("account:primary", ChatId(-1001234)))
    );
    assert_eq!(parse_notification_tag("account:x:chat:nope"), None);
    assert_eq!(parse_notification_tag("no-chat-here"), None);
}

#[test]
fn aumid_key_is_under_classes_appusermodelid() {
    assert_eq!(
        aumid_registry_key("org.shinycake.quill"),
        r"Software\Classes\AppUserModelId\org.shinycake.quill"
    );
}

#[test]
fn native_backend_spawns_no_command() {
    let notification = OsNotification {
        chat_id: ChatId(7),
        title: "Ada".into(),
        body: "hello".into(),
    };
    let native = cfg!(any(target_os = "macos", target_os = "windows"));
    assert_eq!(current_backend() == NotifyBackend::Native, native);
    if native {
        assert!(build_notification_command(&notification).is_none());
    }
}

#[test]
fn failed_spawn_never_clicks() {
    let cmd = NotificationCommand {
        program: "quill-definitely-not-a-real-binary".to_string(),
        args: Vec::new(),
        report_click: true,
    };
    assert!(!run_notification_command(&cmd).clicked);
}

fn sound_input(app_active: bool) -> SoundInput {
    SoundInput {
        app_active,
        chat_muted: false,
        use_default_sound: true,
        chat_sound_id: 0,
        scope_sound_id: None,
    }
}

#[test]
fn focused_window_never_plays_sound() {
    assert!(decide_notification_sound(&sound_input(true)).is_none());
}

#[test]
fn muted_chat_never_plays_sound() {
    let mut input = sound_input(false);
    input.chat_muted = true;
    assert!(decide_notification_sound(&input).is_none());
}

#[test]
fn unknown_scope_plays_default_tone() {
    // `use_default_sound` with scope settings not yet fetched → app default.
    assert_eq!(
        decide_notification_sound(&sound_input(false)),
        Some(NotificationSoundKind::Default)
    );
}

#[test]
fn disabled_chat_sound_is_silent() {
    // Explicit exception sound 0 = disabled (schema line 3350).
    let mut input = sound_input(false);
    input.use_default_sound = false;
    input.chat_sound_id = 0;
    assert!(decide_notification_sound(&input).is_none());
}

#[test]
fn custom_chat_sound_resolves() {
    let mut input = sound_input(false);
    input.use_default_sound = false;
    input.chat_sound_id = 99;
    assert_eq!(
        decide_notification_sound(&input),
        Some(NotificationSoundKind::Custom(99))
    );
}

#[test]
fn scope_sound_minus_one_is_default_tone() {
    // `-1` = app-dependent default (schema line 3368).
    let mut input = sound_input(false);
    input.scope_sound_id = Some(-1);
    assert_eq!(
        decide_notification_sound(&input),
        Some(NotificationSoundKind::Default)
    );
}

#[test]
fn scope_sound_zero_is_silent() {
    let mut input = sound_input(false);
    input.scope_sound_id = Some(0);
    assert!(decide_notification_sound(&input).is_none());
}

#[test]
fn scope_custom_sound_resolves() {
    let mut input = sound_input(false);
    input.scope_sound_id = Some(42);
    assert_eq!(
        decide_notification_sound(&input),
        Some(NotificationSoundKind::Custom(42))
    );
}

#[test]
fn action_ids_roundtrip() {
    for action in NotificationAction::BUTTONS {
        assert_eq!(NotificationAction::from_id(Some(action.id())), action);
    }
    assert_eq!(NotificationAction::from_id(None), NotificationAction::Open);
    assert_eq!(
        NotificationAction::from_id(Some("default")),
        NotificationAction::Open
    );
    assert_eq!(
        NotificationAction::from_id(Some("garbage")),
        NotificationAction::Open
    );
    assert_eq!(NotificationAction::MarkRead.label(), "Mark as read");
}

#[test]
fn locked_apps_offer_no_buttons() {
    assert!(action_buttons(true).is_empty());
    assert_eq!(
        action_buttons(false),
        vec![("reply", "Reply"), ("read", "Mark as read")]
    );
}

#[test]
fn notify_send_output_picks_the_action() {
    assert_eq!(
        parse_notify_send_output("reply\n"),
        Some(NotificationAction::Reply)
    );
    assert_eq!(
        parse_notify_send_output("read"),
        Some(NotificationAction::MarkRead)
    );
    assert_eq!(
        parse_notify_send_output("default\n"),
        Some(NotificationAction::Open)
    );
    assert_eq!(parse_notify_send_output(""), None);
    assert_eq!(parse_notify_send_output("closed"), None);
}

fn reaction_input<'a>(source: ReactionNotificationSource) -> ReactionNotifyInput<'a> {
    ReactionNotifyInput {
        chat_id: ChatId(7),
        chat_title: "Ada",
        source,
        sender_name: Some("Grace"),
        sender_is_contact: true,
        emoji: Some("\u{2764}"),
        show_preview: true,
        chat_muted: false,
        app_active: false,
        open_chat: None,
    }
}

#[test]
fn reaction_source_gates_the_notification() {
    assert!(decide_reaction_notify(&reaction_input(ReactionNotificationSource::None)).is_none());
    let all = decide_reaction_notify(&reaction_input(ReactionNotificationSource::All)).unwrap();
    assert_eq!(all.title, "Ada");
    assert_eq!(all.body, "Grace reacted \u{2764} to your message");
    let mut stranger = reaction_input(ReactionNotificationSource::Contacts);
    stranger.sender_is_contact = false;
    assert!(decide_reaction_notify(&stranger).is_none());
    stranger.sender_is_contact = true;
    assert!(decide_reaction_notify(&stranger).is_some());
}

#[test]
fn reaction_respects_mute_focus_and_preview() {
    let mut muted = reaction_input(ReactionNotificationSource::All);
    muted.chat_muted = true;
    assert!(decide_reaction_notify(&muted).is_none());
    let mut open = reaction_input(ReactionNotificationSource::All);
    open.app_active = true;
    open.open_chat = Some(ChatId(7));
    assert!(decide_reaction_notify(&open).is_none());
    let mut hidden = reaction_input(ReactionNotificationSource::All);
    hidden.show_preview = false;
    assert_eq!(
        decide_reaction_notify(&hidden).unwrap().body,
        "New reaction to your message"
    );
    let mut custom = reaction_input(ReactionNotificationSource::All);
    custom.emoji = None;
    assert_eq!(
        decide_reaction_notify(&custom).unwrap().body,
        "Grace reacted to your message"
    );
}

fn service_message(action: crate::telegram::envelope::ServiceAction) -> ParsedMessage {
    let mut message = test_message(7, 100, false, "");
    message.content = MessageContent::Action(Box::new(action));
    message
}

#[test]
fn pinned_message_follows_the_events_setting() {
    let message = service_message(crate::telegram::envelope::ServiceAction::Pin { message_id: 5 });
    assert!(decide_notify(&input(&message, false, None)).is_some());
    let mut ctx = input(&message, false, None);
    ctx.pinned_allowed = false;
    assert!(decide_notify(&ctx).is_none());
}

#[test]
fn contact_joined_follows_the_events_setting() {
    let message = service_message(crate::telegram::envelope::ServiceAction::ContactRegistered);
    assert!(decide_notify(&input(&message, false, None)).is_some());
    let mut ctx = input(&message, false, None);
    ctx.contact_joined_allowed = false;
    assert!(decide_notify(&ctx).is_none());
}

#[test]
fn event_switches_do_not_gate_ordinary_messages() {
    let message = test_message(7, 100, false, "hello");
    let mut ctx = input(&message, false, None);
    ctx.pinned_allowed = false;
    ctx.contact_joined_allowed = false;
    assert!(decide_notify(&ctx).is_some());
    assert_eq!(notify_event(&message.content), None);
}
