//! Sweep fixes (s3-messages-1): failed sends and unmatched answers around
//! drafts, owned bots, polls, quick replies, stickers and the box sends.
use super::super::*;
use super::*;
use crate::composer::DraftTopic;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A sender that records what it takes and refuses everything while
/// `refuse` is set (a channel closed during a reconnect).
struct FlakySender {
    sent: Mutex<Vec<String>>,
    refuse: AtomicBool,
    /// Sends still taken before it refuses; negative: no limit.
    allow: AtomicIsize,
}

impl JsonSender for Arc<FlakySender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if self.refuse.load(Ordering::SeqCst) {
            return Err(ConnectSendError::Native);
        }
        let allowed = self.allow.load(Ordering::SeqCst);
        if allowed == 0 {
            return Err(ConnectSendError::Native);
        }
        if allowed > 0 {
            self.allow.store(allowed - 1, Ordering::SeqCst);
        }
        self.sent
            .lock()
            .expect("flaky sender")
            .push(request.to_string());
        Ok(())
    }
}

struct Rig {
    driver: ConnectDriver<Arc<FlakySender>>,
    sender: Arc<FlakySender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let sender = Arc::new(FlakySender {
            sent: Mutex::new(Vec::new()),
            refuse: AtomicBool::new(false),
            allow: AtomicIsize::new(-1),
        });
        let mut driver = ConnectDriver::new(
            Session::new(AccountKey::primary(), sink.clone()),
            sender.clone(),
            test_credentials(),
            prepared,
        );
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &sink,
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            driver,
            sender,
            sink,
            seq,
            dir,
        }
    }

    fn refuse(&self, refuse: bool) {
        self.sender.refuse.store(refuse, Ordering::SeqCst);
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self, kind: &str) -> Vec<Value> {
        self.sender
            .sent
            .lock()
            .expect("flaky sender")
            .iter()
            .filter_map(|json| serde_json::from_str::<Value>(json).ok())
            .filter(|value| value["@type"] == kind)
            .collect()
    }

    fn error(&mut self, request: RequestId, code: i32) {
        self.ingest(&format!(
            r#"{{"@type":"error","@extra":"{}","code":{code},"message":"x"}}"#,
            request.0
        ));
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_draft_save_that_fails_to_send_is_kept_for_the_next_flush() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7}}}"#,
    );
    rig.driver.session.open_chat(ChatId(7));
    rig.refuse(true);
    let content = crate::composer::DraftContent::Text("hello".into());
    // The first note waits out its quiet window; the timer's commit fails.
    let outcome = rig
        .driver
        .note_composer_content_in(ChatId(7), DraftTopic::Chat, content, None, 1_000, true)
        .unwrap();
    let DraftSaveOutcome::Debounced { token, .. } = outcome else {
        panic!("a debounced save, got {outcome:?}");
    };
    assert!(rig.driver.commit_debounced_draft(token).is_err());
    assert!(
        rig.driver.session.draft_is_dirty(ChatId(7), DraftTopic::Chat),
        "the slot stays dirty while nothing was saved"
    );
    // The draft was not forgotten: the next flush sends it.
    rig.refuse(false);
    assert!(rig.driver.flush_pending_draft().unwrap().is_some());
    let drafts = rig.sent("setChatDraftMessage");
    assert_eq!(drafts.len(), 1);
    assert!(drafts[0].to_string().contains("hello"));
    assert!(!rig.driver.session.draft_is_dirty(ChatId(7), DraftTopic::Chat));
}

#[test]
fn opening_the_bot_editor_when_the_second_load_fails_leaves_nothing_half_started() {
    let mut rig = Rig::new();
    // The first load goes, the sender refuses the second.
    rig.sender.allow.store(1, Ordering::SeqCst);
    assert!(rig.driver.begin_bot_edit(21).is_err());
    assert!(rig.driver.session.bots.owned.edit.is_none());
    assert!(
        !rig.driver
            .session
            .requests
            .has_purpose(crate::state::RequestPurpose::Bots(
                crate::state::BotsPurpose::GetBotEditText {
                    bot_user_id: 21,
                    about: false
                }
            ))
    );
}

