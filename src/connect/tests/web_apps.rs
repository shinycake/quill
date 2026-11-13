//! Connect-driver tests: mini apps (`connect/web_apps.rs`,
//! `state/session_web_apps.rs`).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{Session, WebAppOpenResult, WriteAccessResult, WriteAnswer};
use crate::telegram::client::copy_and_parse;
use crate::web_app::LaunchSource;
use crate::web_app::theme::ThemeParams;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Harness {
    fn ready() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut harness = Self {
            driver,
            recorder,
            sink: dyn_sink,
            seq: AtomicU64::new(0),
            dir,
        };
        harness.ingest(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        harness
    }

    fn ingest(&mut self, json: &str) {
        let parsed = copy_and_parse(json, &self.seq, &self.sink).expect("parse");
        self.driver.ingest(parsed).expect("ingest");
    }

    fn last(&self, ty: &str) -> Value {
        let json = self
            .recorder
            .snapshot()
            .into_iter()
            .rfind(|json| json.contains(&format!("\"@type\":\"{ty}\"")))
            .unwrap_or_else(|| panic!("no {ty} sent"));
        serde_json::from_str(&json).unwrap()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn open_web_app_sends_theme_and_keeps_the_launch_context() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .open_web_app(
            ChatId(5),
            42,
            "https://bot.example/app",
            LaunchSource::InlineButton,
            &theme,
            None,
        )
        .expect("sent");
    let sent = h.last("openWebApp");
    assert_eq!(sent["chat_id"], 5);
    assert_eq!(sent["bot_user_id"], 42);
    assert_eq!(sent["url"], "https://bot.example/app");
    assert_eq!(sent["parameters"]["theme"]["@type"], "themeParameters");
    assert_eq!(
        sent["parameters"]["mode"]["@type"],
        "webAppOpenModeFullSize"
    );
    assert_eq!(
        h.driver
            .session
            .bots
            .web_apps
            .opening()
            .as_ref()
            .map(|p| p.bot_user_id),
        Some(42)
    );

    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"777","url":{{"@type":"webAppUrl","url":"https://bot.example/app?tgWebAppData=x","require_same_origin":false}}}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.open_result().cloned(),
        Some(WebAppOpenResult::Opened {
            bot_user_id: 42,
            chat_id: Some(5),
            source: LaunchSource::InlineButton,
            window: None,
            launch_id: 777,
            url: "https://bot.example/app?tgWebAppData=x".into(),
        })
    );
    assert!(h.driver.session.bots.web_apps.opening().is_none());
}

/// Sweep r4-12: the window the launch was pressed in rides with the open
/// request, so the answer opens the app there whatever pass lands it.
#[test]
fn the_open_answer_carries_the_window_the_launch_was_pressed_in() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .open_web_app(
            ChatId(5),
            42,
            "https://bot.example/app",
            LaunchSource::InlineButton,
            &theme,
            Some(ChatId(12)),
        )
        .expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"7","url":{{"@type":"webAppUrl","url":"https://bot.example/app?x","require_same_origin":false}}}}"#,
        extra.as_extra()
    ));
    let Some(WebAppOpenResult::Opened { window, .. }) =
        h.driver.session.bots.web_apps.take_open_result()
    else {
        panic!("not opened");
    };
    assert_eq!(window, Some(ChatId(12)));
}

#[test]
fn a_refused_open_reports_a_failure_and_clears_the_context() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .get_main_web_app(ChatId(5), 42, "promo", &theme, None)
        .expect("sent");
    assert_eq!(h.last("getMainWebApp")["start_parameter"], "promo");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":404,"message":"BOT_APP_INVALID"}}"#,
        extra.as_extra()
    ));
    assert!(matches!(
        h.driver.session.bots.web_apps.open_result().cloned(),
        Some(WebAppOpenResult::Failed {
            bot_user_id: 42,
            ..
        })
    ));
    assert!(h.driver.session.bots.web_apps.opening().is_none());
}

