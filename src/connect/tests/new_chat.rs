//! Connect-driver tests: New Group / New Channel. A channel is created,
//! gets its photo and invite link once TDLib answers, then its link and
//! subscribers; a group is created with its members and opened; refusals
//! keep the server's reason.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::group_type::GroupTypeEdit;
use crate::ids::{ChatId, RequestId};
use crate::new_chat::{
    CreateFailure, CreateProgress, CreationOutcome, NewChatKind, NewChatStage, TypeSaveStage,
    plan_new_chat,
};
use crate::platform::MemorySecretStore;
use crate::profile_photo_sources::ChatPhotoInput;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const CHANNEL: i64 = -100_500;
const GROUP: i64 = -77;

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
        let driver = ready_driver(&recorder, prepared, &sink, &seq);
        let mut rig = Self {
            driver,
            recorder,
            seq,
            sink,
            _dir: dir,
        };
        rig.driver.session.my_user_id = Some(1);
        rig
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains(&format!(r#""@type":"{ty}""#)))
            .map(|j| serde_json::from_str(&j).unwrap())
            .collect()
    }

    fn outcome(&self, extra: RequestId) -> Option<CreationOutcome> {
        self.driver
            .session
            .groups
            .new_chats
            .creations
            .get(&extra)
            .and_then(crate::state::NewChatCreation::outcome)
    }

    fn create(
        &mut self,
        kind: NewChatKind,
        users: &[i64],
        photo: Option<ChatPhotoInput>,
    ) -> RequestId {
        let request = plan_new_chat(kind, " Quill Notes ", "Tips", users, Some(1), 86_400).unwrap();
        self.driver
            .create_new_chat(&request, photo)
            .unwrap()
            .unwrap()
    }

    /// TDLib's answer to `createNewSupergroupChat`: the channel, after
    /// the supergroup it is made of.
    fn answer_channel(&mut self, extra: RequestId) {
        self.ingest(&format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":500,"is_channel":true,"status":{CREATOR},"member_count":1}}}}"#
        ));
        self.ingest(&format!(
            r#"{{"@type":"chat","id":{CHANNEL},"title":"Quill Notes","type":{{"@type":"chatTypeSupergroup","supergroup_id":500,"is_channel":true}},"unread_count":0,"@extra":"{}"}}"#,
            extra.0
        ));
    }
}

#[test]
fn a_channel_is_created_then_gets_photo_link_and_subscribers() {
    let mut rig = Rig::new();
    let photo = ChatPhotoInput::Static {
        path: "/tmp/quill-new-chat.jpg".to_string(),
    };
    let extra = rig.create(NewChatKind::Channel, &[5, 6], Some(photo));
    let create = rig.sent("createNewSupergroupChat");
    assert_eq!(create.len(), 1);
    assert_eq!(create[0]["title"], "Quill Notes");
    assert_eq!(create[0]["is_channel"], true);
    assert_eq!(create[0]["description"], "Tips");
    assert_eq!(create[0]["message_auto_delete_time"], 0);
    let again = plan_new_chat(NewChatKind::Channel, "Again", "", &[], None, 0).unwrap();
    assert_eq!(rig.driver.create_new_chat(&again, None).unwrap(), None);
    assert!(rig.sent("setChatPhoto").is_empty());

    rig.answer_channel(extra);
    let chat = ChatId(CHANNEL);
    assert_eq!(rig.outcome(extra), Some(CreationOutcome::Created(chat)));
    let photo = rig.sent("setChatPhoto");
    assert_eq!(photo.len(), 1);
    assert_eq!(photo[0]["chat_id"], CHANNEL);
    assert_eq!(photo[0]["photo"]["@type"], "inputChatPhotoStatic");
    let link = rig.sent("replacePrimaryChatInviteLink");
    assert_eq!(link.len(), 1);
    assert_eq!(link[0]["chat_id"], CHANNEL);
    assert_ne!(rig.driver.session.open_chat_id(), Some(chat));
    let stage = NewChatStage::Creating {
        request: extra,
        back: crate::new_chat::NewChatBack::Info,
        progress: CreateProgress::InFlight,
    };
    assert_eq!(
        rig.driver.session.settled_new_chat(&stage),
        NewChatStage::Creating {
            request: extra,
            back: crate::new_chat::NewChatBack::Info,
            progress: CreateProgress::Created(chat),
        },
        "the answer is taken into the stage"
    );

    let sent = rig
        .driver
        .save_group_type(
            chat,
            &[GroupTypeEdit::Username("quillnotes".to_string())],
            &[],
        )
        .unwrap();
    assert_eq!(sent.len(), 1);
    let username = rig.sent("setSupergroupUsername");
    assert_eq!(username.len(), 1);
    assert_eq!(username[0]["username"], "quillnotes");

    assert!(
        rig.driver
            .add_chat_members(chat, &[5, 6])
            .unwrap()
            .is_some()
    );
    let add = rig.sent("addChatMembers");
    assert_eq!(add.len(), 1);
    assert_eq!(add[0]["user_ids"], serde_json::json!([5, 6]));
}

