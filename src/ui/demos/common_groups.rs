//! Screenshot demo: a profile with many groups in common (search field and
//! Load more row).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{
    GROUPS_IN_COMMON_PAGE, InfoPanelTarget, ProfileChatsFetch, ProfileChatsKind, RequestPurpose,
    Session,
};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    // A contact who shares 45 groups with you: the first page of 40 is
    // loaded, so the list has a search field and a Load more row.
    DemoSpec::chats(
        "ready-info-common-groups",
        "screenshot demo — 45 groups in common"
    )
    .setup(QuillApp::demo_info_common_groups),
];

const USER: i64 = 22;
const TOTAL: i64 = 45;
const FIRST_GROUP: i64 = 300;

fn apply_common_groups(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut jsons = vec![format!(
        r#"{{"@type":"updateUser","user":{{"id":{USER},"first_name":"Ada","last_name":"Lovelace","type":{{"@type":"userTypeRegular"}}}}}}"#
    )];
    for n in 0..TOTAL {
        let id = FIRST_GROUP + n;
        jsons.push(format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"Group {n}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":false}},"unread_count":0}}}}"#
        ));
    }
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, USER);
    jsons.push(format!(
        r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"","entities":[]}},"group_in_common_count":{TOTAL}}}"#,
        extra.0
    ));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let first_page: Vec<i64> = (0..GROUPS_IN_COMMON_PAGE as i64)
        .map(|n| FIRST_GROUP + n)
        .collect();
    session.users_state.profile_chat_lists.insert(
        (ProfileChatsKind::GroupsInCommon, USER),
        ProfileChatsFetch::Loaded(first_page),
    );
}

impl QuillApp {
    fn demo_info_common_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_common_groups(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::User(USER), window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — 45 groups in common"));
        cx.notify();
    }
}
