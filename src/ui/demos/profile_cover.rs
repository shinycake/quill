//! Screenshot demos: the profile cover facts. A bot with its monthly
//! users, a contact with unread stories and a Stars rating, a contact
//! whose last seen the viewer's own privacy hides, and the viewer's own
//! profile as Saved Messages.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{InfoPanelTarget, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-info-cover-bot",
        "screenshot demo — bot info, monthly users"
    )
    .setup(QuillApp::demo_cover_bot),
    DemoSpec::chats(
        "ready-info-cover-stories",
        "screenshot demo — contact info, story ring and rating"
    )
    .setup(QuillApp::demo_cover_stories),
    DemoSpec::chats(
        "ready-info-cover-last-seen",
        "screenshot demo — contact info, last seen hidden by you"
    )
    .setup(QuillApp::demo_cover_last_seen),
    DemoSpec::chats(
        "ready-info-cover-saved",
        "screenshot demo — your own info, Saved Messages"
    )
    .setup(QuillApp::demo_cover_saved),
];

pub(in crate::ui) const COVER_BOT: i64 = 9601;
pub(in crate::ui) const COVER_STORIES: i64 = 9602;
pub(in crate::ui) const COVER_HIDDEN: i64 = 9603;
pub(in crate::ui) const COVER_ME: i64 = 9600;

fn apply_all(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn private_chat(user_id: i64, title: &str, order: i64) -> [String; 2] {
    [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{user_id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{user_id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{user_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        ),
    ]
}

fn full_info(session: &mut Session, user_id: i64, body: &str) -> String {
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, user_id);
    format!(
        r#"{{"@type":"userFullInfo","@extra":"{}",{body}}}"#,
        extra.0
    )
}

/// Every cover fact this demo set shows, in the session `user_id` opens on.
pub(in crate::ui) fn apply_cover_fixture(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    session.my_user_id = Some(COVER_ME);
    let mut jsons = vec![
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{COVER_ME},"first_name":"Demo","last_name":"Viewer","phone_number":"15550109600","is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{COVER_BOT},"first_name":"Forecast","usernames":{{"@type":"usernames","active_usernames":["forecastbot"],"disabled_usernames":[],"editable_username":"forecastbot"}},"type":{{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":1284567}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{COVER_STORIES},"first_name":"Maya","last_name":"Levin","phone_number":"15550109602","is_contact":true,"status":{{"@type":"userStatusOnline","expires":4102444800}},"usernames":{{"@type":"usernames","active_usernames":["mayalevin"],"disabled_usernames":[],"editable_username":"mayalevin"}},"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{COVER_HIDDEN},"first_name":"Theo","last_name":"Brandt","phone_number":"15550109603","is_contact":true,"status":{{"@type":"userStatusRecently","by_my_privacy_settings":true}},"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{COVER_STORIES},"list":{{"@type":"storyListMain"}},"order":"{COVER_STORIES}","can_be_archived":false,"max_read_story_id":1,"stories":[{{"@type":"storyInfo","story_id":1,"date":1790000000,"is_for_close_friends":false,"is_live":false}},{{"@type":"storyInfo","story_id":2,"date":1790000100,"is_for_close_friends":false,"is_live":false}},{{"@type":"storyInfo","story_id":3,"date":1790000200,"is_for_close_friends":false,"is_live":false}}]}}}}"#
        ),
        r#"{"@type":"updateSavedMessagesTopicCount","topic_count":4}"#.to_string(),
    ];
    for (user_id, title, order) in [
        (COVER_BOT, "Forecast", 9601),
        (COVER_STORIES, "Maya Levin", 9602),
        (COVER_HIDDEN, "Theo Brandt", 9603),
        (COVER_ME, "Demo Viewer", 9600),
    ] {
        jsons.extend(private_chat(user_id, title, order));
    }
    jsons.push(full_info(
        session,
        COVER_BOT,
        r#""bot_info":{"@type":"botInfo","short_description":"Weather for any city.","description":"Ask for the forecast anywhere.","commands":[]}"#,
    ));
    jsons.push(full_info(
        session,
        COVER_STORIES,
        r#""bio":{"@type":"formattedText","text":"Photos from the road.","entities":[]},"rating":{"@type":"userRating","level":7,"is_maximum_level_reached":false,"rating":1840,"current_level_rating":1500,"next_level_rating":2500}"#,
    ));
    jsons.push(full_info(
        session,
        COVER_HIDDEN,
        r#""bio":{"@type":"formattedText","text":"Mostly offline.","entities":[]}"#,
    ));
    apply_all(session, sink, seq, &jsons);
}

impl QuillApp {
    fn demo_cover(
        &mut self,
        user_id: i64,
        note: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_cover_fixture(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
        self.connection.set_note(StatusNote::silent(note));
        cx.notify();
    }

    fn demo_cover_bot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_cover(
            COVER_BOT,
            "screenshot demo — bot info, monthly users",
            window,
            cx,
        );
    }

    fn demo_cover_stories(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_cover(
            COVER_STORIES,
            "screenshot demo — contact info, story ring and rating",
            window,
            cx,
        );
    }

    fn demo_cover_last_seen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_cover(
            COVER_HIDDEN,
            "screenshot demo — contact info, last seen hidden by you",
            window,
            cx,
        );
    }

    fn demo_cover_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_cover(
            COVER_ME,
            "screenshot demo — your own info, Saved Messages",
            window,
            cx,
        );
    }
}