#[test]
fn a_group_is_created_with_members_and_opened() {
    let mut rig = Rig::new();
    let extra = rig.create(NewChatKind::Group, &[5, 1, 5, 6], None);
    let create = rig.sent("createNewBasicGroupChat");
    assert_eq!(create.len(), 1);
    assert_eq!(create[0]["user_ids"], serde_json::json!([5, 6]));
    assert_eq!(create[0]["title"], "Quill Notes");
    assert_eq!(create[0]["message_auto_delete_time"], 86_400);
    rig.ingest(&format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Quill Notes","type":{{"@type":"chatTypeBasicGroup","basic_group_id":8}},"unread_count":0}}}}"#
    ));
    rig.ingest(&format!(
        r#"{{"@type":"createdBasicGroupChat","chat_id":{GROUP},"failed_to_add_members":{{"@type":"failedToAddMembers","failed_to_add_members":[]}},"@extra":"{}"}}"#,
        extra.0
    ));
    assert_eq!(
        rig.outcome(extra),
        Some(CreationOutcome::Created(ChatId(GROUP)))
    );
    assert_eq!(rig.driver.session.open_chat_id(), Some(ChatId(GROUP)));
    assert!(rig.sent("setChatPhoto").is_empty());
    assert!(rig.sent("replacePrimaryChatInviteLink").is_empty());
}

#[test]
fn a_refused_create_keeps_the_reason_and_sends_nothing_more() {
    let mut rig = Rig::new();
    let photo = ChatPhotoInput::Static {
        path: "/tmp/quill-new-chat.jpg".to_string(),
    };
    let extra = rig.create(NewChatKind::Group, &[5], Some(photo));
    rig.ingest(&format!(
        r#"{{"@type":"error","code":400,"message":"USERS_TOO_FEW","@extra":"{}"}}"#,
        extra.0
    ));
    match rig.outcome(extra) {
        Some(CreationOutcome::Failed { failure, .. }) => {
            assert_eq!(failure, CreateFailure::UsersTooFew);
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    assert!(rig.sent("setChatPhoto").is_empty());
    let retry = plan_new_chat(NewChatKind::Group, "Again", "", &[5], Some(1), 0).unwrap();
    assert!(rig.driver.create_new_chat(&retry, None).unwrap().is_some());
}

#[test]
fn an_invalid_photo_is_refused_before_sending() {
    let mut rig = Rig::new();
    let request = plan_new_chat(NewChatKind::Channel, "Notes", "", &[], None, 0).unwrap();
    let photo = ChatPhotoInput::Static {
        path: String::new(),
    };
    assert!(rig.driver.create_new_chat(&request, Some(photo)).is_err());
    assert!(rig.sent("createNewSupergroupChat").is_empty());
}

/// Round 4, 006: closing the dialog while a create is out does not block
/// the next dialog's Create and does not open the group over what the
/// person is doing.
#[test]
fn an_abandoned_create_neither_blocks_nor_opens_its_chat() {
    let mut rig = Rig::new();
    let extra = rig.create(NewChatKind::Group, &[5], None);
    let again = plan_new_chat(NewChatKind::Group, "Again", "", &[5], Some(1), 0).unwrap();
    assert_eq!(rig.driver.create_new_chat(&again, None).unwrap(), None);
    rig.driver.abandon_new_chat(extra);
    assert!(rig.driver.create_new_chat(&again, None).unwrap().is_some());
    rig.ingest(&format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Quill Notes","type":{{"@type":"chatTypeBasicGroup","basic_group_id":8}},"unread_count":0}}}}"#
    ));
    rig.ingest(&format!(
        r#"{{"@type":"createdBasicGroupChat","chat_id":{GROUP},"failed_to_add_members":{{"@type":"failedToAddMembers","failed_to_add_members":[]}},"@extra":"{}"}}"#,
        extra.0
    ));
    assert_ne!(rig.driver.session.open_chat_id(), Some(ChatId(GROUP)));
    assert!(
        !rig.driver
            .session
            .groups
            .new_chats
            .creations
            .contains_key(&extra),
        "nobody reads the answer: the entry is dropped when it is applied"
    );
}

/// Round 4, 006: a create's entry goes with its answer, whoever reads it.
#[test]
fn an_answered_creation_is_dropped_once_its_dialog_has_taken_it() {
    let mut rig = Rig::new();
    let creations = |rig: &Rig| rig.driver.session.groups.new_chats.creations.len();
    // Refused after the dialog closed: forgotten at once.
    let closed = rig.create(NewChatKind::Group, &[5], None);
    rig.driver.abandon_new_chat(closed);
    rig.ingest(&format!(
        r#"{{"@type":"error","code":400,"message":"USERS_TOO_FEW","@extra":"{}"}}"#,
        closed.0
    ));
    assert_eq!(creations(&rig), 0);
    // Refused with the dialog open: the dialog reads the reason, then
    // leaves the stage (abandon is its "answer taken").
    let open = rig.create(NewChatKind::Group, &[5], None);
    rig.ingest(&format!(
        r#"{{"@type":"error","code":400,"message":"USERS_TOO_FEW","@extra":"{}"}}"#,
        open.0
    ));
    assert!(rig.outcome(open).is_some());
    rig.driver.abandon_new_chat(open);
    assert_eq!(creations(&rig), 0);
}

/// Round 5, 086: a type-step Save that was refused for a reason other
/// than a bad link, or lost, does not move on to the subscribers step as
/// if it had saved.
#[test]
fn a_refused_or_lost_type_save_keeps_the_type_step() {
    let mut rig = Rig::new();
    let extra = rig.create(NewChatKind::Channel, &[], None);
    rig.answer_channel(extra);
    let chat = ChatId(CHANNEL);
    let edits = [crate::group_type::GroupTypeEdit::DisableAllUsernames];
    let sent = rig.driver.save_group_type(chat, &edits, &[]).unwrap();
    assert!(!sent.is_empty());
    rig.driver.watch_new_chat_type_save(&sent);
    let stage = NewChatStage::Type {
        chat,
        save: TypeSaveStage::Saving(sent.clone()),
    };
    assert_eq!(
        rig.driver.session.settled_new_chat(&stage),
        stage,
        "the Save is still out"
    );
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
        sent[0].0
    ));
    assert!(
        matches!(
            rig.driver.session.settled_new_chat(&stage),
            NewChatStage::Type {
                save: TypeSaveStage::Refused(_),
                ..
            }
        ),
        "the step stays"
    );
    assert!(
        rig.driver
            .session
            .groups
            .new_chats
            .type_save_failure(&sent)
            .is_some()
    );
}

