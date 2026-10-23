//! Connect-driver tests: the Location picker's venue bot, venues, tiles,
//! icons and sends, against the recording sender.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::location_picker::{GeoOutcome, GeoPoint, MapSize, VenueList};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut fixture = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            dir,
        };
        fixture.ingest(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        fixture.ingest(r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#);
        fixture.ingest(r#"{"@type":"updateOption","name":"venue_search_bot_username","value":{"@type":"optionValueString","value":"foursquare"}}"#);
        fixture.driver.select_chat(ChatId(7)).unwrap();
        fixture
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, kind: &str) -> Vec<Value> {
        let tag = format!("\"@type\":\"{kind}\"");
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains(&tag))
            .map(|json| serde_json::from_str(&json).unwrap())
            .collect()
    }

    fn last(&self, kind: &str) -> Value {
        self.sent(kind).pop().unwrap_or_else(|| panic!("no {kind}"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn extra(request: &Value) -> &str {
    request["@extra"].as_str().expect("extra")
}

fn file_answer(extra: &str, id: i32) -> String {
    format!(
        r#"{{"@type":"file","@extra":"{extra}","id":{id},"size":0,"expected_size":9000,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"m{id}","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":0}}}}"#
    )
}

const BOT_USER: &str = r#"{"@type":"updateUser","user":{"id":55,"first_name":"Foursquare","usernames":{"@type":"usernames","active_usernames":["foursquare"],"disabled_usernames":[],"editable_username":"foursquare","collectible_usernames":[]},"type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":true,"inline_query_placeholder":"Search places","supports_guest_queries":false,"is_guard":false,"need_location":true,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#;

fn here() -> GeoPoint {
    GeoPoint::new(48.8566, 2.3522).expect("finite")
}

#[test]
fn a_fix_finds_the_venue_bot_then_venues_then_their_icons() {
    let mut f = Fixture::new();
    f.driver.open_location_picker(ChatId(7), None).unwrap();
    assert!(
        f.sent("searchPublicChat").is_empty(),
        "no place to look yet"
    );
    f.driver
        .location_picker_geolocated(GeoOutcome::Found(here()))
        .unwrap();
    let lookup = f.last("searchPublicChat");
    assert_eq!(lookup["username"], "foursquare");

    f.ingest(BOT_USER);
    let chat = format!(
        r#"{{"@type":"chat","@extra":"{}","id":55,"title":"Foursquare","type":{{"@type":"chatTypePrivate","user_id":55}},"unread_count":0}}"#,
        extra(&lookup)
    );
    f.ingest(&chat);
    let query = f.last("getInlineQueryResults");
    assert_eq!(query["bot_user_id"], 55);
    assert_eq!(query["chat_id"], 7);
    assert_eq!(query["query"], "");
    assert_eq!(query["user_location"]["latitude"], 48.8566);

    let answer = format!(
        r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":"1","results":[{{"@type":"inlineQueryResultVenue","id":"r1","venue":{{"@type":"venue","location":{{"@type":"location","latitude":48.857,"longitude":2.353,"horizontal_accuracy":0}},"title":"Cafe","address":"Rue de Rivoli","provider":"foursquare","id":"v1","type":"food/cafe"}}}}],"next_offset":""}}"#,
        extra(&query)
    );
    f.ingest(&answer);
    let picker = f.driver.session.location.picker().expect("open");
    let VenueList::Found(venues) = picker.venues() else {
        panic!("venues: {:?}", picker.venues());
    };
    assert_eq!(venues[0].title, "Cafe");
    let icon = f.last("getRemoteFile");
    assert_eq!(
        icon["remote_file_id"],
        "https://ss3.4sqi.net/img/categories_v2/food/cafe_64.png"
    );
    f.ingest(&file_answer(extra(&icon), 91));
    assert!(
        f.sent("downloadFile")
            .iter()
            .any(|request| request["file_id"] == 91)
    );
    assert_eq!(
        f.driver.session.location.icon_file("food/cafe"),
        Some(crate::ids::FileId(91))
    );
}

#[test]
fn a_resting_map_asks_for_its_tile_once_and_downloads_it() {
    let mut f = Fixture::new();
    f.driver.open_location_picker(ChatId(7), None).unwrap();
    if let Some(picker) = f.driver.session.location.picker_mut() {
        picker.set_map_size(MapSize::new(364., 220., 2.));
    }
    f.driver.pump_location_picker().unwrap();
    f.driver.pump_location_picker().unwrap();
    let tiles = f.sent("getMapThumbnailFile");
    assert_eq!(tiles.len(), 1, "asked once");
    assert_eq!(tiles[0]["width"], 364);
    assert_eq!(tiles[0]["height"], 220);
    assert_eq!(tiles[0]["scale"], 2);
    assert_eq!(tiles[0]["chat_id"], 7);
    f.ingest(&file_answer(extra(&tiles[0]), 92));
    assert!(
        f.sent("downloadFile")
            .iter()
            .any(|request| request["file_id"] == 92)
    );

    // Dragging asks for nothing until the map rests again.
    if let Some(picker) = f.driver.session.location.picker_mut() {
        let moved = picker.view().panned(150., 0.);
        picker.moved_to(moved);
    }
    f.driver.pump_location_picker().unwrap();
    assert_eq!(f.sent("getMapThumbnailFile").len(), 1);
    if let Some(picker) = f.driver.session.location.picker_mut() {
        picker.move_ended();
    }
    f.driver.pump_location_picker().unwrap();
    assert_eq!(f.sent("getMapThumbnailFile").len(), 2);
}

#[test]
fn without_a_venue_bot_the_list_says_nothing_was_found() {
    let mut f = Fixture::new();
    f.driver.open_location_picker(ChatId(7), None).unwrap();
    f.driver
        .location_picker_geolocated(GeoOutcome::Found(here()))
        .unwrap();
    let lookup = f.last("searchPublicChat");
    f.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"USERNAME_NOT_OCCUPIED"}}"#,
        extra(&lookup)
    ));
    assert!(f.sent("getInlineQueryResults").is_empty());
    assert!(matches!(
        f.driver
            .session
            .location
            .picker()
            .map(|p| p.venues().clone()),
        Some(VenueList::NothingFound { .. })
    ));
}

#[test]
fn venues_and_the_pin_send_as_venue_and_location_messages() {
    let mut f = Fixture::new();
    let options = crate::composer::SendOptions::default();
    let venue = crate::location_picker::Venue {
        point: here(),
        title: "Cafe".into(),
        address: "Rue de Rivoli".into(),
        provider: "foursquare".into(),
        id: "v1".into(),
        venue_type: "food/cafe".into(),
    };
    f.driver
        .share_venue_to_chat(ChatId(7), &venue, None, &options)
        .unwrap();
    let sent = f.last("sendMessage");
    assert_eq!(sent["input_message_content"]["@type"], "inputMessageVenue");
    assert_eq!(sent["input_message_content"]["venue"]["id"], "v1");
    assert!(
        f.driver
            .share_venue_to_chat(ChatId(99), &venue, None, &options)
            .is_err()
    );
    f.driver
        .share_location_to_chat(ChatId(7), 48.8566, 2.3522, None, &options)
        .unwrap();
    let sent = f.last("sendMessage");
    assert_eq!(
        sent["input_message_content"]["@type"],
        "inputMessageLocation"
    );
}
