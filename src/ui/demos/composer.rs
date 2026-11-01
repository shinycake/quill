//! Screenshot demos: composer.

use super::attachments;
use crate::ui::app::QuillApp;
use crate::ui::demo::seed_ready_send_media_session;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::ids::ChatId;

register_demos![
    DemoSpec::chat_list("ready-chats").setup(QuillApp::demo_ready_chats),
    DemoSpec::chat_list("ready-chats-composer").setup(QuillApp::demo_ready_chats_composer),
    // The rich editor over formatted block markup, with its live preview.
    // `QUILL_DEMO_RICH_BYTES` sets how much text it holds (default a short draft).
    DemoSpec::chat_list("ready-rich-editor").setup(QuillApp::demo_ready_rich_editor),
    // Composer attachment chip + outgoing photo/document (injected, no live Telegram).
    DemoSpec::ready(
        "ready-send-media",
        seed_ready_send_media_session,
        "screenshot demo — outgoing photo/document send (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_send_media)
    .attachments(|| attachments(&[("demo-notes.txt", AttachmentKind::Document)])),
    // A photo and a video in the tray, their send waiting on the background
    // read of the files (`send_prep.rs`): the tray says it is preparing.
    DemoSpec::chat_list("ready-send-preparing")
        .setup(QuillApp::demo_ready_send_preparing)
        .attachments(|| {
            attachments(&[
                ("demo-thumb.png", AttachmentKind::Photo),
                ("demo-clip.mp4", AttachmentKind::Video),
            ])
        }),
    // Ready chat draft with typos underlined (red wavy).
    DemoSpec::chat_list("ready-spellcheck").setup(|app, window, cx| app.demo_spellcheck(
        SpellcheckDemo::Spellcheck,
        window,
        cx
    )),
    // Multi-line draft: typos underlined; link, mention, hashtag, command and code skipped.
    DemoSpec::chat_list("ready-spellcheck-panel").setup(|app, window, cx| app.demo_spellcheck(
        SpellcheckDemo::SpellcheckPanel,
        window,
        cx
    )),
    // The `/` menu in a group with two bots: userpics, `/command@bot` rows
    // and the bot that wrote last first. `QUILL_DEMO_COMMAND_TEXT` sets the
    // typed text (default `/`).
    DemoSpec::chats(
        "ready-group-commands",
        "screenshot demo — the / menu in a group with bots (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_group_commands),
    // The reply and edit bars, the "Cancel editing?" box and the field's
    // placeholders. `QUILL_DEMO_BARS` is `reply`, `edit`, `confirm`,
    // `stars`, `broadcast`, `silent`, `anonymous`, `inline`, `topic`,
    // `topic-reply`, `keyboard` or `offthread`.
    DemoSpec::chats(
        "ready-composer-bars",
        "screenshot demo — reply and edit bars, placeholders (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_composer_bars),
];

/// Block markup of at least `bytes` bytes, formatted on every line.
pub(in crate::ui) fn rich_editor_markup(bytes: usize) -> String {
    let mut markup = String::from("# Release notes\n\n");
    let mut item = 0;
    loop {
        item += 1;
        markup.push_str(&format!(
            "## Section {item}\nSome **bold** and _italic_ text with a [link](https://x.y/{item}).\n- first ~~point~~\n- second `point`\n\n"
        ));
        if markup.len() >= bytes {
            return markup;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpellcheckDemo {
    Spellcheck,
    SpellcheckPanel,
}

impl QuillApp {
    fn demo_ready_chats(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // `QUILL_DEMO_HISTORY_ANIM=…,panel|menu|select`: something over the
        // animated history, to check what the animation layer draws under it.
        use crate::ui::demo::demo_history_extra;
        let sticker = quill::ids::MessageId(crate::ui::demo::HISTORY_ANIM_STICKER);
        if demo_history_extra("panel") {
            self.pickers.media_panel.open = true;
            self.pickers.media_panel.tab = crate::ui::media_panel::PanelTab::Emoji;
        }
        if demo_history_extra("menu") {
            self.message_ui.menu = Some(crate::ui::menu_states::MessageMenuState {
                chat_id: ChatId(11),
                message_id: sticker,
                position: point(px(340.), px(380.)),
            });
        }
        if demo_history_extra("select") {
            self.toggle_forward_select(ChatId(11), sticker, false, cx);
        }
    }

    fn demo_ready_chats_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("hello from composer", window, cx);
        });
    }

    fn demo_ready_rich_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let bytes = std::env::var("QUILL_DEMO_RICH_BYTES")
            .ok()
            .and_then(|bytes| bytes.parse().ok())
            .unwrap_or(0);
        let markup = rich_editor_markup(bytes);
        self.composer_ui.rich_editor_open = true;
        self.set_composer_markup(&markup, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
    }

    fn demo_group_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_group_commands(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        let text = std::env::var("QUILL_DEMO_COMMAND_TEXT").unwrap_or_else(|_| "/".into());
        self.composer.update(cx, |input, cx| {
            input.set_value(&text, window, cx);
            input.focus(window, cx);
            let end = input.value().len();
            input.set_selected_range(end..end, cx);
        });
        self.sync_command_menu(cx);
    }

    fn demo_composer_bars(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        let mode = std::env::var("QUILL_DEMO_BARS").unwrap_or_else(|_| "reply".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_composer_bars(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let chat = match mode.as_str() {
                "stars" => ChatId(51),
                "broadcast" | "silent" => ChatId(-1002000000001),
                "anonymous" => ChatId(-1002000000002),
                "inline" => ChatId(51),
                "topic" | "topic-reply" => FORUM_CHAT,
                "keyboard" => KEYBOARD_BOT_CHAT,
                "offthread" => TOPIC_BOT_CHAT,
                _ => ChatId(-2002),
            };
            session.open_chat(chat);
        }
        match mode.as_str() {
            "reply" => {
                self.composer_ui
                    .set_reply(Some(quill::composer::ComposerReplyTo::new(
                        ChatId(-2002),
                        quill::ids::MessageId(10),
                    )));
            }
            "edit" | "confirm" => {
                let edit = self
                    .session()
                    .and_then(|session| session.last_editable_message(ChatId(-2002)));
                if let Some(edit) = edit {
                    self.begin_edit(edit, window, cx);
                }
                if mode == "confirm" {
                    self.composer.update(cx, |input, cx| {
                        input.set_value("Saturday works, let's meet at 8", window, cx);
                    });
                    self.request_cancel_edit(window, cx);
                }
            }
            "silent" => self.toggle_composer_silent(),
            "topic-reply" => {
                self.composer_ui
                    .set_reply(Some(quill::composer::ComposerReplyTo::new(
                        FORUM_CHAT,
                        quill::ids::MessageId(20),
                    )));
            }
            "inline" => {
                self.composer.update(cx, |input, cx| {
                    input.set_value("@gifbot ", window, cx);
                    input.focus(window, cx);
                    let end = input.value().len();
                    input.set_selected_range(end..end, cx);
                });
            }
            _ => {}
        }
        cx.notify();
    }

    fn demo_ready_send_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("sending a photo too", window, cx);
        });
    }

    fn demo_ready_send_preparing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("two files for you", window, cx);
        });
        if let Some(slot) = self.send_slot() {
            self.composer_ui.send_box.prep.begin(slot);
        }
    }

    fn demo_spellcheck(
        &mut self,
        demo: SpellcheckDemo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat_prefs.spellcheck_enabled = true;
        // Ignore persisted app words so this fixture always shows typos.
        self.spell.checker = Self::new_spellchecker(false).0;
        // Off macOS the fixture must not depend on the host's dictionaries.
        #[cfg(not(target_os = "macos"))]
        {
            self.spell.checker = std::sync::Arc::new(quill::spellcheck::SpellChecker::wordlist());
        }
        // `-panel`: a multi-line draft proving the skip rules — the
        // link, mention, hashtag, command and code stay unmarked.
        let draft = if matches!(demo, SpellcheckDemo::SpellcheckPanel) {
            "Teh quick brown fox has a speling error.\n\
             See https://exampel.com/tehh, ask @tehuser about #tehtag,\n\
             run /strat or `cargo biuld` \u{1F60A} and recieve it tomorow."
        } else {
            "Teh quick brown fox has a speling error"
        };
        self.composer.update(cx, |input, cx| {
            input.set_value(draft, window, cx);
        });
        self.spellcheck_now(cx);
    }
}

