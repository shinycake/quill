//! Sweep r2-20: answers land only in the stage that asked for them.
use super::common::*;
use super::*;

fn users_json(extra: RequestId, ids: &str) -> String {
    format!(
        r#"{{"@type":"users","@extra":"{}","total_count":1,"user_ids":[{ids}]}}"#,
        extra.0
    )
}

#[test]
fn an_older_close_friends_answer_does_not_overwrite_a_saved_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let asked = session.request(RequestPurpose::GetCloseFriends, None);
    session.stories.close_friends.begin_load(asked);
    // The user saved a new list meanwhile: it is the list now.
    session.stories.close_friends = CloseFriendsList::Ready(vec![5]);
    apply_json(&mut session, &seq, &sink, &users_json(asked, "9"));
    assert_eq!(session.stories.close_friends.ids(), Some(&vec![5]));
    // The answer of the load in flight lands.
    let asked = session.request(RequestPurpose::GetCloseFriends, None);
    session.stories.close_friends.begin_load(asked);
    apply_json(&mut session, &seq, &sink, &users_json(asked, "9"));
    assert_eq!(session.stories.close_friends.ids(), Some(&vec![9]));
}

#[test]
fn an_older_forum_topics_answer_after_a_created_topic_lands_nowhere() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let chat = ChatId(16);
    let old = session.request(RequestPurpose::GetForumTopics, Some(chat));
    session.threads.forum_topics.begin(16, old);
    // A topic was created: the list is dropped, the row asks again.
    session.threads.forum_topics.remove(&16);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":0,"topics":[],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            old.0
        ),
    );
    assert!(!session.threads.forum_topics.contains_key(&16));
    assert!(!session.threads.forum_topics.asked(16));
}

#[test]
fn a_refused_chatbot_search_leg_is_a_failure_not_an_empty_answer() {
    use crate::state::{BotLookup, BotLookupPart, LookupLeg};
    let mut lookup = BotLookup::default();
    lookup.sent(BotLookupPart::Exact, RequestId(1));
    assert!(!lookup.done());
    lookup.exact = LookupLeg::Failed;
    lookup.mine = LookupLeg::Found(vec![]);
    lookup.global = LookupLeg::Found(vec![]);
    assert!(lookup.done() && lookup.failed());
    assert_eq!(lookup.exact_user(), None);
}

#[test]
fn a_second_business_save_keeps_saving_until_it_answers() {
    use crate::business_settings::BusinessSettingKind::Greeting;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let purpose = RequestPurpose::Settings(SettingsPurpose::SaveBusinessSetting { kind: Greeting });
    let first = session.request(purpose, None);
    let second = session.request(purpose, None);
    session.settings.business.saving.push((first, Greeting));
    session.settings.business.saving.push((second, Greeting));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, first.0),
    );
    assert!(session.settings.business.is_saving(), "save 2 is still out");
    // Save 2's own answer ends the saving.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, second.0),
    );
    assert!(!session.settings.business.is_saving());
}

#[test]
fn a_report_waits_for_its_own_request_and_leaves_on_its_error_or_a_failed_send() {
    use crate::state::{SponsoredReportFlow, SponsoredReportTarget};
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let target = SponsoredReportTarget {
        chat_id: ChatId(3),
        message_id: 7,
    };
    // A send that failed before leaving: the target is dropped.
    session.messages.sponsored_report = SponsoredReportFlow::Choosing(target);
    session.dismiss_sponsored_report();
    assert!(matches!(
        session.messages.sponsored_report,
        SponsoredReportFlow::Idle
    ));
    // The flow waits on its own request: another request's error leaves it.
    let asked = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(ChatId(3)));
    let other = session.request(RequestPurpose::ReportChatSponsoredMessage, Some(ChatId(3)));
    session.messages.sponsored_report = SponsoredReportFlow::Asking {
        target,
        request: asked,
    };
    let error = |extra: RequestId| {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &error(other));
    assert!(matches!(
        session.messages.sponsored_report,
        SponsoredReportFlow::Asking { .. }
    ));
    apply_json(&mut session, &seq, &sink, &error(asked));
    assert!(matches!(
        session.messages.sponsored_report,
        SponsoredReportFlow::Idle
    ));
}

#[test]
fn an_older_sessions_refresh_neither_fails_nor_fills_the_newer_one_in_either_order() {
    let sessions_json = |extra: RequestId| {
        format!(
            r#"{{"@type":"sessions","@extra":"{}","sessions":[]}}"#,
            extra.0
        )
    };
    let error_json = |extra: RequestId| {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            extra.0
        )
    };
    for newest_first in [false, true] {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let old = session.request(RequestPurpose::GetActiveSessions, None);
        session.settings.sessions.begin();
        session.settings.sessions.sent(old);
        let new = session.request(RequestPurpose::GetActiveSessions, None);
        session.settings.sessions.begin();
        session.settings.sessions.sent(new);
        if newest_first {
            apply_json(&mut session, &seq, &sink, &sessions_json(new));
            apply_json(&mut session, &seq, &sink, &error_json(old));
        } else {
            apply_json(&mut session, &seq, &sink, &error_json(old));
            assert!(
                session.settings.sessions.is_loading(),
                "the older error is not the newer fetch's"
            );
            apply_json(&mut session, &seq, &sink, &sessions_json(new));
        }
        assert!(
            session.settings.sessions.data().is_some(),
            "newest_first={newest_first}"
        );
        assert!(session.settings.sessions.error().is_none());
    }
}

#[test]
fn a_scope_default_update_beats_the_fetch_still_out() {
    use crate::telegram::envelope::NotificationSettingsScope::GroupChats;
    let (mut session, _sink) = session();
    let ask = session.request_for_scope(RequestPurpose::GetScopeNotificationSettings, GroupChats);
    session
        .settings
        .scope_notification_settings
        .begin(GroupChats, ask);
    let update = ScopeNotificationSettings {
        mute_for: 77,
        ..Default::default()
    };
    session
        .settings
        .scope_notification_settings
        .insert(GroupChats, update);
    // The fetch's older answer finds nothing waiting for it.
    let older = ScopeNotificationSettings::default();
    assert!(
        !session
            .settings
            .scope_notification_settings
            .answered(GroupChats, Some(ask), older)
    );
    assert_eq!(
        session
            .settings
            .scope_notification_settings
            .get(&GroupChats)
            .map(|s| s.mute_for),
        Some(77)
    );
}
