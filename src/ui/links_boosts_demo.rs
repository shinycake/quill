//! Screenshot-demo fixtures for the admin links, boosts and usernames batch
//! (`--screenshot-demo ready-links-boosts`, mode from
//! `QUILL_DEMO_LINKS_BOOSTS`). Everything goes through the real reducers;
//! no live Telegram. English-only text.

use super::app::QuillApp;
use super::auth_ui::render_qr_image;
use super::dialogs::GroupSettingsView;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{
    AdminLinksState, BoostsListState, InfoPanelTarget, InviteLinkFetch, LinkRequestsState,
    RequestPurpose, Session,
};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const CHAT: i64 = 811;
const SUPERGROUP: i64 = CHAT + 1000;
const LINK: &str = "https://t.me/+moderatorslink";

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn user(id: i64, first: &str, last: &str) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
    )
}

fn link(
    invite_link: &str,
    name: &str,
    creator: i64,
    uses: i32,
    pending: i32,
    revoked: bool,
) -> String {
    format!(
        r#"{{"@type":"chatInviteLink","invite_link":"{invite_link}","name":"{name}","creator_user_id":{creator},"date":1788000000,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":{uses},"expired_member_count":0,"pending_join_request_count":{pending},"creates_join_request":false,"is_primary":false,"is_revoked":{revoked}}}"#
    )
}

fn seed(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    session.my_user_id = Some(777);
    let usernames = r#","usernames":{"@type":"usernames","active_usernames":["rustaceans","rust_club"],"disabled_usernames":["rust_weekly"],"editable_username":"rustaceans","collectible_usernames":["rust_club","rust_weekly"]}"#;
    for json in [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Rust Programmers","type":{{"@type":"chatTypeSupergroup","supergroup_id":{SUPERGROUP},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{SUPERGROUP},"is_channel":false,"member_count":640,"status":{OWNER}{usernames}}}}}"#
        ),
        user(7001, "Dana", "Levi"),
        user(7002, "Omar", "Haddad"),
        user(7003, "Maya", "Chen"),
        user(7004, "Lior", "Katz"),
    ] {
        apply(session, sink, seq, &json);
    }
    let chat = Some(ChatId(CHAT));
    let links = session.request(RequestPurpose::GetChatInviteLinks, chat);
    let counts = session.request(RequestPurpose::GetChatInviteLinkCounts, chat);
    for json in [
        format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
            links.0,
            link("https://t.me/+primarylink", "", 777, 204, 0, false),
            link(LINK, "Moderators", 777, 8, 2, false),
        ),
        format!(
            r#"{{"@type":"chatInviteLinkCounts","@extra":"{}","invite_link_counts":[{{"@type":"chatInviteLinkCount","user_id":777,"invite_link_count":2,"revoked_invite_link_count":0}},{{"@type":"chatInviteLinkCount","user_id":7001,"invite_link_count":2,"revoked_invite_link_count":1}}]}}"#,
            counts.0
        ),
    ] {
        apply(session, sink, seq, &json);
    }
}

fn seed_boosts(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, gifts: bool) {
    let chat = Some(ChatId(CHAT));
    session.chat_boost_status.insert(CHAT, (3, 14));
    let link_extra = session.request(RequestPurpose::GetChatBoostLink, chat);
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chatBoostLink","@extra":"{}","link":"https://t.me/boost/rustaceans","is_public":true}}"#,
            link_extra.0
        ),
    );
    let extra = session.request(RequestPurpose::GetChatBoosts { append: false }, chat);
    session.chat_boost_lists.insert(
        CHAT,
        BoostsListState {
            only_gifts: gifts,
            total_count: 0,
            boosts: Vec::new(),
            next_offset: String::new(),
            loading: true,
            error: None,
            request: Some(extra),
        },
    );
    let premium = |id: &str, user: i64, count: i32| {
        format!(
            r#"{{"@type":"chatBoost","id":"{id}","count":{count},"source":{{"@type":"chatBoostSourcePremium","user_id":{user}}},"start_date":1788000000,"expiration_date":1798000000}}"#
        )
    };
    let boosts = if gifts {
        vec![
            r#"{"@type":"chatBoost","id":"g1","count":1,"source":{"@type":"chatBoostSourceGiftCode","user_id":7003,"gift_code":"x"},"start_date":1788000000,"expiration_date":1799000000}"#.to_string(),
            r#"{"@type":"chatBoost","id":"g2","count":1,"source":{"@type":"chatBoostSourceGiveaway","user_id":7004,"gift_code":"","star_count":0,"giveaway_message_id":5,"is_unclaimed":false},"start_date":1788000000,"expiration_date":1799500000}"#.to_string(),
            r#"{"@type":"chatBoost","id":"g3","count":1,"source":{"@type":"chatBoostSourceGiveaway","user_id":0,"gift_code":"","star_count":0,"giveaway_message_id":5,"is_unclaimed":true},"start_date":1788000000,"expiration_date":1799500000}"#.to_string(),
        ]
    } else {
        vec![
            premium("p1", 7001, 2),
            premium("p2", 7002, 1),
            premium("p3", 7003, 1),
        ]
    };
    let (total, next) = if gifts { (3, "") } else { (14, "more") };
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"foundChatBoosts","@extra":"{}","total_count":{total},"boosts":[{}],"next_offset":"{next}"}}"#,
            extra.0,
            boosts.join(",")
        ),
    );
}

