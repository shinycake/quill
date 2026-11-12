//! Sweep fixes (r2-15): requests sent from a poll or a refresh, answers
//! matched to the ask that sent them.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut rig = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            _dir: dir,
        };
        rig.ingest(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        rig
    }

    fn ingest(&mut self, json: &str) {
        if let Some(parsed) = copy_and_parse(json, &self.seq, &self.sink) {
            let _ = self.driver.ingest(parsed);
        }
    }

    /// The requests of this `@type` sent so far.
    fn sent(&self, kind: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .filter_map(|json| serde_json::from_str::<Value>(json).ok())
            .filter(|v| v["@type"] == kind)
            .collect()
    }
}

fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap_or_default().to_owned()
}

fn forum_rig() -> Rig {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
    );
    rig.ingest(
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"is_forum":true}}"#,
    );
    rig
}

fn topics_answer(extra: &str) -> String {
    format!(r#"{{"@type":"forumTopics","@extra":"{extra}","total_count":0,"topics":[]}}"#)
}

#[test]
fn a_forum_refresh_is_sent_even_while_an_older_ask_is_out_and_the_older_answer_is_dropped() {
    let mut rig = forum_rig();
    rig.driver.load_forum_topics(ChatId(13)).unwrap();
    let asks = rig.sent("getForumTopics");
    assert_eq!(asks.len(), 1);
    // A mutation lands while that ask is out: the manage dialog refreshes.
    rig.driver.refresh_forum_topics(ChatId(13)).unwrap();
    let asks = rig.sent("getForumTopics");
    assert_eq!(asks.len(), 2, "the refresh is not a no-op");
    // The older ask predates the change: its answer is not taken.
    rig.ingest(&topics_answer(&extra_of(&asks[0])));
    assert!(!rig.driver.session.threads.forum_topics.contains_key(&13));
    rig.ingest(&topics_answer(&extra_of(&asks[1])));
    assert!(rig.driver.session.threads.forum_topics.contains_key(&13));
}

#[test]
fn a_failed_get_me_is_not_asked_again_by_the_poll() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"news","type":{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":true},"unread_count":0}}"#,
    );
    rig.driver.session.open_chat(ChatId(9));
    rig.driver.maybe_probe_channel_membership().unwrap();
    rig.driver.maybe_probe_channel_membership().unwrap();
    let asks = rig.sent("getMe");
    assert_eq!(asks.len(), 1, "one getMe while it is in flight");
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
        extra_of(&asks[0])
    ));
    rig.driver.maybe_probe_channel_membership().unwrap();
    rig.driver.maybe_probe_channel_membership().unwrap();
    assert_eq!(rig.sent("getMe").len(), 1, "no re-ask after the failure");
}

#[test]
fn a_full_rich_load_ends_only_with_the_answer_of_the_ask_that_waits() {
    let mut rig = Rig::new();
    let key = crate::ids::ChatMessage::new(ChatId(7), crate::ids::MessageId(42));
    let first = rig
        .driver
        .fetch_full_rich_message(key.chat, key.message)
        .unwrap();
    let second = rig
        .driver
        .fetch_full_rich_message(key.chat, key.message)
        .unwrap();
    // The older ask is refused: the newer one still loads.
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
        first.0
    ));
    assert!(rig.driver.session.full_rich_loading(key.chat, key.message));
    assert!(rig.driver.session.chats_state.chat_action_error.is_empty());
    rig.ingest(&format!(
        r#"{{"@type":"richMessage","@extra":"{}","is_rtl":false,"is_full":true,"blocks":[]}}"#,
        second.0
    ));
    assert!(!rig.driver.session.full_rich_loading(key.chat, key.message));
}

/// A sender that lets `ok` sends through and refuses every later one.
struct FailsAfter {
    inner: Arc<RecordingSender>,
    ok: std::sync::atomic::AtomicUsize,
}

impl JsonSender for FailsAfter {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        use std::sync::atomic::Ordering;
        let left = self.ok.load(Ordering::SeqCst);
        if left == 0 {
            return Err(ConnectSendError::Native);
        }
        self.ok.store(left - 1, Ordering::SeqCst);
        self.inner.send_json(request)
    }
}