/// A basic group (id -2001) with a member and two bots whose commands come
/// with the group's full info; the second bot wrote last.
fn apply_group_commands(
    session: &mut quill::state::Session,
    sink: &std::sync::Arc<quill::diagnostics::MemorySink>,
    seq: &std::sync::atomic::AtomicU64,
) {
    use quill::diagnostics::DiagnosticSink;
    use quill::telegram::client::copy_and_parse;
    let dyn_sink: std::sync::Arc<dyn DiagnosticSink> = sink.clone();
    session.open_chat(ChatId(-2001));
    let bot = |id: i64, name: &str, username: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{name}","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}"}},"type":{{"@type":"userTypeBot","is_inline":false}},"status":{{"@type":"userStatusOffline","was_online":1}}}}}}"#
        )
    };
    let message = |id: i64, sender: i64, text: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":-2001,"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":false,"date":1700000{id},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
        )
    };
    let command = |name: &str, description: &str, ephemeral: bool| {
        format!(
            r#"{{"@type":"botCommand","command":"{name}","description":"{description}","is_ephemeral":{ephemeral}}}"#
        )
    };
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#.to_string(),
        bot(41, "Poll Bot", "pollbot"),
        bot(42, "Weather", "weatherbot"),
        r#"{"@type":"updateNewChat","chat":{"id":-2001,"title":"Weekend plans","type":{"@type":"chatTypeBasicGroup","basic_group_id":2001},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":-2001,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"50","is_pinned":false}}"#.to_string(),
        message(10, 31, "Where should we go on Saturday?"),
        message(11, 41, "Poll created: hike, beach or museum"),
        message(12, 42, "Saturday: sunny, 24 degrees"),
        format!(
            r#"{{"@type":"updateBasicGroupFullInfo","basic_group_id":2001,"basic_group_full_info":{{"description":"","members":[],"bot_commands":[{{"@type":"botCommands","bot_user_id":41,"commands":[{},{},{}]}},{{"@type":"botCommands","bot_user_id":42,"commands":[{},{}]}}]}}}}"#,
            command("start", "Start the bot", false),
            command("poll", "Create a poll", false),
            command("results", "Show the results", true),
            command("start", "Show the weather", false),
            command("forecast", "Five day forecast", false),
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// The forum supergroup, the bot with a keyboard that has its own
/// placeholder and the bot that makes its own topics, in the composer demo.
pub(crate) const FORUM_CHAT: ChatId = ChatId(-1002000000003);
pub(crate) const KEYBOARD_BOT_CHAT: ChatId = ChatId(71);
pub(crate) const TOPIC_BOT_CHAT: ChatId = ChatId(72);
/// The forum's "Releases" topic.
pub(crate) const RELEASES_TOPIC: i32 = 5;

/// Three chats for the composer bars demo: a group with an incoming and an
/// own message, a private chat whose user charges 25 Stars per message and
/// a channel the account owns.
fn apply_composer_bars(
    session: &mut quill::state::Session,
    sink: &std::sync::Arc<quill::diagnostics::MemorySink>,
    seq: &std::sync::atomic::AtomicU64,
) {
    use quill::diagnostics::DiagnosticSink;
    use quill::telegram::client::copy_and_parse;
    let dyn_sink: std::sync::Arc<dyn DiagnosticSink> = sink.clone();
    let message = |id: i64, chat: i64, sender: i64, outgoing: bool, text: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":{outgoing},"date":1700000{id},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
        )
    };
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#.to_string(),
        r#"{"@type":"updateUser","user":{"id":51,"first_name":"Mira","last_name":"Stone","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOffline","was_online":1}}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":-2002,"title":"Weekend plans","type":{"@type":"chatTypeBasicGroup","basic_group_id":2002},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":-2002,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"60","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":51,"title":"Mira Stone","type":{"@type":"chatTypePrivate","user_id":51},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":51,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"55","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":-1002000000001,"title":"Quill news","type":{"@type":"chatTypeSupergroup","supergroup_id":2000000001,"is_channel":true},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":-1002000000001,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"52","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateUser","user":{"id":61,"first_name":"GIF","usernames":{"@type":"usernames","active_usernames":["gifbot"],"disabled_usernames":[],"editable_username":"gifbot"},"type":{"@type":"userTypeBot","is_inline":true,"inline_query_placeholder":"Search GIFs"},"status":{"@type":"userStatusOffline","was_online":1}}}"#.to_string(),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":2000000002,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","is_anonymous":true}}}}"#.to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":-1002000000002,"title":"Quill makers","type":{"@type":"chatTypeSupergroup","supergroup_id":2000000002,"is_channel":false},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":-1002000000002,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"51","is_pinned":false}}"#.to_string(),
        message(10, -2002, 31, false, "Where should we go on Saturday?"),
        message(11, -2002, 1, true, "Saturday works, let's meet at 7"),
        message(12, 51, 51, false, "Hi! Quick question about the schedule"),
        message(13, -1002000000001, 1, true, "Release notes are up"),
        message(14, -1002000000002, 31, false, "Who is on call this week?"),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    apply_placeholder_chats(session, &dyn_sink, seq);
    if let Some(chat) = session.chats.get_mut(&-1002000000001) {
        chat.set_member_status(
            quill::telegram::envelope::ChannelMemberStatus::Creator,
            None,
        );
    }
    session.bots.inline_bot_resolve = Some(quill::state::InlineBotResolve::Resolved {
        username: "gifbot".into(),
        user_id: 61,
        is_inline: Some(true),
    });
    session.payments.paid_messages.record_answer(
        51,
        quill::paid_messages::CanSend::HasPaidMessages { stars: 25 },
        1,
    );
}

