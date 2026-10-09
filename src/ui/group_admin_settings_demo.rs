//! B7 screenshot-demo fixtures for the group and channel settings dialog
//! (`--screenshot-demo ready-group-admin-settings`, mode from
//! `QUILL_DEMO_GROUP_ADMIN`). Everything is injected through the real
//! reducers; no live Telegram. English-only text.

use super::app::QuillApp;
use super::dialogs::GroupSettingsView;
use super::screenshot_demo::ScreenshotDemo;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{InfoPanelTarget, ProfileChatsKind, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const GROUP: i64 = 801;
const CHANNEL: i64 = 802;
const BASIC: i64 = 803;
const CANDIDATE_A: i64 = 804;
const CANDIDATE_B: i64 = 805;
const LINKED_GROUP: i64 = 806;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn supergroup_chat(id: i64, title: &str, channel: bool, extra: &str) -> String {
    let sg = id + 1000;
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{sg},"is_channel":{channel}}},"unread_count":0{extra}}}}}"#
    )
}

fn supergroup(id: i64, channel: bool, fields: &str) -> String {
    let sg = id + 1000;
    format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{sg},"is_channel":{channel},"member_count":640,"status":{OWNER}{fields}}}}}"#
    )
}

fn full_info(id: i64, fields: &str) -> String {
    let sg = id + 1000;
    format!(
        r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{sg},"supergroup_full_info":{{"description":"","member_count":640{fields}}}}}"#
    )
}

/// An owned private supergroup, a linked discussion group, an owned
/// channel, a basic group and two groups the channel could link.
fn apply_group_admin(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    session.my_user_id = Some(777);
    let reactions = r#","available_reactions":{"@type":"chatAvailableReactionsSome","reactions":[{"@type":"reactionTypeEmoji","emoji":"👍"},{"@type":"reactionTypeEmoji","emoji":"❤"},{"@type":"reactionTypeEmoji","emoji":"🔥"}],"max_reaction_count":3}"#;
    for json in [
        supergroup_chat(GROUP, "Rust Programmers", false, reactions),
        supergroup(GROUP, false, ""),
        full_info(
            GROUP,
            r#","can_hide_members":true,"has_hidden_members":false,"is_all_history_available":true"#,
        ),
        supergroup_chat(CHANNEL, "Compiler Weekly", true, ""),
        supergroup(CHANNEL, true, ""),
        full_info(CHANNEL, r#","can_enable_paid_reaction":true"#),
        supergroup_chat(CANDIDATE_A, "Compiler Weekly Chat", false, ""),
        supergroup(CANDIDATE_A, false, ""),
        supergroup_chat(CANDIDATE_B, "Weekend Hikers", false, ""),
        supergroup(CANDIDATE_B, false, ""),
        supergroup_chat(LINKED_GROUP, "Systems Digest Discussion", false, ""),
        supergroup(LINKED_GROUP, false, r#","join_to_send_messages":true"#),
        full_info(LINKED_GROUP, r#","linked_chat_id":-1001000000009"#),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{BASIC},"title":"Family Plans","type":{{"@type":"chatTypeBasicGroup","basic_group_id":7}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":7,"member_count":12,"status":{OWNER},"is_active":true}}}}"#
        ),
        r#"{"@type":"updateActiveEmojiReactions","emojis":["👍","👎","❤","🔥","🥰","👏","😁","🤔","🤯","😱","🎉","🤩"]}"#
            .to_string(),
        supergroup_chat(-1001000000009, "Systems Digest", true, ""),
        supergroup(-1001000000009, true, ""),
    ] {
        apply(session, sink, seq, &json);
    }
    let suitable = session.request(
        RequestPurpose::GetProfileChats(ProfileChatsKind::SuitableDiscussionChats),
        None,
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[{CANDIDATE_A},{CANDIDATE_B}]}}"#,
            suitable.0
        ),
    );
}

impl QuillApp {
    /// `QUILL_DEMO_GROUP_ADMIN=group|channel|basic|reactions|discussion|
    /// linked|confirm` (default `group`).
    pub(super) fn demo_setup_group_admin_settings(
        &mut self,
        demo: Option<ScreenshotDemo>,
        cx: &mut Context<Self>,
    ) {
        if demo != Some(ScreenshotDemo::ReadyGroupAdminSettings) {
            return;
        }
        let mode = std::env::var("QUILL_DEMO_GROUP_ADMIN").unwrap_or_else(|_| "group".into());
        let (chat, view) = match mode.as_str() {
            "channel" => (CHANNEL, GroupSettingsView::Main),
            "basic" => (BASIC, GroupSettingsView::Main),
            "reactions" => (GROUP, GroupSettingsView::Reactions),
            "discussion" => (CHANNEL, GroupSettingsView::Discussion),
            "linked" => (LINKED_GROUP, GroupSettingsView::Main),
            "confirm" => (BASIC, GroupSettingsView::ConfirmUpgrade),
            _ => (GROUP, GroupSettingsView::Main),
        };
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_group_admin(session, &self.demo_sink, &self.demo_seq);
            session.open_chat(ChatId(chat));
            if chat != BASIC {
                session.open_info_panel = Some(InfoPanelTarget::Supergroup(chat));
            }
        }
        self.open_group_settings_dialog(ChatId(chat), cx);
        if let Some(dialog) = self.group_settings_dialog.as_mut() {
            dialog.view = view;
        }
    }
}
