//! Share theme driver tests (`parity:profile-share-theme`): a peer's saved
//! theme from its full info, who may save, the gifts panel's pages and
//! `setProfileShareTheme`, with TDLib 1.8.69 JSON shapes (recorded,
//! nothing live).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::peer_qr::appearance::SwatchKey;
use crate::peer_qr::state::{QrPeer, SaveTicket};
use crate::peer_qr::theme::{SaveRights, ShareTheme, ThemeChoice, ThemeEmoji, can_save};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChannelMemberStatus;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 7;
const FRIEND: i64 = 21;
const CHANNEL: i64 = -1_000_113;
const SUPERGROUP: i64 = 113;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
        driver.session.my_user_id = Some(ME);
        Self {
            driver,
            recorder,
            seq,
            sink,
            _dir: dir,
        }
    }

    fn feed(&mut self, json: Value) {
        self.driver
            .ingest(copy_and_parse(&json.to_string(), &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn last(&self) -> Value {
        serde_json::from_str(self.recorder.snapshot().last().unwrap()).unwrap()
    }

    fn sent(&self) -> usize {
        self.recorder.snapshot().len()
    }

    /// A channel `status` names your rights in, with or without a public
    /// username.
    fn channel(&mut self, status: Value, username: bool) {
        self.feed(
            json!({"@type": "updateNewChat", "chat": {"id": CHANNEL, "title": "News",
            "type": {"@type": "chatTypeSupergroup", "supergroup_id": SUPERGROUP,
                "is_channel": true}, "unread_count": 0}}),
        );
        let usernames = if username {
            json!({"@type": "usernames", "active_usernames": ["news"],
                "disabled_usernames": [], "editable_username": "news",
                "collectible_usernames": []})
        } else {
            Value::Null
        };
        self.feed(
            json!({"@type": "updateSupergroup", "supergroup": {"@type": "supergroup",
            "id": SUPERGROUP, "status": status, "is_channel": true, "usernames": usernames}}),
        );
    }
}

fn emoji_theme() -> Value {
    json!({"@type": "profileShareThemeEmoji", "emojis": [
            {"@type": "emojiTypeRegular", "emoji": "🐱"},
            {"@type": "emojiTypeCustomEmoji", "custom_emoji_id": "5368324170671202286"}],
        "use_default_background": false, "background_backdrop_id": 4})
}

fn creator() -> Value {
    json!({"@type": "chatMemberStatusCreator", "is_member": true})
}

#[test]
fn a_peers_full_info_carries_its_theme() {
    let mut rig = Rig::new();
    rig.driver.ensure_share_theme(QrPeer::User(FRIEND)).unwrap();
    let request = rig.last();
    assert_eq!(request["@type"], "getUserFullInfo");
    assert_eq!(request["user_id"], FRIEND);
    rig.feed(json!({"@type": "userFullInfo", "@extra": request["@extra"],
        "share_theme": emoji_theme()}));
    assert_eq!(
        rig.driver.session.qr_served_theme(QrPeer::User(FRIEND)),
        Some(&ShareTheme::Emoji {
            backdrop: Some(4),
            emoji: vec![
                ThemeEmoji::Plain("🐱".into()),
                ThemeEmoji::Custom(5_368_324_170_671_202_286),
            ],
        })
    );
    // A later update replaces it.
    rig.feed(json!({"@type": "updateUserFullInfo", "user_id": FRIEND,
        "user_full_info": {"@type": "userFullInfo", "share_theme": null}}));
    assert_eq!(
        rig.driver.session.qr_served_theme(QrPeer::User(FRIEND)),
        Some(&ShareTheme::Default)
    );
}

#[test]
fn a_channels_theme_comes_with_its_supergroup_full_info() {
    let mut rig = Rig::new();
    rig.channel(creator(), true);
    rig.driver
        .ensure_share_theme(QrPeer::Chat(CHANNEL))
        .unwrap();
    let request = rig.last();
    assert_eq!(request["@type"], "getSupergroupFullInfo");
    rig.feed(
        json!({"@type": "supergroupFullInfo", "@extra": request["@extra"],
        "share_theme": emoji_theme()}),
    );
    assert!(matches!(
        rig.driver.session.qr_served_theme(QrPeer::Chat(CHANNEL)),
        Some(ShareTheme::Emoji {
            backdrop: Some(4),
            ..
        })
    ));
}

/// The channel with your status and `can_change_info` right as
/// `getChatMember(me)` left them.
fn set_rights(rig: &mut Rig, status: ChannelMemberStatus, can_change_info: Option<bool>) {
    let chat = rig.driver.session.chats.get_mut(&CHANNEL).unwrap();
    chat.set_member_status(status, None);
    chat.set_admin_can_change_info(can_change_info);
}

#[test]
fn who_may_save_follows_share_themes_can_save() {
    let mut rig = Rig::new();
    let session = &rig.driver.session;
    assert_eq!(session.qr_save_rights(QrPeer::User(ME)), SaveRights::Me);
    assert!(can_save(session.qr_save_rights(QrPeer::User(ME))));
    assert!(!can_save(session.qr_save_rights(QrPeer::User(FRIEND))));
    assert!(!can_save(session.qr_save_rights(QrPeer::Chat(CHANNEL))));
    for (status, can_change_info, username, expected) in [
        (ChannelMemberStatus::Creator, None, true, true),
        (ChannelMemberStatus::Administrator, Some(true), true, true),
        (ChannelMemberStatus::Administrator, Some(false), true, false),
        (ChannelMemberStatus::Creator, None, false, false),
        (ChannelMemberStatus::Member, None, true, false),
    ] {
        rig.channel(creator(), username);
        set_rights(&mut rig, status, can_change_info);
        assert_eq!(
            can_save(rig.driver.session.qr_save_rights(QrPeer::Chat(CHANNEL))),
            expected,
            "{status:?} {can_change_info:?} with username {username}"
        );
    }
}

#[test]
fn a_save_sends_the_theme_and_keeps_it_once_answered() {
    let mut rig = Rig::new();
    let ticket = SaveTicket {
        box_id: 1,
        serial: 1,
    };
    let choice = ThemeChoice::Emoji {
        color: SwatchKey::Backdrop(4),
        emoji: vec![ThemeEmoji::Plain("🐱".into())],
    };
    rig.driver
        .save_share_theme(ticket, QrPeer::User(ME), choice.clone())
        .unwrap();
    let request = rig.last();
    assert_eq!(request["@type"], "setProfileShareTheme");
    assert_eq!(request["chat_id"], ME);
    assert_eq!(request["theme"]["@type"], "inputProfileShareThemeEmoji");
    assert_eq!(request["theme"]["background_backdrop_id"], 4);
    assert_eq!(request["theme"]["use_default_background"], false);
    assert_eq!(request["theme"]["emojis"][0]["emoji"], "🐱");
    rig.feed(json!({"@type": "ok", "@extra": request["@extra"]}));
    let answers = rig.driver.session.users_state.peer_qr.take_save_answers();
    assert_eq!(answers, vec![(ticket, Ok(choice.clone()))]);
    assert_eq!(
        rig.driver.session.qr_served_theme(QrPeer::User(ME)),
        Some(&choice.to_theme())
    );
}

#[test]
fn a_refused_save_reports_why() {
    let mut rig = Rig::new();
    let ticket = SaveTicket {
        box_id: 1,
        serial: 2,
    };
    rig.driver
        .save_share_theme(ticket, QrPeer::User(ME), ThemeChoice::default())
        .unwrap();
    let request = rig.last();
    assert_eq!(request["theme"], Value::Null, "the default theme is null");
    rig.feed(
        json!({"@type": "error", "@extra": request["@extra"], "code": 400,
        "message": "THEME_INVALID"}),
    );
    let answers = rig.driver.session.users_state.peer_qr.take_save_answers();
    assert_eq!(answers.len(), 1);
    assert!(
        answers[0]
            .1
            .as_ref()
            .is_err_and(|reason| !reason.is_empty())
    );
    assert_eq!(rig.driver.session.qr_served_theme(QrPeer::User(ME)), None);
}

#[test]
fn nobody_else_saves_and_no_request_goes_out() {
    let mut rig = Rig::new();
    let ticket = SaveTicket {
        box_id: 1,
        serial: 3,
    };
    let sent = rig.sent();
    assert!(
        rig.driver
            .save_share_theme(ticket, QrPeer::User(FRIEND), ThemeChoice::default())
            .is_err()
    );
    assert_eq!(rig.sent(), sent);
    let answers = rig.driver.session.users_state.peer_qr.take_save_answers();
    assert_eq!(answers, vec![(ticket, Err(String::new()))]);
}

fn upgraded(received: &str, id: i64, burned: bool) -> Value {
    json!({"@type": "receivedGift", "received_gift_id": received, "date": 5,
        "gift": {"@type": "sentGiftUpgraded", "gift": {
            "@type": "upgradedGift", "id": id.to_string(), "title": "Plush Pepe",
            "name": format!("PlushPepe-{id}"), "number": id, "is_burned": burned,
            "backdrop": {"@type": "upgradedGiftBackdrop", "id": 1, "name": "Ocean",
                "colors": {"@type": "upgradedGiftBackdropColors", "center_color": 5934320,
                    "edge_color": 2838453, "symbol_color": 1193046, "text_color": 16777215},
                "rarity": {"@type": "upgradedGiftAttributeRarityPerMille", "per_mille": 10}},
            "owner_id": {"@type": "messageSenderUser", "user_id": ME}}}})
}

#[test]
fn the_gifts_panel_pages_through_your_upgraded_gifts() {
    let mut rig = Rig::new();
    rig.driver
        .fetch_share_theme_gifts(QrPeer::User(ME), false)
        .unwrap();
    let first = rig.last();
    assert_eq!(first["@type"], "getReceivedGifts");
    assert_eq!(first["owner_id"]["user_id"], ME);
    assert_eq!(first["exclude_unlimited"], true);
    assert_eq!(first["exclude_upgradable"], true);
    assert_eq!(first["exclude_non_upgradable"], true);
    assert_eq!(first["exclude_hosted"], true);
    assert_eq!(first["offset"], "");
    assert_eq!(first["limit"], 50);
    let sent = rig.sent();
    rig.driver
        .fetch_share_theme_gifts(QrPeer::User(ME), true)
        .unwrap();
    assert_eq!(rig.sent(), sent, "no next page before the first one");
    rig.feed(
        json!({"@type": "receivedGifts", "@extra": first["@extra"], "total_count": 3,
        "next_offset": "page2",
        "gifts": [upgraded("r1", 101, false), upgraded("r2", 102, true)]}),
    );
    let gifts = &rig.driver.session.users_state.peer_qr.gifts;
    assert_eq!(gifts.list.len(), 1, "burned gifts are left out");
    assert_eq!(gifts.list[0].id, 101);
    assert_eq!(gifts.list[0].received_id, "r1");
    assert!(gifts.loaded && !gifts.complete);
    rig.driver
        .fetch_share_theme_gifts(QrPeer::User(ME), true)
        .unwrap();
    let second = rig.last();
    assert_eq!(second["offset"], "page2");
    rig.feed(
        json!({"@type": "receivedGifts", "@extra": second["@extra"], "total_count": 3,
        "next_offset": "", "gifts": [upgraded("r1", 101, false), upgraded("r3", 103, false)]}),
    );
    let gifts = &rig.driver.session.users_state.peer_qr.gifts;
    let ids: Vec<i64> = gifts.list.iter().map(|gift| gift.id).collect();
    assert_eq!(ids, vec![101, 103], "deduplicated by gift id");
    assert!(gifts.complete);
    let sent = rig.sent();
    rig.driver
        .fetch_share_theme_gifts(QrPeer::User(ME), true)
        .unwrap();
    assert_eq!(rig.sent(), sent, "nothing after the last page");
}

#[test]
fn a_stale_gifts_page_is_dropped() {
    let mut rig = Rig::new();
    rig.driver
        .fetch_share_theme_gifts(QrPeer::User(ME), false)
        .unwrap();
    let stale = rig.last();
    rig.driver
        .fetch_share_theme_gifts(QrPeer::User(ME), false)
        .unwrap();
    rig.feed(
        json!({"@type": "receivedGifts", "@extra": stale["@extra"], "total_count": 1,
        "next_offset": "", "gifts": [upgraded("r1", 101, false)]}),
    );
    let gifts = &rig.driver.session.users_state.peer_qr.gifts;
    assert!(gifts.list.is_empty() && gifts.loading);
}

#[test]
fn a_peer_you_cannot_style_lists_no_gifts() {
    let mut rig = Rig::new();
    let sent = rig.sent();
    assert!(
        rig.driver
            .fetch_share_theme_gifts(QrPeer::User(FRIEND), false)
            .is_err()
    );
    assert_eq!(rig.sent(), sent);
}