#[test]
fn a_basic_group_add_that_stops_midway_counts_the_unsent_users_as_failed() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let sender = FailsAfter {
        inner: Arc::new(RecordingSender::new()),
        ok: std::sync::atomic::AtomicUsize::new(usize::MAX),
    };
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":5,"title":"g","type":{"@type":"chatTypeBasicGroup","basic_group_id":5},"permissions":{"@type":"chatPermissions","can_invite_users":true},"unread_count":0}}"#,
    ] {
        if let Some(parsed) = copy_and_parse(json, &seq, &sink) {
            driver.ingest(parsed).unwrap();
        }
    }
    driver
        .sender
        .ok
        .store(1, std::sync::atomic::Ordering::SeqCst);
    let first = driver.add_chat_members(ChatId(5), &[1, 2, 3]).unwrap();
    assert!(first.is_some(), "the first user went out");
    assert_eq!(
        driver.session.groups.add_members_failed.get(&5),
        Some(&2),
        "users 2 and 3 were never sent"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_trending_page_keeps_the_list_and_a_new_ask_ends_the_failure() {
    use crate::emoji::EmojiSetTab;
    let mut rig = Rig::new();
    rig.driver.open_emoji_sets().unwrap();
    let first = rig
        .driver
        .select_emoji_set_tab(EmojiSetTab::Trending)
        .unwrap()
        .unwrap();
    rig.ingest(&format!(
        r#"{{"@type":"trendingStickerSets","@extra":"{}","total_count":5,"sets":[],"is_premium":false}}"#,
        first.0
    ));
    let lists = &rig.driver.session.stickers.emoji.lists;
    assert!(!lists.trending.is_loading() && lists.trending.error().is_none());
    // Choosing the tab again asks again; that ask is refused.
    let second = rig
        .driver
        .select_emoji_set_tab(EmojiSetTab::Trending)
        .unwrap()
        .unwrap();
    assert!(
        rig.driver
            .session
            .stickers
            .emoji
            .lists
            .trending
            .is_loading()
    );
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
        second.0
    ));
    assert!(
        rig.driver
            .session
            .stickers
            .emoji
            .lists
            .trending
            .error()
            .is_some()
    );
    // The answer of the first ask, long gone, changes nothing.
    rig.ingest(&format!(
        r#"{{"@type":"trendingStickerSets","@extra":"{}","total_count":5,"sets":[],"is_premium":true}}"#,
        first.0
    ));
    let trending = &rig.driver.session.stickers.emoji.lists.trending;
    assert!(trending.error().is_some() && !trending.is_premium());
}

#[test]
fn a_keyword_search_is_settled_only_by_its_own_answer_and_a_refusal_is_not_asked_again() {
    let mut rig = Rig::new();
    rig.driver.search_keyword_emojis("fire").unwrap();
    let first = rig.sent("getKeywordEmojis");
    rig.driver.search_keyword_emojis("water").unwrap();
    let both = rig.sent("getKeywordEmojis");
    assert_eq!(both.len(), 2);
    // The first search was replaced: its answer lands nowhere.
    rig.ingest(&format!(
        r#"{{"@type":"emojis","emojis":["🔥"],"@extra":"{}"}}"#,
        extra_of(&first[0])
    ));
    let keywords = &rig.driver.session.stickers.emoji.keyword_emojis;
    assert_eq!(keywords.query(), "water");
    assert!(keywords.for_query("water").is_empty() && keywords.for_query("fire").is_empty());
    // The second is refused: the query stays, so the panel does not re-ask.
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
        extra_of(&both[1])
    ));
    let keywords = &rig.driver.session.stickers.emoji.keyword_emojis;
    assert!(matches!(keywords, crate::emoji::KeywordEmojis::Failed { query } if query == "water"));
}

#[test]
fn an_animated_emoji_answer_for_a_replaced_ask_is_dropped() {
    let mut rig = Rig::new();
    rig.driver.update_animated_emoji_suggestion("a 🔥").unwrap();
    rig.driver.update_animated_emoji_suggestion("a 😀").unwrap();
    let asks = rig.sent("getAnimatedEmoji");
    assert_eq!(asks.len(), 2);
    rig.ingest(&format!(
        r#"{{"@type":"animatedEmoji","@extra":"{}","sticker":null,"sticker_width":0,"sticker_height":0,"fitzpatrick_type":0,"sound":null}}"#,
        extra_of(&asks[0])
    ));
    assert!(matches!(
        rig.driver.session.stickers.emoji.animated,
        crate::emoji::AnimatedSuggestion::Asking { ref emoji, .. } if emoji == "😀"
    ));
}