#[test]
fn stopping_a_poll_finds_it_in_a_topic_history() {
    let (dir, mut driver, _recorder, _sink, _dyn_sink, _seq) = poll_driver();
    // An own poll that only a topic's history holds.
    let mut message = driver
        .session
        .histories
        .get_mut(&7)
        .and_then(|history| history.messages.remove(&106))
        .expect("the poll");
    message.is_outgoing = true;
    let mut topic = crate::state::TopicHistory::default();
    topic.messages.insert(106, message);
    driver.session.threads.topic_histories.insert((7, 3), topic);
    assert!(driver.stop_poll(ChatId(7), MessageId(106)).unwrap().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_stickers_manager_step_keeps_the_commits_behind_it() {
    use crate::state::ManagerCommit;
    let mut rig = Rig::new();
    rig.driver
        .session
        .stickers
        .manager
        .commits
        .push_back(ManagerCommit {
            masks: false,
            removals: [11, 12].into(),
            order: None,
        });
    rig.driver
        .session
        .stickers
        .manager
        .commits
        .push_back(ManagerCommit {
            masks: false,
            removals: [21].into(),
            order: None,
        });
    rig.refuse(true);
    assert!(rig.driver.advance_manager_commit().is_err());
    // Only the step that did not go out is lost.
    let commits = &rig.driver.session.stickers.manager.commits;
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].removals, [12]);
    assert_eq!(commits[1].removals, [21]);
    assert!(rig.driver.session.stickers.creator_note.is_some());
    // The queue goes on once the sender is back.
    rig.refuse(false);
    rig.driver.advance_manager_commit().unwrap();
    assert_eq!(rig.sent("changeStickerSet")[0]["set_id"], "12");
}

#[test]
fn stale_sticker_lists_stay_stale_until_their_refetch_goes_out() {
    let mut rig = Rig::new();
    let panel = &mut rig.driver.session.stickers.stickers;
    panel.open = true;
    panel.recent_stale = true;
    panel.favorites_stale = true;
    // The first refetch cannot be sent: neither flag is lost.
    rig.refuse(true);
    assert!(rig.driver.refresh_stale_panels().is_err());
    let panel = &rig.driver.session.stickers.stickers;
    assert!(panel.recent_stale && panel.favorites_stale);
    // Sent: its flag goes, and so does the next one's.
    rig.refuse(false);
    rig.driver.refresh_stale_panels().unwrap();
    let panel = &rig.driver.session.stickers.stickers;
    assert!(!panel.recent_stale && !panel.favorites_stale);
    assert_eq!(rig.sent("getRecentStickers").len(), 1);
    assert_eq!(rig.sent("getFavoriteStickers").len(), 1);
}

#[test]
fn one_failing_sticker_request_fails_only_its_own_list() {
    use crate::state::StickerTab;
    let mut rig = Rig::new();
    rig.driver.session.stickers.stickers.open = true;
    let trending = rig
        .driver
        .select_sticker_tab(StickerTab::Trending)
        .unwrap()
        .unwrap();
    let recent = rig
        .driver
        .select_sticker_tab(StickerTab::Recent)
        .unwrap()
        .unwrap();
    rig.error(trending, 500);
    let lists = &rig.driver.session.stickers.stickers.lists;
    assert!(lists.trending.error().is_some());
    assert!(lists.recent.is_loading(), "an unrelated list is not failed");
    // A failed change is a note, not a list failure.
    rig.error(recent, 500);
    assert!(
        rig.driver
            .session
            .stickers
            .stickers
            .lists
            .favorites
            .error()
            .is_none()
    );
}

#[test]
fn an_answer_for_a_set_that_is_no_longer_selected_does_not_end_the_new_selection() {
    use crate::state::StickerSetStage;
    let mut rig = Rig::new();
    rig.driver.session.stickers.stickers.open = true;
    let first = rig.driver.select_sticker_set(77).unwrap().unwrap();
    // Another set is chosen before the first answers: it is asked at once.
    let second = rig.driver.select_sticker_set(88).unwrap().unwrap();
    assert_eq!(
        rig.driver.session.stickers.stickers.set,
        StickerSetStage::Loading {
            set_id: 88,
            request: second
        }
    );
    // The first set's failure changes nothing for the second.
    rig.error(first, 500);
    assert_eq!(
        rig.driver.session.stickers.stickers.set,
        StickerSetStage::Loading {
            set_id: 88,
            request: second
        }
    );
    // Its own failure fails it.
    rig.error(second, 500);
    assert_eq!(
        rig.driver.session.stickers.stickers.set,
        StickerSetStage::Failed { set_id: 88 }
    );
}