/// A type-step Save whose requests all answered well moves the stage on to
/// the subscribers; a lost request returns a create to its form.
#[test]
fn an_answered_type_save_moves_the_stage_on() {
    let mut rig = Rig::new();
    let extra = rig.create(NewChatKind::Channel, &[], None);
    rig.answer_channel(extra);
    let chat = ChatId(CHANNEL);
    let edits = [crate::group_type::GroupTypeEdit::DisableAllUsernames];
    let sent = rig.driver.save_group_type(chat, &edits, &[]).unwrap();
    rig.driver.watch_new_chat_type_save(&sent);
    let stage = NewChatStage::Type {
        chat,
        save: TypeSaveStage::Saving(sent.clone()),
    };
    assert_eq!(rig.driver.session.settled_new_chat(&stage), stage);
    for id in &sent {
        rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, id.0));
    }
    assert_eq!(
        rig.driver.session.settled_new_chat(&stage),
        NewChatStage::Subscribers { chat }
    );
    // A create whose request is gone with no answer returns the form.
    let lost = NewChatStage::Creating {
        request: RequestId(9_999),
        back: crate::new_chat::NewChatBack::Info,
        progress: CreateProgress::InFlight,
    };
    assert!(matches!(
        rig.driver.session.settled_new_chat(&lost),
        NewChatStage::Creating {
            progress: CreateProgress::Lost,
            ..
        }
    ));
}

/// A Save request that is gone without an answer did not happen: the step
/// stays with a reason, and an old refusal of the same chat and field (not
/// this Save's request) neither moves it nor stops it.
#[test]
fn a_lost_type_save_request_lands_in_refused() {
    let mut rig = Rig::new();
    let extra = rig.create(NewChatKind::Channel, &[], None);
    rig.answer_channel(extra);
    let chat = ChatId(CHANNEL);
    let lost = RequestId(987_654);
    rig.driver.watch_new_chat_type_save(&[lost]);
    let stage = NewChatStage::Type {
        chat,
        save: TypeSaveStage::Saving(vec![lost]),
    };
    assert!(
        matches!(
            rig.driver.session.settled_new_chat(&stage),
            NewChatStage::Type {
                save: TypeSaveStage::Refused(_),
                ..
            }
        ),
        "a request that vanished is a failed Save"
    );
    // An answered Save moves on even with an older refusal for the chat.
    let edits = [crate::group_type::GroupTypeEdit::DisableAllUsernames];
    let sent = rig.driver.save_group_type(chat, &edits, &[]).unwrap();
    rig.driver.watch_new_chat_type_save(&sent);
    rig.driver.session.groups.info_edit_errors.insert(
        chat.0,
        crate::state::GroupInfoEditError {
            field: crate::state::GroupInfoField::Username,
            reason: "old".into(),
        },
    );
    for id in &sent {
        rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, id.0));
    }
    let stage = NewChatStage::Type {
        chat,
        save: TypeSaveStage::Saving(sent),
    };
    assert_eq!(
        rig.driver.session.settled_new_chat(&stage),
        NewChatStage::Subscribers { chat }
    );
}