/// A forum supergroup with General and Releases topics, a bot whose keyboard
/// carries its own placeholder, and a bot that makes its own topics.
fn apply_placeholder_chats(
    session: &mut quill::state::Session,
    sink: &std::sync::Arc<dyn quill::diagnostics::DiagnosticSink>,
    seq: &std::sync::atomic::AtomicU64,
) {
    use quill::telegram::client::copy_and_parse;
    let bot = |id: i64, name: &str, topics: bool| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{name}","type":{{"@type":"userTypeBot","has_topics":{topics},"allows_users_to_create_topics":false,"is_inline":false}},"status":{{"@type":"userStatusOffline","was_online":1}}}}}}"#
        )
    };
    let chat = |id: i64, title: &str, order: u32| {
        [
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
            ),
        ]
    };
    let forum = FORUM_CHAT.0;
    let mut jsons = vec![bot(71, "Sizer", false), bot(72, "Storyteller", true)];
    jsons.extend(chat(71, "Sizer", 48));
    jsons.extend(chat(72, "Storyteller", 47));
    jsons.extend([
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{forum},"title":"Quill releases","type":{{"@type":"chatTypeSupergroup","supergroup_id":2000000003,"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{forum},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"49","is_pinned":false}}}}"#
        ),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":2000000003,"is_forum":true}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":20,"chat_id":{forum},"sender_id":{{"@type":"messageSenderUser","user_id":31}},"topic_id":{{"@type":"messageTopicForum","forum_topic_id":{RELEASES_TOPIC}}},"is_outgoing":false,"date":1700000020,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Release candidate is out","entities":[]}}}}}}}}"#
        ),
        r#"{"@type":"updateNewMessage","message":{"id":21,"chat_id":71,"sender_id":{"@type":"messageSenderUser","user_id":71},"is_outgoing":false,"date":1700000021,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[[{"@type":"keyboardButton","text":"Small","type":{"@type":"keyboardButtonTypeText"}},{"@type":"keyboardButton","text":"Medium","type":{"@type":"keyboardButtonTypeText"}},{"@type":"keyboardButton","text":"Large","type":{"@type":"keyboardButtonTypeText"}}]],"is_persistent":false,"resize_keyboard":true,"one_time":false,"is_personal":false,"force_reply":false,"input_field_placeholder":"Choose a size"},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Which size do you need?","entities":[]}}}}"#.to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":22,"chat_id":72,"sender_id":{"@type":"messageSenderUser","user_id":72},"is_outgoing":false,"date":1700000022,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick a story from the topics above.","entities":[]}}}}"#.to_string(),
    ]);
    let topic = |id: i32, name: &str, general: bool| {
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{forum},"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":16766590,"custom_emoji_id":"0"}},"is_general":{general},"is_closed":false,"is_hidden":false}},"order":"{id}","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}}"#
        )
    };
    let apply = |session: &mut quill::state::Session, json: &str| {
        if let Some(owned) = copy_and_parse(json, seq, sink) {
            session.apply(owned);
        }
    };
    for json in &jsons {
        apply(session, json);
    }
    let extra = session.request(
        quill::state::RequestPurpose::GetForumTopics,
        Some(FORUM_CHAT),
    );
    apply(
        session,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0,
            topic(1, "General", true),
            topic(RELEASES_TOPIC, "Releases", false),
        ),
    );
}