#[test]
fn a_failed_category_strip_is_asked_again_when_the_panel_opens_again() {
    let mut rig = Rig::new();
    rig.driver.fetch_sticker_categories().unwrap();
    let asked = rig.sent("getEmojiCategories");
    assert_eq!(asked.len(), 1);
    let request = RequestId(asked[0]["@extra"].as_str().unwrap().parse().unwrap());
    rig.error(request, 500);
    let categories = &mut rig.driver.session.stickers.emoji.sticker_categories;
    assert!(categories.error().is_some());
    // The render does not ask again on its own.
    rig.driver.fetch_sticker_categories().unwrap();
    assert_eq!(rig.sent("getEmojiCategories").len(), 1);
    // Opening the panel again clears the failure; then it asks once more.
    rig.driver
        .session
        .stickers
        .emoji
        .sticker_categories
        .clear_error();
    rig.driver.fetch_sticker_categories().unwrap();
    assert_eq!(rig.sent("getEmojiCategories").len(), 2);
}

#[test]
fn creating_a_forum_topic_lets_the_chat_list_row_ask_for_its_topics_again() {
    let mut rig = Rig::new();
    let threads = &mut rig.driver.session.threads;
    threads.forum_row_asked.insert(7);
    threads.forum_topics_failed.insert(7);
    let created = rig.driver.session.request(
        crate::state::RequestPurpose::CreateForumTopic,
        Some(ChatId(7)),
    );
    rig.ingest(&format!(
        r#"{{"@type":"forumTopicInfo","chat_id":7,"forum_topic_id":3,"name":"x","@extra":"{}"}}"#,
        created.0
    ));
    let threads = &rig.driver.session.threads;
    assert!(!threads.forum_row_asked.contains(&7));
    assert!(!threads.forum_topics_failed.contains(&7));
}

#[test]
fn a_start_parameter_goes_only_when_telegram_took_the_start() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":31}}}"#,
    );
    rig.driver
        .session
        .bots
        .bot_start_params
        .insert(31, "ref".into());
    let refused = rig
        .driver
        .send_bot_start_message(ChatId(31), 31, "ref")
        .unwrap()
        .unwrap();
    rig.error(refused, 429);
    assert_eq!(
        rig.driver.session.bots.bot_start_params.get(&31).map(String::as_str),
        Some("ref"),
        "a refusal keeps the parameter for the next press"
    );
    let taken = rig
        .driver
        .send_bot_start_message(ChatId(31), 31, "ref")
        .unwrap()
        .unwrap();
    rig.ingest(&format!(
        r#"{{"@type":"message","id":900,"chat_id":31,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"/start ref","entities":[]}}}},"@extra":"{}"}}"#,
        taken.0
    ));
    assert!(rig.driver.session.bots.bot_start_params.is_empty());
}

#[test]
fn a_box_request_settles_as_answered_or_failed_by_its_request() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":31}}}"#,
    );
    let ok = rig
        .driver
        .send_game_message(ChatId(31), 31, "game")
        .unwrap();
    let bad = rig
        .driver
        .send_game_message(ChatId(31), 31, "game2")
        .unwrap();
    rig.driver.session.bots.sends.watch(ok);
    rig.driver.session.bots.sends.watch(bad);
    assert_eq!(rig.driver.session.bots.sends.take_outcome(ok), None);
    rig.error(bad, 403);
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, ok.0));
    assert_eq!(rig.driver.session.bots.sends.take_outcome(bad), Some(false));
    assert_eq!(rig.driver.session.bots.sends.take_outcome(ok), Some(true));
}

#[test]
fn a_failed_game_button_press_is_noted() {
    let mut rig = Rig::new();
    let request = rig.driver.session.request(
        crate::state::RequestPurpose::GetCallbackQueryAnswerGame,
        Some(ChatId(31)),
    );
    rig.error(request, 502);
    let answer = rig.driver.session.bots.last_callback_answer.as_ref();
    assert_eq!(answer.map(|a| a.answer.text.as_str()), Some("bot did not answer"));
}