/// Two keyboard-button launches before either answers (a double click):
/// the latest asked is the current one. The first answer opens nothing and
/// its app is closed; the second keeps its own source and chat.
#[test]
fn only_the_latest_launch_asked_opens() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let first = h
        .driver
        .open_web_app(
            ChatId(5),
            42,
            "https://bot.example/a",
            LaunchSource::InlineButton,
            &theme,
            None,
        )
        .expect("sent");
    let second = h
        .driver
        .get_web_app_url(ChatId(5), 43, "https://bot.example/b", "Open", &theme, None)
        .expect("sent");
    assert_eq!(
        h.driver
            .session
            .bots
            .web_apps
            .opening()
            .as_ref()
            .map(|o| o.request),
        Some(second)
    );
    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"801","url":{{"@type":"webAppUrl","url":"https://bot.example/a?x","require_same_origin":false}}}}"#,
        first.as_extra()
    ));
    assert_eq!(h.driver.session.bots.web_apps.open_result().cloned(), None);
    assert_eq!(h.last("closeWebApp")["web_app_launch_id"], 801);
    h.ingest(&format!(
        r#"{{"@type":"webAppUrl","@extra":"{}","url":"https://bot.example/b?x","require_same_origin":false}}"#,
        second.as_extra()
    ));
    let Some(WebAppOpenResult::Opened {
        source,
        chat_id,
        bot_user_id,
        ..
    }) = h.driver.session.bots.web_apps.take_open_result()
    else {
        panic!("not opened");
    };
    assert!(source.allows_data_send(), "still the keyboard button");
    assert_eq!((chat_id, bot_user_id), (Some(5), 43));
    assert!(h.driver.session.bots.web_apps.opening().is_none());
}

/// Sweep r4-03 round 2 (item 15): a second launch whose send is refused
/// leaves the first launch as it was, so it still opens.
#[test]
fn a_refused_second_launch_leaves_the_first_alone() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let first = h
        .driver
        .open_web_app(
            ChatId(5),
            42,
            "https://bot.example/a",
            LaunchSource::InlineButton,
            &theme,
            None,
        )
        .expect("sent");
    h.recorder.refuse_sends(true);
    assert!(
        h.driver
            .get_web_app_url(ChatId(5), 43, "https://bot.example/b", "Open", &theme, None)
            .is_err()
    );
    h.recorder.refuse_sends(false);
    assert_eq!(
        h.driver.session.bots.web_apps.opening().map(|o| o.request),
        Some(first)
    );
    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"801","url":{{"@type":"webAppUrl","url":"https://bot.example/a?x","require_same_origin":false}}}}"#,
        first.as_extra()
    ));
    assert!(matches!(
        h.driver.session.bots.web_apps.open_result(),
        Some(WebAppOpenResult::Opened {
            bot_user_id: 42,
            ..
        })
    ));
}

#[test]
fn keyboard_button_apps_use_get_web_app_url_and_may_send_data() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .get_web_app_url(
            ChatId(5),
            42,
            "https://bot.example/form",
            "Order",
            &theme,
            None,
        )
        .expect("sent");
    let sent = h.last("getWebAppUrl");
    assert!(sent.get("chat_id").is_none());
    h.ingest(&format!(
        r#"{{"@type":"webAppUrl","@extra":"{}","url":"https://bot.example/form?x","require_same_origin":false}}"#,
        extra.as_extra()
    ));
    let Some(WebAppOpenResult::Opened {
        source, launch_id, ..
    }) = h.driver.session.bots.web_apps.open_result().cloned()
    else {
        panic!("not opened");
    };
    assert_eq!(launch_id, 0);
    assert!(source.allows_data_send());
    let extra = h
        .driver
        .send_web_app_data(42, "Order", "{\"item\":1}")
        .expect("sent");
    assert_eq!(h.last("sendWebAppData")["button_text"], "Order");
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.data_sent,
        vec![crate::state::DataSent {
            request: extra,
            result: Ok(())
        }]
    );
}

