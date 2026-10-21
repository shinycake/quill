//! `ready-peer-qr`: your own QR code box open over the chat list, with
//! six colors to pick from (injected, no live Telegram).
//!
//! - `QUILL_DEMO_PEER_QR_SUBJECT=friend|group|invite`: another user's,
//!   a public group's or an invite link's box instead of your own.
//! - `QUILL_DEMO_PEER_QR_COLOR=<n>`: pick the n-th offered color (1-6).
//! - `QUILL_DEMO_PEER_QR_EMOJI=<a>,<b>`: pick emoji for the pattern.
//! - `QUILL_DEMO_PEER_QR_PANEL=colors|emoji`: open a panel.
//! - `QUILL_DEMO_PEER_QR_PRICE=<stars>`: charge for messages (the pill).
//! - `QUILL_DEMO_PEER_QR_FREE=temporary|permanent`: the code already
//!   lets scanners message you free (no server to ask in the demo).
//!
//! The GPUI tests in `tests.rs` drive the same demo.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::demo::seed_ready_chats_session;
use super::super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::peer_qr::appearance::{ShareThemeBackdrop, SwatchKey};
use quill::peer_qr::color::Rgb;
use quill::peer_qr::free::FreeMode;
use quill::peer_qr::session::BackdropsFetch;
use quill::peer_qr::state::{QrAction, QrPanel, QrPeer, QrSubject};
use quill::privacy::{NewChatPrivacy, NewChatPrivacyState};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-peer-qr",
        seed_peer_qr_session,
        "screenshot demo — your own QR code box (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_peer_qr),
];

/// The account, another user and a public group, all with usernames.
pub(super) const ME: i64 = 501;
pub(super) const FRIEND: i64 = 502;
pub(super) const GROUP_CHAT: i64 = -100_777;
const GROUP_SUPERGROUP: i64 = 777;

/// The colors the server offers, `(id, center, edge)`.
pub(super) const BACKDROPS: [(i32, u32, u32); 6] = [
    (1, 0x4B8FE0, 0x1D3E7A),
    (2, 0x5FBF7A, 0x1F5C3A),
    (3, 0x9B6FE0, 0x3E2A78),
    (4, 0xF0A050, 0x7A3A1A),
    (5, 0xE87FB0, 0x7A2A55),
    (6, 0x4FC3C8, 0x1A5560),
];

pub(super) fn seed_peer_qr_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    session.my_user_id = Some(ME);
    let seq = AtomicU64::new(session.last_seq);
    let mut apply = |json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    let user = |id: i64, first: &str, username: &str, phone: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}","collectible_usernames":[]}},"phone_number":"{phone}","status":{{"@type":"userStatusRecently"}},"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    apply(&user(ME, "Avery", "averystone", "15550101"));
    apply(&user(FRIEND, "Maya", "mayachen", ""));
    apply(&format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{FRIEND},"title":"Maya","type":{{"@type":"chatTypePrivate","user_id":{FRIEND}}},"unread_count":0}}}}"#
    ));
    apply(&format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{GROUP_SUPERGROUP},"usernames":{{"@type":"usernames","active_usernames":["quillreaders"],"disabled_usernames":[],"editable_username":"quillreaders","collectible_usernames":[]}},"status":{{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}},"member_count":42,"is_channel":false}}}}"#
    ));
    apply(&format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP_CHAT},"title":"Quill Readers","type":{{"@type":"chatTypeSupergroup","supergroup_id":{GROUP_SUPERGROUP},"is_channel":false}},"unread_count":0,"permissions":{{"@type":"chatPermissions","can_change_info":true}}}}}}"#
    ));
    session.users_state.peer_qr.backdrops = BackdropsFetch::Ready(
        BACKDROPS
            .iter()
            .map(|(id, center, edge)| ShareThemeBackdrop {
                id: *id,
                name: format!("demo-{id}"),
                center: Rgb::hex(*center),
                edge: Rgb::hex(*edge),
            })
            .collect(),
    );
    let price = std::env::var("QUILL_DEMO_PEER_QR_PRICE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    session.settings.privacy_data.new_chat = Some(NewChatPrivacyState::Ready(NewChatPrivacy {
        allow_from_unknown: true,
        incoming_paid_message_star_count: price,
    }));
    session
}

impl QuillApp {
    fn demo_peer_qr(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match std::env::var("QUILL_DEMO_PEER_QR_SUBJECT").as_deref() {
            Ok("friend") => self.open_peer_qr_box(QrPeer::User(FRIEND), cx),
            Ok("group") => self.open_peer_qr_box(QrPeer::Chat(GROUP_CHAT), cx),
            Ok("invite") => self.open_invite_link_qr("https://t.me/+quillreaders", cx),
            _ => self.open_qr_box(QrSubject::Peer(QrPeer::User(ME)), cx),
        }
        let color = std::env::var("QUILL_DEMO_PEER_QR_COLOR")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .and_then(|index| BACKDROPS.get(index.checked_sub(1)?))
            .map(|(id, _, _)| SwatchKey::Backdrop(*id));
        if let Some(color) = color {
            self.qr_act(QrAction::PickColor(color), cx);
        }
        for emoji in std::env::var("QUILL_DEMO_PEER_QR_EMOJI")
            .unwrap_or_default()
            .split(',')
            .filter(|emoji| !emoji.is_empty())
        {
            self.qr_act(QrAction::ToggleEmoji(emoji.to_string()), cx);
        }
        let free = |permanent: bool| quill::peer_qr::free::FreeCode {
            link: quill::peer_qr::link::UserLinkData {
                url: "https://t.me/+free-demo".into(),
                expires_at: if permanent {
                    0
                } else {
                    quill::local_time::now_unix() + 3_600
                },
            },
            can_permanent: true,
        };
        let mode = match std::env::var("QUILL_DEMO_PEER_QR_FREE").as_deref() {
            Ok("temporary") => Some(FreeMode::Temporary { code: free(false) }),
            Ok("permanent") => Some(FreeMode::Permanent { code: free(true) }),
            _ => None,
        };
        if let (Some(mode), Some(open)) = (mode, self.peer_qr.open.as_mut()) {
            open.state.free.mode = mode;
        }
        let panel = match std::env::var("QUILL_DEMO_PEER_QR_PANEL").as_deref() {
            Ok("colors") => Some(QrPanel::Colors),
            Ok("emoji") => Some(QrPanel::Emoji),
            _ => None,
        };
        if let Some(panel) = panel {
            self.qr_act(QrAction::OpenPanel(panel), cx);
        }
        cx.notify();
    }
}
