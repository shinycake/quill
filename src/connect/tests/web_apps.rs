//! Connect-driver tests: mini apps (`connect/web_apps.rs`,
//! `state/session_web_apps.rs`).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::{Session, WebAppOpenResult, WriteAccessResult};
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
            .pending
            .as_ref()
            .map(|p| p.bot_user_id),
        Some(42)
    );

    h.ingest(&format!(
        r#"{{"@type":"webAppInfo","@extra":"{}","launch_id":"777","url":{{"@type":"webAppUrl","url":"https://bot.example/app?tgWebAppData=x","require_same_origin":false}}}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.open_result,
        Some(WebAppOpenResult::Opened {
            bot_user_id: 42,
            chat_id: Some(5),
            source: LaunchSource::InlineButton,
            launch_id: 777,
            url: "https://bot.example/app?tgWebAppData=x".into(),
        })
    );
    assert!(h.driver.session.bots.web_apps.pending.is_none());
}

#[test]
fn a_refused_open_reports_a_failure_and_clears_the_context() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .get_main_web_app(ChatId(5), 42, "promo", &theme)
        .expect("sent");
    assert_eq!(h.last("getMainWebApp")["start_parameter"], "promo");
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":404,"message":"BOT_APP_INVALID"}}"#,
        extra.as_extra()
    ));
    assert!(matches!(
        h.driver.session.bots.web_apps.open_result,
        Some(WebAppOpenResult::Failed {
            bot_user_id: 42,
            ..
        })
    ));
    assert!(h.driver.session.bots.web_apps.pending.is_none());
}

#[test]
fn keyboard_button_apps_use_get_web_app_url_and_may_send_data() {
    let mut h = Harness::ready();
    let theme = ThemeParams::light_default();
    let extra = h
        .driver
        .get_web_app_url(ChatId(5), 42, "https://bot.example/form", "Order", &theme)
        .expect("sent");
    let sent = h.last("getWebAppUrl");
    assert!(sent.get("chat_id").is_none());
    h.ingest(&format!(
        r#"{{"@type":"webAppUrl","@extra":"{}","url":"https://bot.example/form?x","require_same_origin":false}}"#,
        extra.as_extra()
    ));
    let Some(WebAppOpenResult::Opened {
        source, launch_id, ..
    }) = h.driver.session.bots.web_apps.open_result.clone()
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
    assert_eq!(h.driver.session.bots.web_apps.data_sent, Some(Ok(())));
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
        h.driver.session.bots.web_apps.write_access,
        Some((42, WriteAccessResult::NeedsConsent))
    );
    let extra = h.driver.allow_bot_to_send_messages(42).expect("sent");
    assert_eq!(h.last("allowBotToSendMessages")["bot_user_id"], 42);
    assert!(
        h.driver.session.bots.web_apps.write_access.is_none(),
        "taken while in flight"
    );
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.write_access,
        Some((42, WriteAccessResult::Granted))
    );
    let extra = h.driver.can_bot_send_messages(42).expect("sent");
    h.ingest(&format!(
        r#"{{"@type":"ok","@extra":"{}"}}"#,
        extra.as_extra()
    ));
    assert_eq!(
        h.driver.session.bots.web_apps.write_access,
        Some((42, WriteAccessResult::Allowed))
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
    assert!(h.driver.session.bots.web_apps.grossing_loading);
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
        h.driver.session.bots.web_apps.grossing_bots,
        Some(vec![42, 43])
    );
    assert!(!h.driver.session.bots.web_apps.grossing_loading);
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
    let bot = h
        .driver
        .session
        .bots
        .web_apps
        .attachment_menu_bot
        .clone()
        .unwrap()
        .unwrap();
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
        Some(Ok((43, true)))
    );

    let extra = h
        .driver
        .send_web_app_custom_request(42, "r9", "getRequestedContact", "{}")
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
    assert_eq!(replies[0].result, Ok("{\"ok\":true}".into()));
    assert!(h.driver.session.bots.web_apps.custom_requests.is_empty());
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
    let (bot_id, app) = h
        .driver
        .session
        .bots
        .web_apps
        .found
        .clone()
        .unwrap()
        .unwrap();
    assert_eq!(bot_id, 42);
    assert_eq!(app.title, "Shop");
    assert!(app.request_write_access);
    let theme = ThemeParams::light_default();
    h.driver
        .get_web_app_link_url(ChatId(5), 42, "shop", "ref", true, &theme)
        .expect("sent");
    let sent = h.last("getWebAppLinkUrl");
    assert_eq!(sent["allow_write_access"], true);
    assert_eq!(sent["start_parameter"], "ref");
    assert_eq!(
        h.driver
            .session
            .bots
            .web_apps
            .pending
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