#[test]
fn write_access_goes_through_consent_on_404() {
    let mut h = Harness::ready();
    let extra = h.driver.can_bot_send_messages(42).expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        std::mem::take(&mut h.driver.session.bots.web_apps.write_access),
        [WriteAnswer::new(extra, 42, WriteAccessResult::NeedsConsent)]
    );
    let extra = h.driver.allow_bot_to_send_messages(42).expect("sent");
    assert_eq!(h.last("allowBotToSendMessages")["bot_user_id"], 42);
    assert!(
        h.driver.session.bots.web_apps.write_access.is_empty(),
        "taken while in flight"
    );
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        std::mem::take(&mut h.driver.session.bots.web_apps.write_access),
        [WriteAnswer::new(extra, 42, WriteAccessResult::Granted)]
    );
    let extra = h.driver.can_bot_send_messages(42).expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        std::mem::take(&mut h.driver.session.bots.web_apps.write_access),
        [WriteAnswer::new(extra, 42, WriteAccessResult::Allowed)]
    );
}

#[test]
fn two_write_access_answers_before_the_poll_both_survive() {
    let mut h = Harness::ready();
    let first = h.driver.can_bot_send_messages(42).expect("sent");
    let second = h.driver.can_bot_send_messages(43).expect("sent");
    for extra in [first, second] {
        h.ingest(&format!(
            r#"{{"@type":"ok","@extra":"{}"}}"#,
            extra.as_extra()
        ));
    }
    assert_eq!(
        std::mem::take(&mut h.driver.session.bots.web_apps.write_access),
        [
            WriteAnswer::new(first, 42, WriteAccessResult::Allowed),
            WriteAnswer::new(second, 43, WriteAccessResult::Allowed),
        ]
    );
}

fn bot_with_emoji_status_access(h: &mut Harness, allowed: bool) {
    h.driver.session.bots.bot_info.insert(
        42,
        Some(crate::telegram::envelope::BotInfo {
            can_manage_emoji_status: allowed,
            ..Default::default()
        }),
    );
}

fn emoji_access(h: &Harness) -> Option<bool> {
    h.driver
        .session
        .bots
        .bot_info
        .get(&42)
        .and_then(|info| info.as_ref())
        .map(|info| info.can_manage_emoji_status)
}

#[test]
fn emoji_status_access_follows_the_switch_and_comes_back_on_failure() {
    let mut h = Harness::ready();
    bot_with_emoji_status_access(&mut h, true);
    let extra = h
        .driver
        .set_bot_emoji_status_access(42, false)
        .expect("sent");
    let sent = h.last("toggleBotCanManageEmojiStatus");
    assert_eq!(sent["bot_user_id"], 42);
    assert_eq!(sent["can_manage_emoji_status"], false);
    assert_eq!(emoji_access(&h), Some(false), "the switch moves at once");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"BOT_INVALID"}}"#,
        extra.as_extra()
    ));
    assert_eq!(emoji_access(&h), Some(true), "put back");
    assert_eq!(
        h.driver.session.bots.web_apps.emoji_access,
        [crate::state::EmojiAccessAnswer {
            request: extra,
            result: crate::state::EmojiAccessResult::Refused
        }]
    );
    h.driver.session.bots.web_apps.emoji_access.clear();

    let extra = h
        .driver
        .set_bot_emoji_status_access(42, false)
        .expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert_eq!(emoji_access(&h), Some(false));
    assert_eq!(
        h.driver.session.bots.web_apps.emoji_access,
        [crate::state::EmojiAccessAnswer {
            request: extra,
            result: crate::state::EmojiAccessResult::Granted
        }]
    );
}

