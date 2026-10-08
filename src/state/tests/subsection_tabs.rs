//! State reducer tests: subsection tabs (bots with topics, forums with tabs).
use super::common::*;
use super::*;
use crate::subsection_tabs::SubsectionTabsMode;

fn bot_user(id: i64, has_topics: bool, allows: bool) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"Bot","type":{{"@type":"userTypeBot","has_topics":{has_topics},"allows_users_to_create_topics":{allows},"is_inline":false}}}}}}"#
    )
}

fn private_chat(id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"Bot","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
    )
}

fn topics_answer(extra: u64, chat: i64, ids: &[i32]) -> String {
    let topics: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{chat},"forum_topic_id":{id},"name":"T{id}","icon":{{"@type":"forumTopicIcon","color":16766590,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":{{"id":{last},"chat_id":{chat},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}},"order":"{id}","is_pinned":false,"unread_count":1,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}}"#,
                last = 100 + id
            )
        })
        .collect();
    format!(
        r#"{{"@type":"forumTopics","@extra":"{extra}","total_count":{},"topics":[{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        ids.len(),
        topics.join(",")
    )
}

#[test]
fn parses_bot_topic_flags_and_forum_tabs() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &bot_user(41, true, false));
    let user = session.users.get(&41).unwrap();
    assert!(user.has_topics);
    assert!(!user.allows_users_to_create_topics);
    // Non-bots never carry the flags, even if the JSON says so.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":42,"first_name":"Ann","type":{"@type":"userTypeRegular","has_topics":true}}}"#,
    );
    assert!(!session.users.get(&42).unwrap().has_topics);

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true,"has_forum_tabs":true}}"#,
    );
    assert!(session.forum_tabs_supergroups.contains(&16));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true,"has_forum_tabs":false}}"#,
    );
    assert!(!session.forum_tabs_supergroups.contains(&16));
}

#[test]
fn used_for_gating_matches_tdesktop() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // A plain bot: no tabs.
    apply_json(&mut session, &seq, &sink, &bot_user(40, false, false));
    apply_json(&mut session, &seq, &sink, &private_chat(40));
    assert!(!session.chat_has_topics(ChatId(40)));
    assert!(!session.subsection_tabs_used_for(ChatId(40)));

    // Users create topics: tabs right away (even with none yet).
    apply_json(&mut session, &seq, &sink, &bot_user(41, true, true));
    apply_json(&mut session, &seq, &sink, &private_chat(41));
    assert!(session.chat_has_topics(ChatId(41)));
    assert!(session.subsection_tabs_used_for(ChatId(41)));

    // The bot creates topics itself (`IsBotCreatesTopics`): only once a
    // topic exists.
    apply_json(&mut session, &seq, &sink, &bot_user(43, true, false));
    apply_json(&mut session, &seq, &sink, &private_chat(43));
    assert!(session.chat_has_topics(ChatId(43)));
    assert!(!session.subsection_tabs_used_for(ChatId(43)));
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(43)));
    apply_json(&mut session, &seq, &sink, &topics_answer(extra.0, 43, &[]));
    assert!(!session.subsection_tabs_used_for(ChatId(43)));
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(43)));
    apply_json(&mut session, &seq, &sink, &topics_answer(extra.0, 43, &[2]));
    assert!(session.subsection_tabs_used_for(ChatId(43)));

    // Forum supergroups: tabs only with `has_forum_tabs`.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"F","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true,"has_forum_tabs":false}}"#,
    );
    assert!(session.chat_has_topics(ChatId(16)));
    assert!(!session.subsection_tabs_used_for(ChatId(16)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true,"has_forum_tabs":true}}"#,
    );
    assert!(session.subsection_tabs_used_for(ChatId(16)));
}

#[test]
fn mode_cycles_top_bottom_left_per_chat() {
    let (mut session, _sink) = session();
    let chat = ChatId(41);
    assert_eq!(session.subsection_tabs_mode(chat), SubsectionTabsMode::Top);
    assert_eq!(
        session.cycle_subsection_tabs_mode(chat),
        SubsectionTabsMode::Bottom
    );
    assert_eq!(
        session.cycle_subsection_tabs_mode(chat),
        SubsectionTabsMode::Left
    );
    assert_eq!(
        session.subsection_tabs_mode(ChatId(7)),
        SubsectionTabsMode::Top
    );
    assert_eq!(
        session.cycle_subsection_tabs_mode(chat),
        SubsectionTabsMode::Top
    );
    // The default is not stored.
    assert!(session.media_prefs.subsection_tabs_modes.is_empty());
}

#[test]
fn topic_updates_keep_tabs_current() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &bot_user(41, true, true));
    apply_json(&mut session, &seq, &sink, &private_chat(41));
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(41)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &topics_answer(extra.0, 41, &[2, 3]),
    );

    // Rename + recolor.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateForumTopicInfo","info":{"@type":"forumTopicInfo","chat_id":41,"forum_topic_id":2,"name":"Trips","icon":{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"},"is_general":false,"is_closed":false,"is_hidden":false}}"#,
    );
    // A brand-new topic goes first.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateForumTopicInfo","info":{"@type":"forumTopicInfo","chat_id":41,"forum_topic_id":9,"name":"New","icon":{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"},"is_general":false,"is_closed":false,"is_hidden":false}}"#,
    );
    let topics = session.ordered_forum_topics(ChatId(41));
    assert_eq!(topics[0].forum_topic_id, 9);
    let trips = topics.iter().find(|t| t.forum_topic_id == 2).unwrap();
    assert_eq!(trips.name, "Trips");
    assert_eq!(trips.icon_color, 7322096);

    // An incoming topic message counts as unread and becomes the last one.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":500,"chat_id":41,"is_outgoing":false,"topic_id":{"@type":"messageTopicForum","forum_topic_id":3},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"ping","entities":[]}}}}"#,
    );
    let t3 = |session: &Session| {
        session
            .ordered_forum_topics(ChatId(41))
            .into_iter()
            .find(|t| t.forum_topic_id == 3)
            .unwrap()
    };
    assert_eq!(t3(&session).unread_count, 2);
    assert_eq!(t3(&session).last_message_id, 500);
    assert_eq!(t3(&session).last_message_preview, "ping");

    // Pin + mute + read up to the last message.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateForumTopic","chat_id":41,"forum_topic_id":3,"is_pinned":true,"last_read_inbox_message_id":500,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647},"draft_message":null}"#,
    );
    let topic = t3(&session);
    assert!(topic.is_pinned);
    assert!(topic.notification_settings.is_muted());
    assert_eq!(topic.unread_count, 0);

    // Chat-row topic line.
    assert_eq!(
        session.chat_row_topic_names(ChatId(41)).as_deref(),
        Some("T3  New  Trips")
    );
}