fn seed_admin_links(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let chat = Some(ChatId(CHAT));
    let active = session.request(
        RequestPurpose::GetAdminChatInviteLinks { revoked: false },
        chat,
    );
    let revoked = session.request(
        RequestPurpose::GetAdminChatInviteLinks { revoked: true },
        chat,
    );
    session.admin_invite_links.insert(
        CHAT,
        AdminLinksState {
            creator_user_id: 7001,
            active: InviteLinkFetch::Loading,
            revoked: InviteLinkFetch::Loading,
            active_request: Some(active),
            revoked_request: Some(revoked),
        },
    );
    for json in [
        format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
            active.0,
            link("https://t.me/+danaone", "Newsletter", 7001, 31, 0, false),
            link("https://t.me/+danatwo", "Meetup", 7001, 12, 0, false),
        ),
        format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":1,"invite_links":[{}]}}"#,
            revoked.0,
            link("https://t.me/+danaold", "Old campaign", 7001, 41, 0, true),
        ),
    ] {
        apply(session, sink, seq, &json);
    }
}

fn seed_link_requests(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let extra = session.request(
        RequestPurpose::GetLinkJoinRequests { append: false },
        Some(ChatId(CHAT)),
    );
    session.link_join_requests.insert(
        CHAT,
        LinkRequestsState {
            invite_link: LINK.into(),
            total_count: 0,
            requests: Vec::new(),
            loading: true,
            error: None,
            request: Some(extra),
        },
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7003,"date":1788500000,"bio":""}},{{"@type":"chatJoinRequest","user_id":7004,"date":1788550000,"bio":""}}]}}"#,
            extra.0
        ),
    );
}

register_demos![
    // Admin links, boosts and usernames
    // (`QUILL_DEMO_LINKS_BOOSTS=usernames|boosts|gifts|admin-links|
    // link-requests|qr`; injected data, no live Telegram).
    DemoSpec::chats(
        "ready-links-boosts",
        "screenshot demo — admin links, boosts and usernames (injected, no live Telegram)",
    )
    .setup(|app, _, cx| app.demo_setup_links_boosts(cx)),
];

impl QuillApp {
    /// `QUILL_DEMO_LINKS_BOOSTS=usernames|boosts|gifts|admin-links|
    /// link-requests|qr` (default `usernames`).
    fn demo_setup_links_boosts(&mut self, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_LINKS_BOOSTS").unwrap_or_else(|_| "usernames".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            seed(session, &self.demo_sink, &self.demo_seq);
            match mode.as_str() {
                "boosts" => seed_boosts(session, &self.demo_sink, &self.demo_seq, false),
                "gifts" => seed_boosts(session, &self.demo_sink, &self.demo_seq, true),
                "admin-links" => seed_admin_links(session, &self.demo_sink, &self.demo_seq),
                "link-requests" => seed_link_requests(session, &self.demo_sink, &self.demo_seq),
                _ => {}
            }
            session.open_chat(ChatId(CHAT));
            session.open_info_panel = Some(InfoPanelTarget::Supergroup(CHAT));
        }
        match mode.as_str() {
            "usernames" | "boosts" | "gifts" => {
                self.open_group_settings_dialog(ChatId(CHAT), cx);
                if let Some(dialog) = self.group_settings_dialog.as_mut() {
                    dialog.view = if mode == "usernames" {
                        GroupSettingsView::Usernames
                    } else {
                        GroupSettingsView::Boosts
                    };
                }
            }
            "qr" => {
                self.invite_link_qr =
                    render_qr_image(LINK).map(|image| (ChatId(CHAT), LINK.to_owned(), image));
            }
            _ => {}
        }
        self.invite_link_details = None;
        self.status_note = "screenshot demo - admin links, boosts and usernames".into();
    }
}