#[test]
fn grossing_apps_load_once_and_land_in_the_session() {
    let mut h = Harness::ready();
    let extra = h
        .driver
        .fetch_grossing_web_app_bots()
        .expect("sent")
        .expect("request");
    assert!(h.driver.session.bots.web_apps.grossing.is_loading());
    assert_eq!(
        h.driver.fetch_grossing_web_app_bots().unwrap(),
        None,
        "in flight"
    );
    h.ingest(&format!(
        r#"{{"@type":"foundUsers","@extra":"{}","user_ids":[42,43],"next_offset":""}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.grossing.data(),
        Some(&vec![42, 43])
    );
    assert!(!h.driver.session.bots.web_apps.grossing.is_loading());
    assert_eq!(
        h.driver.fetch_grossing_web_app_bots().unwrap(),
        None,
        "loaded"
    );
}

#[test]
fn attachment_menu_bots_and_custom_requests_round_trip() {
    let mut h = Harness::ready();
    h.ingest(
        r#"{"@type":"updateAttachmentMenuBots","bots":[{"@type":"attachmentMenuBot","bot_user_id":42,"name":"Shop","supports_user_chats":true,"request_write_access":true,"is_added":true,"show_in_attachment_menu":true,"show_in_side_menu":false}]}"#,
    );
    assert_eq!(h.driver.session.bots.web_apps.attachment_menu_bots.len(), 1);
    assert_eq!(
        h.driver.session.bots.web_apps.attachment_menu_bots[0].name,
        "Shop"
    );

    let extra = h.driver.fetch_attachment_menu_bot(43).expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"attachmentMenuBot","@extra":"{}","bot_user_id":43,"name":"Maps","supports_group_chats":true,"request_write_access":false,"is_added":false,"show_in_attachment_menu":true,"show_in_side_menu":false}}"#,
        extra.as_extra()
    ));
    let (asked, bot) = h.driver.session.bots.web_apps.attachment_menu_bot[0].clone();
    assert_eq!(asked, extra, "the answer names its request");
    let bot = bot.unwrap();
    assert_eq!(bot.bot_user_id, 43);
    assert!(!bot.is_added);
    let extra = h
        .driver
        .toggle_bot_in_attachment_menu(43, true, true)
        .expect("sent");
    let sent = h.last("toggleBotIsAddedToAttachmentMenu");
    assert_eq!(sent["is_added"], true);
    assert_eq!(sent["allow_write_access"], true);
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.attachment_menu_toggled,
        [crate::state::AttachmentToggle {
            request: extra,
            bot_user_id: 43,
            result: Ok(true)
        }]
    );

    let extra = h
        .driver
        .send_web_app_custom_request(42, 77, "r9", "getRequestedContact", "{}")
        .expect("sent");
    assert_eq!(
        h.last("sendWebAppCustomRequest")["method"],
        "getRequestedContact"
    );
    h.ingest(&format!(
        r#"{{"@type":"customRequestResult","@extra":"{}","result":"{{\"ok\":true}}"}}"#,
        extra.as_extra()
    ));
    let replies = std::mem::take(&mut h.driver.session.bots.web_apps.custom_replies);
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].req_id, "r9");
    assert_eq!(replies[0].launch_id, 77, "the answer names the launch");
    assert_eq!(replies[0].result, Ok("{\"ok\":true}".into()));
}

/// Round 17 C finding 15: a refused search names the bot it was for, so a
/// second link clicked before the first answered is not cancelled (or
/// launched without its consent box) by the first one's answer.
#[test]
fn a_search_answer_names_its_bot_even_when_refused() {
    let mut h = Harness::ready();
    let extra = h.driver.search_web_app(42, "shop").expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
        extra.as_extra()
    ));
    let (asked, app) = h.driver.session.bots.web_apps.found[0].clone();
    assert_eq!(asked, extra);
    assert!(app.is_err());
}

/// Two searches of one bot's apps answer in either order: each answer
/// names the request it is for, so the newer wait is never given the older
/// app's answer.
#[test]
fn two_searches_of_one_bot_keep_their_own_answers() {
    let mut h = Harness::ready();
    let first = h.driver.search_web_app(42, "shop").expect("sent");
    let second = h.driver.search_web_app(42, "maps").expect("sent");
    for (extra, name, skip) in [(second, "maps", true), (first, "shop", false)] {
        h.ingest(&format!(
            r#"{{"@type":"foundWebApp","@extra":"{}","web_app":{{"@type":"webApp","short_name":"{name}","title":"{name}","description":""}},"request_write_access":false,"skip_confirmation":{skip}}}"#,
            extra.as_extra()
        ));
    }
    let found = &h.driver.session.bots.web_apps.found;
    assert_eq!(found.len(), 2, "neither answer overwrote the other");
    assert_eq!(found[0].0, second);
    assert_eq!(found[1].0, first);
    assert!(found[0].1.as_ref().is_ok_and(|app| app.skip_confirmation));
    assert!(found[1].1.as_ref().is_ok_and(|app| !app.skip_confirmation));
}

