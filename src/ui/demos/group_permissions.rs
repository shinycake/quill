//! Screenshot demos: the group permissions box, its Exceptions list and
//! the "Add exception" picker, over an offline supergroup (id 62, the
//! viewer owns it) and a forum (id 63, the viewer an admin without the
//! info and topics rights, so those rows are locked).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::dialogs::MemberTab;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::add_special::SpecialRole;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::{GroupsPurpose, MemberListFilter, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(in crate::ui) const GROUP: i64 = 62;
pub(in crate::ui) const FORUM: i64 = 63;
const ME: i64 = 777;

register_demos![
    DemoSpec::chats("group-permissions", "screenshot demo — group permissions")
        .setup(QuillApp::demo_group_permissions),
    DemoSpec::chats(
        "group-permissions-forum",
        "screenshot demo — forum permissions"
    )
    .setup(QuillApp::demo_group_permissions_forum),
    DemoSpec::chats("group-exceptions", "screenshot demo — group exceptions")
        .setup(QuillApp::demo_group_exceptions),
    DemoSpec::chats("group-add-exception", "screenshot demo — add exception")
        .setup(QuillApp::demo_group_add_exception),
];

fn user(id: i64, first: &str, last: &str) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
    )
}

fn member(id: i64, inviter: i64, date: i64, status: &str) -> String {
    format!(
        r#"{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":{id}}},"tag":"","inviter_user_id":{inviter},"joined_chat_date":{date},"status":{status}}}"#
    )
}

fn rights(info: bool, topics: bool, can_be_edited: bool) -> String {
    format!(
        r#"{{"@type":"chatMemberStatusAdministrator","can_be_edited":{can_be_edited},"rights":{{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":{info},"can_post_messages":true,"can_edit_messages":true,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":true,"can_pin_messages":true,"can_manage_topics":{topics},"can_promote_members":false,"can_manage_video_chats":false,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":false,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}}}"#
    )
}

const RESTRICTED: &str = r#"{"@type":"chatMemberStatusRestricted","is_member":true,"restricted_until_date":0,"permissions":{"@type":"chatPermissions","can_send_basic_messages":true,"can_send_photos":false,"can_send_videos":false,"can_send_other_messages":false,"can_add_link_previews":false}}"#;

/// The chat's default permissions: photos, polls and pinning off.
const DEFAULTS: &str = r#"{"@type":"chatPermissions","can_send_basic_messages":true,"can_send_audios":true,"can_send_documents":true,"can_send_photos":false,"can_send_videos":true,"can_send_video_notes":true,"can_send_voice_notes":true,"can_send_polls":false,"can_send_other_messages":true,"can_add_link_previews":true,"can_react_to_messages":true,"can_change_info":false,"can_invite_users":true,"can_pin_messages":false,"can_create_topics":true,"can_edit_tag":true}"#;

/// One group of the demo: the viewer owns `GROUP`; in `FORUM` they are an
/// admin without the info and topics rights.
fn apply_group(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, chat_id: i64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let forum = chat_id == FORUM;
    let chat = ChatId(chat_id);
    let me_extra = session.request(RequestPurpose::GetMe, None);
    let member_extra = session.request(RequestPurpose::GetChatMember, Some(chat));
    let page = |session: &mut Session, filter| {
        session.request(
            RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers { filter }),
            Some(chat),
        )
    };
    let recent = page(session, MemberListFilter::Recent);
    let restricted = page(session, MemberListFilter::Restricted);
    let banned = page(session, MemberListFilter::Banned);
    let my_status = if forum {
        rights(false, false, false)
    } else {
        r#"{"@type":"chatMemberStatusCreator","custom_title":"","is_anonymous":false,"is_member":true}"#
            .to_string()
    };
    let title = if forum {
        "Product Forum"
    } else {
        "Design Club"
    };
    let (members, size_max) = if forum { (840, 0) } else { (199_400, 200_000) };
    let date = 1_760_000_000;
    let jsons = [
        format!(
            r#"{{"@type":"updateOption","name":"supergroup_size_max","value":{{"@type":"optionValueInteger","value":"{size_max}"}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":false}},"permissions":{DEFAULTS},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{}","is_pinned":false}}}}"#,
            90 + chat_id
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{chat_id},"is_forum":{forum},"member_count":{members},"status":{my_status}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{chat_id},"supergroup_full_info":{{"@type":"supergroupFullInfo","description":"","member_count":{members},"administrator_count":2,"restricted_count":2,"banned_count":1,"slow_mode_delay":30,"unrestrict_boost_count":2,"linked_chat_id":0}}}}"#
        ),
        user(ME, tr(keys::Q_PEOPLE_DEMO), tr(keys::Q_PEOPLE_VIEWER)),
        format!(
            r#"{{"@type":"user","@extra":"{}","id":{ME},"first_name":"Demo","last_name":"Viewer","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}"#,
            me_extra.0,
        ),
        format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":{ME}}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{my_status}}}"#,
            member_extra.0,
        ),
        user(1, tr(keys::Q_PEOPLE_IDAN), tr(keys::Q_PEOPLE_FOUNDER)),
        user(2, tr(keys::Q_PEOPLE_DANA), tr(keys::Q_PEOPLE_LEVI)),
        user(5, tr(keys::Q_PEOPLE_OMAR), tr(keys::Q_PEOPLE_HADDAD)),
        user(6, tr(keys::Q_PEOPLE_MAYA), tr(keys::Q_PEOPLE_SHARON)),
        format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":5,"members":[{},{},{},{},{}]}}"#,
            recent.0,
            member(ME, 0, date, &my_status),
            member(1, ME, date, &rights(true, true, true)),
            member(2, 1, date, RESTRICTED),
            member(5, 0, date, r#"{"@type":"chatMemberStatusMember"}"#),
            member(6, ME, date, RESTRICTED),
        ),
        format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":2,"members":[{},{}]}}"#,
            restricted.0,
            member(2, 1, date, RESTRICTED),
            member(6, ME, date + 86_400, RESTRICTED),
        ),
        format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":1,"members":[{}]}}"#,
            banned.0,
            member(
                5,
                1,
                date,
                r#"{"@type":"chatMemberStatusBanned","banned_until_date":0}"#
            ),
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(chat);
}

impl QuillApp {
    fn demo_permissions_fixture(&mut self, chat_id: i64) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_group(session, &self.demo_ui.sink, &self.demo_ui.seq, chat_id);
        }
    }

    pub(in crate::ui) fn demo_group_permissions(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.demo_permissions_fixture(GROUP);
        self.open_permissions_dialog(ChatId(GROUP), cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — group permissions"));
        cx.notify();
    }

    fn demo_group_permissions_forum(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.demo_permissions_fixture(FORUM);
        self.open_permissions_dialog(ChatId(FORUM), cx);
        cx.notify();
    }

    pub(in crate::ui) fn demo_group_exceptions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.demo_group_permissions(window, cx);
        self.open_member_dialog(ChatId(GROUP), window, cx);
        self.member_dialog_tab(MemberTab::Restricted, cx);
    }

    fn demo_group_add_exception(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_group_exceptions(window, cx);
        self.open_add_special(ChatId(GROUP), SpecialRole::Restricted, window, cx);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_permissions_tests.rs"]
mod tests;
