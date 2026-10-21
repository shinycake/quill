//! Interface language packs: the switch flow, string updates and the saved
//! choice at startup (docs/decisions/language-packs.md).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::lang::PackValue;
use crate::lang::prefs::{LanguageChoice, save_choice};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::LanguagePackInfoData;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
    /// Requests the ready driver sent before the test began.
    skip: usize,
}

impl Fixture {
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &sink, &seq);
        let skip = recorder.snapshot().len();
        Self {
            driver,
            recorder,
            sink,
            seq,
            dir,
            skip,
        }
    }

    fn feed(&mut self, value: Value) {
        self.driver
            .ingest(copy_and_parse(&value.to_string(), &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .skip(self.skip)
            .map(|j| serde_json::from_str(j).unwrap())
            .collect()
    }

    /// The `@extra` of the last sent `getLanguagePackStrings` for `pack`.
    fn strings_extra(&self, pack: &str) -> Value {
        self.sent()
            .into_iter()
            .rev()
            .find(|v| v["@type"] == "getLanguagePackStrings" && v["language_pack_id"] == pack)
            .map(|v| v["@extra"].clone())
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn pack(id: &str, base: &str) -> LanguagePackInfoData {
    LanguagePackInfoData {
        id: id.into(),
        base_language_pack_id: base.into(),
        name: "Portuguese (Brazil)".into(),
        native_name: "Português (Brasil)".into(),
        plural_code: "pt".into(),
        is_official: true,
        is_rtl: false,
        is_beta: false,
        is_installed: false,
        total_string_count: 0,
        translated_string_count: 0,
    }
}

fn strings(extra: &Value, key: &str, text: &str) -> Value {
    json!({"@type":"languagePackStrings","@extra":extra,"strings":[
        {"@type":"languagePackString","key":key,
         "value":{"@type":"languagePackStringValueOrdinary","value":text}}]})
}

#[test]
fn switch_selects_fetches_pack_and_base_then_synchronizes() {
    let mut f = Fixture::new();
    f.driver.switch_language(pack("pt-br", "pt")).unwrap();
    let types: Vec<String> = f
        .sent()
        .iter()
        .map(|v| v["@type"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        types,
        [
            "setOption",
            "getLanguagePackStrings",
            "getLanguagePackStrings",
            "synchronizeLanguagePack"
        ]
    );
    assert_eq!(f.sent()[0]["name"], "language_pack_id");
    assert_eq!(f.sent()[0]["value"]["value"], "pt-br");
    let pack_extra = f.strings_extra("pt-br");
    let base_extra = f.strings_extra("pt");
    f.feed(strings(&pack_extra, "lng_languages", "Idioma"));
    assert!(f.driver.session.settings.language.ready.is_none());
    f.feed(strings(&base_extra, "lng_box_ok", "OK"));
    let ready = f.driver.session.settings.language.ready.take().unwrap();
    assert_eq!(ready.info.id, "pt-br");
    assert_eq!(
        ready.pack,
        vec![("lng_languages".into(), PackValue::Ordinary("Idioma".into()))]
    );
    assert_eq!(ready.base.len(), 1);
    assert_eq!(
        f.driver
            .session
            .settings
            .language
            .current
            .as_ref()
            .map(|c| c.id.as_str()),
        Some("pt-br")
    );
}

#[test]
fn a_newer_switch_drops_the_older_answers() {
    let mut f = Fixture::new();
    f.driver.switch_language(pack("pt-br", "")).unwrap();
    let old = f.strings_extra("pt-br");
    let mut de = pack("de", "");
    de.plural_code = "de".into();
    f.driver.switch_language(de).unwrap();
    f.feed(strings(&old, "lng_languages", "Idioma"));
    assert!(f.driver.session.settings.language.ready.is_none());
    let new = f.strings_extra("de");
    f.feed(strings(&new, "lng_languages", "Sprache"));
    assert_eq!(
        f.driver
            .session
            .settings
            .language
            .ready
            .as_ref()
            .map(|r| r.info.id.as_str()),
        Some("de")
    );
}

#[test]
fn unofficial_packs_are_added_first_and_english_needs_no_strings() {
    let mut f = Fixture::new();
    let mut custom = pack("x-quill", "");
    custom.is_official = false;
    f.driver.switch_language(custom).unwrap();
    assert_eq!(f.sent()[0]["@type"], "addCustomServerLanguagePack");

    let mut f = Fixture::new();
    f.driver.switch_language(pack("en", "")).unwrap();
    let sent = f.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["value"]["value"], "en");
    let ready = f.driver.session.settings.language.ready.take().unwrap();
    assert!(ready.pack.is_empty());
    assert!(f.driver.session.settings.language.current.is_none());
}

#[test]
fn string_updates_patch_the_current_pack_or_ask_for_a_reload() {
    let mut f = Fixture::new();
    f.driver.switch_language(pack("pt-br", "")).unwrap();
    let extra = f.strings_extra("pt-br");
    f.feed(strings(&extra, "lng_languages", "Idioma"));
    f.feed(
        json!({"@type":"updateLanguagePackStrings","localization_target":"android",
        "language_pack_id":"pt-br","strings":[]}),
    );
    assert!(!f.driver.session.settings.language.reload);
    f.feed(
        json!({"@type":"updateLanguagePackStrings","localization_target":"tdesktop",
        "language_pack_id":"pt-br","strings":[{"@type":"languagePackString","key":"lng_box_ok",
        "value":{"@type":"languagePackStringValueOrdinary","value":"Certo"}}]}),
    );
    assert_eq!(
        f.driver.session.settings.language.patch,
        vec![("lng_box_ok".into(), PackValue::Ordinary("Certo".into()))]
    );
    assert!(!f.driver.session.settings.language.reload);
    // A deleted string falls back to the base pack: fetch again.
    f.feed(
        json!({"@type":"updateLanguagePackStrings","localization_target":"tdesktop",
        "language_pack_id":"pt-br","strings":[{"@type":"languagePackString","key":"lng_box_ok",
        "value":{"@type":"languagePackStringValueDeleted"}}]}),
    );
    assert!(f.driver.session.settings.language.reload);
    f.driver.session.settings.language.reload = false;
    f.feed(
        json!({"@type":"updateLanguagePackStrings","localization_target":"tdesktop",
        "language_pack_id":"pt-br","strings":[]}),
    );
    assert!(f.driver.session.settings.language.reload);
}

#[test]
fn a_finished_switch_drops_changes_queued_for_the_old_pack() {
    let mut f = Fixture::new();
    f.driver.switch_language(pack("pt-br", "")).unwrap();
    let extra = f.strings_extra("pt-br");
    f.feed(strings(&extra, "lng_languages", "Idioma"));
    f.driver.session.settings.language.ready = None;
    f.driver.switch_language(pack("de", "")).unwrap();
    f.feed(
        json!({"@type":"updateLanguagePackStrings","localization_target":"tdesktop",
        "language_pack_id":"pt-br","strings":[{"@type":"languagePackString","key":"lng_box_ok",
        "value":{"@type":"languagePackStringValueOrdinary","value":"Certo"}}]}),
    );
    assert_eq!(f.driver.session.settings.language.patch.len(), 1);
    let extra = f.strings_extra("de");
    f.feed(strings(&extra, "lng_languages", "Sprache"));
    let state = &f.driver.session.settings.language;
    assert!(state.ready.is_some() && state.patch.is_empty());
}

#[test]
fn failed_strings_end_the_switch_with_an_error() {
    let mut f = Fixture::new();
    f.driver.switch_language(pack("pt-br", "")).unwrap();
    let extra = f.strings_extra("pt-br");
    f.feed(json!({"@type":"error","@extra":extra,"code":400,"message":"LANG_PACK_INVALID"}));
    let state = &f.driver.session.settings.language;
    assert!(state.switching.is_none() && state.ready.is_none());
    assert!(state.error.is_some());
    // TDLib goes back to the pack the interface still shows.
    let before = f.sent().len();
    f.driver.restore_language_option().unwrap();
    let sent = f.sent();
    assert_eq!(sent.len(), before + 1);
    assert_eq!(sent.last().unwrap()["value"]["value"], "en");
}

#[test]
fn startup_points_tdlib_at_tdesktop_strings_and_restores_the_saved_pack() {
    let mut f = Fixture::new();
    f.driver.send_language_options().unwrap();
    let sent = f.sent();
    assert_eq!(sent.len(), 2, "English: options only");
    assert_eq!(sent[0]["value"]["value"], "tdesktop");
    assert!(
        sent[1]["value"]["value"]
            .as_str()
            .unwrap()
            .ends_with("langpack.sqlite")
    );

    let mut f = Fixture::new();
    let choice = LanguageChoice {
        pack_id: "de".into(),
        plural_code: "de".into(),
        name: "German".into(),
        native_name: "Deutsch".into(),
        ..Default::default()
    };
    save_choice(&f.dir, &choice).unwrap();
    f.driver.send_language_options().unwrap();
    assert!(f.sent().iter().any(|v| v["@type"] == "setOption"
        && v["name"] == "language_pack_id"
        && v["value"]["value"] == "de"));
    let extra = f.strings_extra("de");
    f.feed(strings(&extra, "lng_languages", "Sprache"));
    assert_eq!(
        f.driver
            .session
            .settings
            .language
            .ready
            .as_ref()
            .map(|r| r.info.native_name.as_str()),
        Some("Deutsch")
    );
}

#[test]
fn the_pack_list_comes_from_the_localization_target() {
    let mut f = Fixture::new();
    let extra = f.driver.request_language_packs().unwrap();
    assert!(f.driver.session.settings.language.packs_loading);
    assert_eq!(f.sent()[0]["@type"], "getLocalizationTargetInfo");
    f.feed(
        json!({"@type":"localizationTargetInfo","@extra":extra.as_extra(),"language_packs":[
        {"@type":"languagePackInfo","id":"en","name":"English","native_name":"English",
         "plural_code":"en","is_official":true}]}),
    );
    let state = &f.driver.session.settings.language;
    assert!(!state.packs_loading);
    assert_eq!(state.packs.len(), 1);
}

fn position(chat: i64, source: Value) -> Value {
    json!({"@type":"updateChatPosition","chat_id":chat,"position":{
        "@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9",
        "is_pinned":false,"source":source}})
}

#[test]
fn announcement_labels_are_fetched_once_and_hide_deletes_the_history() {
    let mut f = Fixture::new();
    f.feed(position(
        7,
        json!({"@type":"chatSourcePublicServiceAnnouncement","type":"covid","text":"Hi"}),
    ));
    f.feed(position(8, json!({"@type":"chatSourceMtprotoProxy"})));
    // Both are channels the account is not in (`useTopPromotion`).
    for id in [7, 8] {
        if let Some(chat) = f.driver.session.chats.get_mut(&id) {
            chat.kind = crate::telegram::envelope::ChatKind::Supergroup {
                supergroup_id: id,
                is_channel: true,
            };
        }
    }
    f.driver.pump_psa_labels();
    let sent = f.sent();
    let ask = sent
        .iter()
        .find(|v| v["@type"] == "getLanguagePackStrings")
        .unwrap();
    assert_eq!(ask["language_pack_id"], "en");
    assert_eq!(
        ask["keys"],
        json!(["cloud_lng_badge_psa_covid", "cloud_lng_about_psa_covid"])
    );
    f.feed(strings(
        &ask["@extra"],
        "cloud_lng_badge_psa_covid",
        "COVID-19 Info",
    ));
    let label = |f: &Fixture, chat: i64| {
        let list = &f.driver.session.chat_list;
        list.psa_labels.row(&list.promoted_chats[&chat]).label
    };
    assert_eq!(label(&f, 7), "COVID-19 Info");
    assert_eq!(label(&f, 8), "Proxy sponsor");
    let before = f.sent().len();
    f.driver.pump_psa_labels();
    assert_eq!(f.sent().len(), before);

    assert!(f.driver.hide_promotion(ChatId(8)).unwrap().is_none());
    assert!(f.driver.hide_promotion(ChatId(7)).unwrap().is_some());
    let sent = f.sent();
    let hide = sent
        .iter()
        .find(|v| v["@type"] == "deleteChatHistory")
        .unwrap();
    assert_eq!(hide["chat_id"], 7);
    assert_eq!(hide["remove_from_chat_list"], true);

    // A joined channel is not hidden, and a language switch asks again.
    if let Some(chat) = f.driver.session.chats.get_mut(&7) {
        chat.my_member_status = Some(crate::telegram::envelope::ChannelMemberStatus::Member);
    }
    let sent_before = f.sent().len();
    assert!(f.driver.hide_promotion(ChatId(7)).unwrap().is_none());
    assert_eq!(f.sent().len(), sent_before);
    if let Some(chat) = f.driver.session.chats.get_mut(&7) {
        chat.my_member_status = Some(crate::telegram::envelope::ChannelMemberStatus::Left);
    }
    f.driver.switch_language(pack("pt-br", "pt")).unwrap();
    f.driver.pump_psa_labels();
    let sent = f.sent();
    let again = sent
        .iter()
        .rev()
        .find(|v| {
            v["@type"] == "getLanguagePackStrings"
                && v["keys"] == json!(["cloud_lng_badge_psa_covid", "cloud_lng_about_psa_covid"])
        })
        .unwrap();
    assert_eq!(again["language_pack_id"], "pt-br");
    assert_eq!(
        f.driver
            .session
            .chat_list
            .psa_labels
            .row(&f.driver.session.chat_list.promoted_chats[&7])
            .label,
        "PSA",
        "the English text is dropped until the new pack answers"
    );
}

#[test]
fn the_about_box_waits_for_the_saved_flags_and_saves_without_overwriting() {
    use crate::psa_about::{load_seen_from, merge_save};
    use std::collections::BTreeSet;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("quill-psa-driver-{}", std::process::id()));
    let file = dir.join("psa_about_seen.json");
    // Another session already saved chat 9.
    merge_save(&file, &BTreeSet::from([9]));

    let mut f = Fixture::new();
    f.feed(position(
        7,
        json!({"@type":"chatSourcePublicServiceAnnouncement","type":"covid","text":"Hi"}),
    ));
    if let Some(chat) = f.driver.session.chats.get_mut(&7) {
        chat.kind = crate::telegram::envelope::ChatKind::Supergroup {
            supergroup_id: 7,
            is_channel: true,
        };
    }
    f.driver.session.chat_list.psa_about.use_file(file.clone());
    f.driver.session.open_chat(ChatId(7));
    let shown = |f: &Fixture| {
        f.driver
            .session
            .chat_list
            .psa_about
            .visible(f.driver.session.open_chat, Instant::now())
            .is_some()
    };
    assert!(!shown(&f), "the saved flags are not in yet");

    let mut tries = 0;
    while !shown(&f) && tries < 200 {
        f.driver.sync_psa_about();
        std::thread::sleep(Duration::from_millis(10));
        tries += 1;
    }
    assert!(
        shown(&f),
        "unseen chat: the box shows once the flags are in"
    );

    let mut tries = 0;
    while !load_seen_from(&file).contains(&7) && tries < 200 {
        f.driver.sync_psa_about();
        std::thread::sleep(Duration::from_millis(10));
        tries += 1;
    }
    assert_eq!(
        load_seen_from(&file),
        BTreeSet::from([7, 9]),
        "saved once, merged with the other session's flag"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