/// Round 17 C finding 19: a data send keeps the mini-app poll alive until
/// TDLib answers, so its outcome is reported then, not by a later flow.
#[test]
fn a_data_send_is_due_until_it_answers() {
    let mut h = Harness::ready();
    assert!(!h.driver.session.web_app_answer_due());
    let extra = h.driver.send_web_app_data(42, "Go", "{}").expect("sent");
    assert!(h.driver.session.web_app_answer_due());
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert!(!h.driver.session.web_app_answer_due());
    assert_eq!(
        h.driver.session.bots.web_apps.data_sent,
        vec![crate::state::DataSent {
            request: extra,
            result: Ok(())
        }]
    );
}

/// Two data sends close together: each outcome names its own request, so
/// the flow that sent the second never reports the first's answer.
#[test]
fn data_send_outcomes_are_kept_per_request() {
    let mut h = Harness::ready();
    let first = h.driver.send_web_app_data(42, "Go", "{}").expect("sent");
    let second = h.driver.send_web_app_data(43, "Go", "{}").expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
        first.as_extra()
    ));
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        second.as_extra()
    ));
    let sent = &h.driver.session.bots.web_apps.data_sent;
    assert_eq!(sent.len(), 2);
    assert_eq!((sent[0].request, sent[0].result.is_err()), (first, true));
    assert_eq!((sent[1].request, sent[1].result.is_ok()), (second, true));
}

/// A custom-method failure names its request too, so the window that sent
/// it (and no other) turns it into the app's reply.
#[test]
fn a_custom_request_error_names_its_request() {
    let mut h = Harness::ready();
    let extra = h
        .driver
        .send_web_app_custom_request(42, 77, "r9", "getRequestedContact", "{}")
        .expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
        extra.as_extra()
    ));
    let replies = &h.driver.session.bots.web_apps.custom_replies;
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].req_id, "r9");
    assert_eq!(replies[0].launch_id, 77);
    assert_eq!(replies[0].result, Err("BAD_REQUEST".into()));
}

#[test]
fn named_app_links_search_first() {
    let mut h = Harness::ready();
    let extra = h.driver.search_web_app(42, "shop").expect("sent");
    assert_eq!(h.last("searchWebApp")["web_app_short_name"], "shop");
    h.ingest(&format!(
        r#"{{"@type":"foundWebApp","@extra":"{}","web_app":{{"@type":"webApp","short_name":"shop","title":"Shop","description":""}},"request_write_access":true,"skip_confirmation":false}}"#,
        extra.as_extra()
    ));
    let (asked, app) = h.driver.session.bots.web_apps.found[0].clone();
    let app = app.unwrap();
    assert_eq!(asked, extra);
    assert_eq!(app.title, "Shop");
    assert!(app.request_write_access);
    let theme = ThemeParams::light_default();
    h.driver
        .get_web_app_link_url(ChatId(5), 42, ("shop", "ref"), true, &theme, None)
        .expect("sent");
    let sent = h.last("getWebAppLinkUrl");
    assert_eq!(sent["allow_write_access"], true);
    assert_eq!(sent["start_parameter"], "ref");
    assert_eq!(
        h.driver
            .session
            .bots
            .web_apps
            .opening()
            .as_ref()
            .map(|p| p.source.clone()),
        Some(LaunchSource::Link {
            short_name: "shop".into()
        })
    );
    assert_eq!(
        h.driver.close_web_app(0).unwrap(),
        None,
        "link apps have no launch id"
    );
    assert!(h.driver.close_web_app(9).unwrap().is_some());
    assert_eq!(h.last("closeWebApp")["web_app_launch_id"], 9);
}

#[test]
fn attachment_menu_bot_icons_download_and_resolve() {
    let mut h = Harness::ready();
    h.ingest(
        r#"{"@type":"updateAttachmentMenuBots","bots":[{"@type":"attachmentMenuBot","bot_user_id":42,"name":"Shop","supports_user_chats":true,"is_added":true,"show_in_attachment_menu":true,"default_icon":{"@type":"file","id":77,"size":900,"expected_size":900,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":false},"remote":{"@type":"remoteFile","id":"x","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":900}}},{"@type":"attachmentMenuBot","bot_user_id":43,"name":"Side","is_added":true,"show_in_attachment_menu":false,"default_icon":{"@type":"file","id":78,"local":{"can_be_downloaded":true}}}]}"#,
    );
    let sent = h.last("downloadFile");
    assert_eq!(sent["file_id"], 77, "only the attach menu bot's icon");
    assert!(h.driver.session.attach_bot_icon_path(42).is_none());
    h.ingest(
        r#"{"@type":"updateFile","file":{"@type":"file","id":77,"size":900,"expected_size":900,"local":{"@type":"localFile","path":"/tmp/shop.svg","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":true},"remote":{"@type":"remoteFile","id":"x","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":900}}}"#,
    );
    assert_eq!(
        h.driver.session.attach_bot_icon_path(42),
        Some("/tmp/shop.svg")
    );
    assert!(h.driver.session.attach_bot_icons_to_download().is_empty());
}

