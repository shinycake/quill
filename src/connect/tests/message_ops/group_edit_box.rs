//! Driver tests: the group Edit box's save pipeline, its error map, and the
//! basic group description.
use super::*;
use crate::group_info_edit::GroupInfoEdit;
use crate::profile_photo_sources::{ChatPhotoInput, PhotoSticker};
use crate::state::GroupInfoField;

type Fixture = (
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    AtomicU64,
    Arc<dyn DiagnosticSink>,
    std::path::PathBuf,
);

fn basic_group_driver() -> Fixture {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"unread_count":0}}"#,
        r#"{"@type":"updateBasicGroup","basic_group":{"@type":"basicGroup","id":3,"member_count":3,"status":{"@type":"chatMemberStatusCreator","is_member":true},"is_active":true}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    (driver, recorder, seq, dyn_sink, dir)
}

fn sent_types(recorder: &RecordingSender) -> Vec<Value> {
    recorder
        .snapshot()
        .iter()
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .collect()
}

#[test]
fn save_sends_title_description_then_photo() {
    let (mut driver, recorder, _seq, _sink, dir) = basic_group_driver();
    let before = recorder.snapshot().len();
    let edits = [
        GroupInfoEdit::Title("Hikers".into()),
        GroupInfoEdit::Description("Weekend hikes".into()),
        GroupInfoEdit::Photo(ChatPhotoInput::Static {
            path: "/tmp/group.png".into(),
        }),
    ];
    let sent = driver.save_group_info(ChatId(9), &edits).unwrap();
    assert_eq!(sent.text.len(), 2);
    assert!(sent.photo.is_some());
    let requests = sent_types(&recorder);
    let new = requests.get(before..).unwrap();
    assert_eq!(new.len(), 3);
    let at = |i: usize| new.get(i).cloned().unwrap_or_default();
    assert_eq!(at(0)["@type"], "setChatTitle");
    assert_eq!(at(0)["chat_id"], 9);
    assert_eq!(at(0)["title"], "Hikers");
    assert_eq!(at(1)["@type"], "setChatDescription");
    assert_eq!(at(1)["description"], "Weekend hikes");
    assert_eq!(at(2)["@type"], "setChatPhoto");
    assert_eq!(at(2)["photo"]["@type"], "inputChatPhotoStatic");
    assert_eq!(at(2)["photo"]["photo"]["@type"], "inputFileLocal");
    assert_eq!(at(2)["photo"]["photo"]["path"], "/tmp/group.png");
    // The same edit again while the first is in flight is refused.
    assert!(
        driver
            .save_group_info(ChatId(9), &[GroupInfoEdit::Title("Again".into())])
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_emoji_photo_is_an_input_chat_photo_sticker() {
    let (mut driver, recorder, _seq, _sink, dir) = basic_group_driver();
    let photo = ChatPhotoInput::Sticker {
        sticker: PhotoSticker::CustomEmoji {
            custom_emoji_id: 42,
        },
        colors: vec![0x112233, 0x445566],
    };
    driver
        .set_chat_photo(ChatId(9), Some(&photo))
        .unwrap()
        .unwrap();
    let last = sent_types(&recorder).pop().unwrap();
    assert_eq!(last["@type"], "setChatPhoto");
    assert_eq!(last["photo"]["@type"], "inputChatPhotoSticker");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_title_is_recorded_for_the_box() {
    let (mut driver, _recorder, seq, sink, dir) = basic_group_driver();
    let sent = driver
        .save_group_info(ChatId(9), &[GroupInfoEdit::Title("Hikers".into())])
        .unwrap();
    let id = *sent.text.first().unwrap();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_TITLE_EMPTY"}}"#,
                    id.0
                ),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.requests.get(id).is_none());
    let error = driver.session.groups.info_edit_errors.get(&9).unwrap();
    assert_eq!(error.field, GroupInfoField::Title);
    // The next save starts clean.
    driver
        .save_group_info(ChatId(9), &[GroupInfoEdit::Title("Hikers".into())])
        .unwrap();
    assert!(!driver.session.groups.info_edit_errors.contains_key(&9));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_member_without_the_permission_cannot_edit() {
    let (mut driver, _recorder, seq, sink, dir) = basic_group_driver();
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateBasicGroup","basic_group":{"@type":"basicGroup","id":3,"member_count":3,"status":{"@type":"chatMemberStatusMember"},"is_active":true}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(!driver.session.chat_can_edit_information(ChatId(9)));
    assert!(
        driver
            .save_group_info(ChatId(9), &[GroupInfoEdit::Title("Hikers".into())])
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_basic_group_description_is_kept() {
    let (mut driver, _recorder, seq, sink, dir) = basic_group_driver();
    let extra = driver
        .fetch_basic_group_members(ChatId(9))
        .unwrap()
        .expect("request sent");
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"basicGroupFullInfo","@extra":"{}","description":"Book club","members":[]}}"#,
                    extra.0
                ),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        driver.session.chat_description(ChatId(9)),
        Some("Book club")
    );
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateBasicGroupFullInfo","basic_group_id":3,"basic_group_full_info":{"description":"Two books a month","members":[]}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        driver.session.chat_description(ChatId(9)),
        Some("Two books a month")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