#[test]
fn a_failed_animated_emoji_ask_leaves_asking_and_is_not_repeated() {
    let mut rig = Rig::new();
    rig.driver.update_animated_emoji_suggestion("a 🔥").unwrap();
    rig.driver.update_animated_emoji_suggestion("a 😀").unwrap();
    let asks = rig.sent("getAnimatedEmoji");
    // The replaced ask's failure is not the current one's.
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"EMOTICON_INVALID"}}"#,
        extra_of(&asks[0])
    ));
    assert!(matches!(
        rig.driver.session.stickers.emoji.animated,
        crate::emoji::AnimatedSuggestion::Asking { ref emoji, .. } if emoji == "😀"
    ));
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"EMOTICON_INVALID"}}"#,
        extra_of(&asks[1])
    ));
    assert!(matches!(
        rig.driver.session.stickers.emoji.animated,
        crate::emoji::AnimatedSuggestion::Failed { ref emoji } if emoji == "😀"
    ));
    // The same emoji is not asked for again while it is typed after.
    rig.driver
        .update_animated_emoji_suggestion("ab 😀")
        .unwrap();
    assert_eq!(rig.sent("getAnimatedEmoji").len(), 2);
    // Another emoji asks, and a text with none clears the stage.
    rig.driver
        .update_animated_emoji_suggestion("ab 🔥")
        .unwrap();
    assert_eq!(rig.sent("getAnimatedEmoji").len(), 3);
    rig.driver.update_animated_emoji_suggestion("ab").unwrap();
    assert!(rig.driver.session.stickers.emoji.animated.is_idle());
}

#[test]
fn a_refused_member_probe_is_not_sent_again_by_the_poll() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"news","type":{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":true},"unread_count":0}}"#,
    );
    rig.driver.session.my_user_id = Some(5);
    rig.driver.session.open_chat(ChatId(9));
    rig.driver.maybe_probe_channel_membership().unwrap();
    let asks = rig.sent("getChatMember");
    assert_eq!(asks.len(), 1);
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHANNEL_PRIVATE"}}"#,
        extra_of(&asks[0])
    ));
    rig.driver.maybe_probe_channel_membership().unwrap();
    rig.driver.maybe_probe_channel_membership().unwrap();
    assert_eq!(
        rig.sent("getChatMember").len(),
        1,
        "no re-ask after the refusal"
    );
}

#[test]
fn opening_a_forum_retries_a_failed_topic_list_but_the_rows_do_not() {
    let mut rig = forum_rig();
    rig.driver.load_forum_topics(ChatId(13)).unwrap();
    let ask = rig.sent("getForumTopics");
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
        extra_of(&ask[0])
    ));
    // A row's upkeep asks nothing while the list is failed.
    rig.driver.load_forum_topics(ChatId(13)).unwrap();
    assert_eq!(rig.sent("getForumTopics").len(), 1);
    // Opening the chat asks again.
    rig.driver.fetch_open_chat_extras(ChatId(13)).unwrap();
    assert_eq!(rig.sent("getForumTopics").len(), 2);
}

fn supergroup_rig(status: &str) -> Rig {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":9,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":false},"unread_count":0}}"#,
    );
    rig.ingest(&format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":9,"status":{status}}}}}"#
    ));
    rig.driver.session.my_user_id = Some(5);
    rig.driver.session.open_chat(ChatId(9));
    rig
}

/// Sweep r3-02 (07-003): a status known only from `updateSupergroup` carries
/// no rights, so the own-member probe is still sent and its answer gives them.
#[test]
fn a_supergroup_admin_status_still_sends_the_probe_and_takes_its_rights() {
    let mut rig = supergroup_rig(r#"{"@type":"chatMemberStatusAdministrator"}"#);
    let chat = ChatId(9);
    assert!(!rig.driver.session.chats[&9].can_invite_users());
    rig.driver.maybe_probe_channel_membership().unwrap();
    let asks = rig.sent("getChatMember");
    assert_eq!(asks.len(), 1, "the status alone does not settle the probe");
    rig.ingest(&format!(
        r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":5}},"inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusAdministrator","rights":{{"@type":"chatAdministratorRights","can_invite_users":true}}}}}}"#,
        extra_of(&asks[0])
    ));
    assert!(rig.driver.session.chats[&chat.0].can_invite_users());
    rig.driver.maybe_probe_channel_membership().unwrap();
    assert_eq!(
        rig.sent("getChatMember").len(),
        1,
        "answered: not asked again"
    );
}

/// Sweep r3-02 (07-003): a refused probe settles a chat whose status came
/// from `updateSupergroup` as restricted, and the status is kept.
#[test]
fn a_refused_probe_settles_a_supergroup_restricted_status() {
    let mut rig = supergroup_rig(r#"{"@type":"chatMemberStatusRestricted","is_member":true}"#);
    rig.driver.maybe_probe_channel_membership().unwrap();
    let asks = rig.sent("getChatMember");
    assert_eq!(asks.len(), 1);
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHANNEL_PRIVATE"}}"#,
        extra_of(&asks[0])
    ));
    rig.driver.maybe_probe_channel_membership().unwrap();
    rig.driver.maybe_probe_channel_membership().unwrap();
    assert_eq!(
        rig.sent("getChatMember").len(),
        1,
        "no re-ask after the refusal"
    );
    let chat = &rig.driver.session.chats[&9];
    assert!(chat.rights_fetched());
    assert_eq!(
        chat.member_status(),
        Some(crate::telegram::envelope::ChannelMemberStatus::Restricted)
    );
}