#[test]
fn attachment_menu_launch_sends_the_chat_context() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    h.driver
        .open_web_app(
            ChatId(5),
            42,
            "",
            LaunchSource::AttachmentMenu,
            &theme,
            None,
        )
        .expect("sent");
    let sent = h.last("openWebApp");
    assert_eq!(sent["chat_id"], 5);
    assert_eq!(sent["bot_user_id"], 42);
    assert_eq!(sent["url"], "");
    h.driver
        .toggle_bot_in_attachment_menu(42, false, false)
        .expect("sent");
    let sent = h.last("toggleBotIsAddedToAttachmentMenu");
    assert_eq!(sent["is_added"], false);
    assert_eq!(sent["allow_write_access"], false);
}

fn my_contact() -> crate::telegram::requests::ContactShare {
    crate::telegram::requests::ContactShare {
        phone_number: "+15550100".into(),
        first_name: "Ada".into(),
        last_name: "L".into(),
        user_id: 7,
    }
}

#[test]
fn sharing_the_phone_sends_my_contact_card_to_the_bot() {
    let mut h = Harness::ready();
    let extra = h
        .driver
        .share_phone_with_bot(42, &my_contact())
        .expect("sent");
    let sent = h.last("sendMessage");
    assert_eq!(sent["chat_id"], 42);
    let content = &sent["input_message_content"];
    assert_eq!(content["@type"], "inputMessageContact");
    assert_eq!(content["contact"]["phone_number"], "+15550100");
    assert_eq!(content["contact"]["user_id"], 7);
    assert!(h.driver.session.bots.web_apps.phone_share.is_empty());
    h.ingest(&format!(
        r#"{{"@type":"message","@extra":"{}","id":9,"chat_id":42,"is_outgoing":true,"date":1700000000,"content":{{"@type":"messageContact","contact":{{"@type":"contact","phone_number":"+15550100","first_name":"Ada","last_name":"L","user_id":7}}}}}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.phone_share,
        [crate::state::PhoneShare::Sent(extra)],
        "the answer names the send it answers"
    );
}

#[test]
fn a_refused_phone_share_reports_failure() {
    let mut h = Harness::ready();
    let extra = h
        .driver
        .share_phone_with_bot(42, &my_contact())
        .expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":403,"message":"Bot was blocked by the user"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.phone_share,
        [crate::state::PhoneShare::Refused(extra)]
    );
}

#[test]
fn a_phone_share_answer_nobody_waits_on_is_dropped_by_the_next_take() {
    let mut h = Harness::ready();
    let late = h
        .driver
        .share_phone_with_bot(42, &my_contact())
        .expect("sent");
    let current = h
        .driver
        .share_phone_with_bot(42, &my_contact())
        .expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":403,"message":"Bot was blocked by the user"}}"#,
        late.as_extra()
    ));
    let apps = &mut h.driver.session.bots.web_apps;
    assert_eq!(apps.take_phone_share(None), None, "no flow waits");
    assert!(apps.phone_share.is_empty(), "the unowned answer is gone");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":403,"message":"Bot was blocked by the user"}}"#,
        late.as_extra()
    ));
    let apps = &mut h.driver.session.bots.web_apps;
    assert_eq!(apps.take_phone_share(Some(current)), None);
    assert!(
        apps.phone_share.is_empty(),
        "an answer for another send is dropped, not kept"
    );
}

#[test]
fn a_phone_share_without_a_number_is_not_sent() {
    let mut h = Harness::ready();
    let mut contact = my_contact();
    contact.phone_number = " ".into();
    assert!(h.driver.share_phone_with_bot(42, &contact).is_err());
}

/// A launch pressed before an account switch: its answer, landing while
/// the account is in the background, opens nothing then or later, and
/// the app TDLib started is closed.
#[test]
fn a_launch_dropped_by_a_switch_opens_nothing_and_is_closed() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .open_web_app(
            ChatId(5),
            42,
            "https://bot.example/app",
            LaunchSource::InlineButton,
            &theme,
            None,
        )
        .expect("sent");
    h.driver.forget_web_app_launches();
    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"777","url":{{"@type":"webAppUrl","url":"https://bot.example/app?x","require_same_origin":false}}}}"#,
        extra.as_extra()
    ));
    assert_eq!(h.driver.session.bots.web_apps.open_result().cloned(), None);
    assert_eq!(h.last("closeWebApp")["web_app_launch_id"], 777);
}

/// An answer that landed before the switch, not yet shown, is closed too.
#[test]
fn an_unshown_answer_is_closed_on_a_switch() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .open_web_app(ChatId(5), 42, "", LaunchSource::InlineButton, &theme, None)
        .expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"778","url":{{"@type":"webAppUrl","url":"https://bot.example/app?x","require_same_origin":false}}}}"#,
        extra.as_extra()
    ));
    h.driver.forget_web_app_launches();
    assert_eq!(h.driver.session.bots.web_apps.open_result().cloned(), None);
    assert_eq!(h.last("closeWebApp")["web_app_launch_id"], 778);
}

/// A second launch while the first answer is unshown must not leave the
/// first app running at the bot: its launch is queued for `closeWebApp`.
#[test]
fn a_new_launch_closes_the_app_of_an_unshown_answer() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let first = h
        .driver
        .open_web_app(ChatId(5), 42, "", LaunchSource::InlineButton, &theme, None)
        .expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"779","url":{{"@type":"webAppUrl","url":"https://bot.example/a","require_same_origin":false}}}}"#,
        first.as_extra()
    ));
    assert!(matches!(
        h.driver.session.bots.web_apps.open_result().cloned(),
        Some(WebAppOpenResult::Opened { launch_id: 779, .. })
    ));
    h.driver
        .open_web_app(ChatId(5), 43, "", LaunchSource::InlineButton, &theme, None)
        .expect("sent");
    assert_eq!(h.driver.session.bots.web_apps.open_result().cloned(), None);
    assert_eq!(h.driver.session.bots.web_apps.dropped_launches, vec![779]);
}

/// A switch flip whose request never left has no error answer coming: it
/// goes back at once.
#[test]
fn emoji_status_access_is_put_back_when_the_send_fails() {
    use super::support::toggle_driver;
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.bots.bot_info.insert(
        42,
        Some(crate::telegram::envelope::BotInfo {
            can_manage_emoji_status: true,
            ..Default::default()
        }),
    );
    sender.refuse(true);
    assert!(driver.set_bot_emoji_status_access(42, false).is_err());
    let value = driver
        .session
        .bots
        .bot_info
        .get(&42)
        .and_then(|info| info.as_ref())
        .map(|info| info.can_manage_emoji_status);
    assert_eq!(value, Some(true));
    assert!(
        !sender
            .snapshot()
            .iter()
            .any(|json| json.contains("toggleBotCanManageEmojiStatus"))
    );
}

#[test]
fn closing_a_mini_app_drops_its_custom_requests_and_answers() {
    use crate::state::{CustomRequestReply, WebApps};
    let mut apps = WebApps::default();
    apps.custom_requests.insert(RequestId(1), (10, "a".into()));
    apps.custom_requests.insert(RequestId(2), (20, "b".into()));
    apps.custom_replies.push(CustomRequestReply {
        launch_id: 10,
        req_id: "x".into(),
        result: Ok("{}".into()),
    });
    apps.custom_replies.push(CustomRequestReply {
        launch_id: 20,
        req_id: "y".into(),
        result: Ok("{}".into()),
    });
    apps.forget_custom_requests_of(10);
    assert_eq!(apps.custom_requests.len(), 1);
    assert!(apps.custom_requests.contains_key(&RequestId(2)));
    assert_eq!(apps.custom_replies.len(), 1);
    assert_eq!(apps.custom_replies[0].launch_id, 20);
}
