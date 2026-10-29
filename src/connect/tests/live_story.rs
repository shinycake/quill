//! Connect-driver tests: watching a live story (no real call, no Stars).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId};
use crate::live_story::CommentsState;
use crate::paid_reaction::ACCUMULATE_MS;
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageSender;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(super) const LIVE_CALL: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":4242,"title":"Live","is_active":true,"is_video_chat":true,"is_live_story":true,"is_rtmp_stream":false,"is_joined":JOINED,"need_rejoin":false,"participant_count":38,"can_send_messages":true,"are_messages_allowed":true,"paid_message_star_count":5,"message_sender_id":{"@type":"messageSenderUser","user_id":1}}}"#;

pub(super) struct Harness {
    pub(super) driver: ConnectDriver<Arc<RecordingSender>>,
    pub(super) recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    dir: std::path::PathBuf,
}

impl Harness {
    pub(super) fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    pub(super) fn answer(&mut self, request: &str, body: &str) {
        let extra = sent_request(&self.recorder, request)["@extra"]
            .as_str()
            .unwrap()
            .to_string();
        let json = body.replacen('{', &format!(r#"{{"@extra":"{extra}","#), 1);
        self.ingest(&json);
    }

    pub(super) fn sent(&self, type_name: &str) -> usize {
        self.recorder
            .snapshot()
            .iter()
            .filter(|json| json.contains(&format!(r#""@type":"{type_name}""#)))
            .count()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A ready driver with no live story yet.
pub(super) fn harness() -> Harness {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    // A live story only joins with a call engine to play it.
    driver.set_call_engine(Box::new(crate::calls::engine::MockEngine::new()));
    let mut h = Harness {
        driver,
        recorder,
        seq: AtomicU64::new(0),
        sink,
        dir,
    };
    seed_ready_alice(&mut h.driver, &h.seq, &h.sink);
    h.ingest(
        r#"{"@type":"updateOption","name":"my_id","value":{"@type":"optionValueInteger","value":1}}"#,
    );
    h
}

/// A driver watching the live story (7, 6) on call 4242, joined.
pub(super) fn watching() -> Harness {
    let mut h = harness();
    h.ingest(
        r#"{"@type":"story","id":6,"poster_chat_id":7,"date":1700000000,"content":{"@type":"storyContentLive","group_call_id":4242,"is_rtmp_stream":false},"caption":{"@type":"formattedText","text":"","entities":[]}}"#,
    );
    h.driver.join_live_story(ChatId(7), 6).unwrap();
    h.ingest(&LIVE_CALL.replace("JOINED", "false"));
    h.ingest(&LIVE_CALL.replace("JOINED", "false"));
    assert_eq!(h.sent("joinLiveStory"), 1);
    h
}

#[test]
fn the_join_answer_asks_for_streamer_donors_and_senders_together() {
    let mut h = watching();
    assert_eq!(h.sent("getLiveStoryStreamer"), 0, "not before the answer");
    h.answer(
        "joinLiveStory",
        r#"{"@type":"text","text":"{\"stream\":true}"}"#,
    );
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    assert_eq!(h.sent("getLiveStoryStreamer"), 1);
    assert_eq!(h.sent("getLiveStoryTopDonors"), 1);
    assert_eq!(h.sent("getLiveStoryAvailableMessageSenders"), 1);
    // Only once.
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    assert_eq!(h.sent("getLiveStoryTopDonors"), 1);

    h.answer(
        "getLiveStoryTopDonors",
        r#"{"@type":"liveStoryDonors","total_star_count":120,"top_donors":[{"@type":"paidReactor","sender_id":{"@type":"messageSenderUser","user_id":7},"star_count":120,"is_top":true,"is_me":false,"is_anonymous":false}]}"#,
    );
    h.answer(
        "getLiveStoryStreamer",
        r#"{"@type":"groupCallParticipant","participant_id":{"@type":"messageSenderChat","chat_id":-100},"video_info":{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[5,6]}],"endpoint_id":"stream-ep","is_paused":false},"order":"1"}"#,
    );
    h.answer(
        "getLiveStoryAvailableMessageSenders",
        r#"{"@type":"chatMessageSenders","senders":[{"@type":"chatMessageSender","sender":{"@type":"messageSenderUser","user_id":1},"needs_premium":false},{"@type":"chatMessageSender","sender":{"@type":"messageSenderChat","chat_id":-200},"needs_premium":false}]}"#,
    );
    let live = h.driver.session.stories.live.clone().unwrap();
    assert_eq!(live.stars_total(), 120);
    assert_eq!(live.message_senders.map(|s| s.len()), Some(2));
    // The streamer is a channel: its video is subscribed anyway, and the
    // native call is keyed by the poster's chat.
    let sources = h.driver.live_story_video_sources(4242);
    assert_eq!(
        sources.first().map(|s| s.endpoint.as_str()),
        Some("stream-ep")
    );
    assert_eq!(h.driver.group_call_chat_id(4242), Some(7));

    h.ingest(
        r#"{"@type":"updateLiveStoryTopDonors","group_call_id":4242,"donors":{"@type":"liveStoryDonors","total_star_count":200,"top_donors":[]}}"#,
    );
    assert_eq!(
        h.driver
            .session
            .stories
            .live
            .as_ref()
            .map(|l| l.stars_total()),
        Some(200)
    );
    h.ingest(
        r#"{"@type":"updateGroupCallMessageLevels","levels":[{"@type":"groupCallMessageLevel","min_star_count":50,"pin_duration":120,"max_text_length":80,"max_custom_emoji_count":2,"first_color":4629483,"second_color":20622,"background_color":20622}]}"#,
    );
    let levels: Vec<i64> = h
        .driver
        .session
        .live_levels()
        .iter()
        .map(|l| l.min_star_count)
        .collect();
    assert_eq!(levels, vec![0, 50]);
}

#[test]
fn star_taps_add_up_then_commit_after_the_undo_window() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.add_live_story_reaction(1, 1_000).unwrap();
    h.driver.add_live_story_reaction(1, 2_000).unwrap();
    assert_eq!(h.sent("addPendingLiveStoryReaction"), 2);
    assert_eq!(
        sent_request(&h.recorder, "addPendingLiveStoryReaction")["star_count"],
        1
    );
    assert_eq!(
        h.driver.session.stories.live.as_ref().map(|l| l.my_stars()),
        Some(2)
    );
    assert!(!h.driver.live_story_reaction_tick(2_000 + ACCUMULATE_MS - 1));
    assert!(h.driver.live_story_reaction_tick(2_000 + ACCUMULATE_MS));
    assert_eq!(h.sent("commitPendingLiveStoryReactions"), 1);
    let live = h.driver.session.stories.live.clone().unwrap();
    assert!(live.pending_reaction.is_none());
    assert_eq!(live.effects.first().map(|e| e.stars), Some(2));

    // Undo drops the waiting Stars.
    h.driver.add_live_story_reaction(3, 10_000).unwrap();
    h.driver.undo_live_story_reaction().unwrap();
    assert_eq!(h.sent("removePendingLiveStoryReactions"), 1);
    assert!(!h.driver.live_story_reaction_tick(u64::MAX));

    // A refused add takes the Stars back.
    h.driver.add_live_story_reaction(2, 20_000).unwrap();
    h.answer(
        "addPendingLiveStoryReaction",
        r#"{"@type":"error","code":400,"message":"BALANCE_TOO_LOW"}"#,
    );
    assert!(
        h.driver
            .session
            .stories
            .live
            .as_ref()
            .is_some_and(|l| l.pending_reaction.is_none() && l.note.is_some())
    );
}

#[test]
fn no_stars_on_your_own_live_story_or_past_the_balance() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true").replace(
        r#""can_send_messages""#,
        r#""can_be_managed":true,"can_send_messages""#,
    ));
    assert_invalid(h.driver.add_live_story_reaction(1, 0));
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.session.payments.hub.balance =
        Some(crate::premium_hub::StarAmount { stars: 1, nanos: 0 });
    assert_invalid(h.driver.add_live_story_reaction(2, 0));
    assert_eq!(h.sent("addPendingLiveStoryReaction"), 0);
    assert!(
        h.driver
            .session
            .stories
            .live
            .as_ref()
            .is_some_and(|l| l.note.is_some())
    );
}

#[test]
fn comments_flatten_carry_stars_and_failures_stay() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.send_live_story_comment("one\ntwo", 50).unwrap();
    let sent = sent_request(&h.recorder, "sendGroupCallMessage");
    assert_eq!(sent["text"]["text"], "one two");
    assert_eq!(sent["paid_message_star_count"], 50);
    assert_invalid(h.driver.send_live_story_comment("  ", 0));

    h.ingest(
        r#"{"@type":"updateNewGroupCallMessage","group_call_id":4242,"message":{"@type":"groupCallMessage","message_id":3,"sender_id":{"@type":"messageSenderUser","user_id":7},"date":1700000000,"text":{"@type":"formattedText","text":"great","entities":[]},"paid_message_star_count":100,"is_from_owner":false,"can_be_deleted":true}}"#,
    );
    // A paid reaction under the comment price only counts as Stars.
    h.ingest(
        r#"{"@type":"updateNewGroupCallMessage","group_call_id":4242,"message":{"@type":"groupCallMessage","message_id":4,"sender_id":{"@type":"messageSenderUser","user_id":8},"date":1700000001,"text":{"@type":"formattedText","text":"","entities":[]},"paid_message_star_count":1,"is_from_owner":false,"can_be_deleted":true}}"#,
    );
    let ids: Vec<i32> = h
        .driver
        .session
        .calls
        .story_stream
        .as_ref()
        .map(|c| c.messages.iter().map(|m| m.message_id).collect())
        .unwrap_or_default();
    assert_eq!(ids, vec![3]);
    let live = h.driver.session.stories.live.clone().unwrap();
    assert_eq!(live.comments.state(), CommentsState::Shown);
    assert_eq!(live.effects.first().map(|e| e.stars), Some(100));

    h.ingest(
        r#"{"@type":"updateGroupCallMessageSendFailed","group_call_id":4242,"message_id":3,"error":{"@type":"error","code":400,"message":"X"}}"#,
    );
    assert!(
        h.driver
            .session
            .stories
            .live
            .as_ref()
            .is_some_and(|l| l.failed.contains(&3))
    );
    assert!(h.driver.session.calls.group_call_error.is_none());

    h.driver.delete_live_story_comment(3, true, None).unwrap();
    assert_eq!(
        sent_request(&h.recorder, "deleteGroupCallMessages")["report_spam"],
        true
    );
    assert!(
        h.driver
            .session
            .calls
            .story_stream
            .as_ref()
            .is_some_and(|c| c.messages.is_empty())
    );
}

#[test]
fn send_as_is_limited_to_offered_senders() {
    let mut h = watching();
    h.answer("joinLiveStory", r#"{"@type":"text","text":"{}"}"#);
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    let channel = MessageSender::Chat { chat_id: -200 };
    assert_invalid(h.driver.set_live_story_sender(channel));
    h.answer(
        "getLiveStoryAvailableMessageSenders",
        r#"{"@type":"chatMessageSenders","senders":[{"@type":"chatMessageSender","sender":{"@type":"messageSenderChat","chat_id":-200},"needs_premium":false}]}"#,
    );
    h.driver.set_live_story_sender(channel).unwrap();
    assert_eq!(
        sent_request(&h.recorder, "setLiveStoryMessageSender")["message_sender_id"]["chat_id"],
        -200
    );
    assert_eq!(h.driver.session.live_story_sender(), Some(channel));
}

#[test]
fn leaving_drops_pending_stars_and_the_call() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.add_live_story_reaction(1, 0).unwrap();
    h.driver.leave_live_story().unwrap();
    assert_eq!(h.sent("removePendingLiveStoryReactions"), 1);
    assert_eq!(h.sent("leaveGroupCall"), 1);
    assert!(h.driver.session.stories.live.is_none());
    assert!(h.driver.session.calls.story_stream.is_none());
    assert_eq!(h.sent("commitPendingLiveStoryReactions"), 0);
}

#[test]
fn a_looked_up_call_does_not_replace_the_joined_story() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.ingest(
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"title":"Other","is_active":true,"is_video_chat":true,"is_joined":false}}"#,
    );
    assert_eq!(story_id(&h), Some(4242));
    assert_eq!(current_id(&h), Some(555));
}

/// Another live story, (7, 8) on call 4343.
const OTHER_LIVE_CALL: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":4343,"title":"Other live","is_active":true,"is_video_chat":true,"is_live_story":true,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"participant_count":3,"can_send_messages":true,"are_messages_allowed":true}}"#;

fn add_other_live_story(h: &mut Harness) {
    h.ingest(
        r#"{"@type":"story","id":8,"poster_chat_id":7,"date":1700000000,"content":{"@type":"storyContentLive","group_call_id":4343,"is_rtmp_stream":false},"caption":{"@type":"formattedText","text":"","entities":[]}}"#,
    );
}

fn story_id(h: &Harness) -> Option<i32> {
    h.driver.session.calls.story_stream.as_ref().map(|c| c.id)
}

fn current_id(h: &Harness) -> Option<i32> {
    h.driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .map(|c| c.id)
}

#[test]
fn stepping_past_a_live_story_before_its_call_arrives_joins_the_next() {
    let mut h = harness();
    h.ingest(
        r#"{"@type":"story","id":6,"poster_chat_id":7,"date":1700000000,"content":{"@type":"storyContentLive","group_call_id":4242,"is_rtmp_stream":false},"caption":{"@type":"formattedText","text":"","entities":[]}}"#,
    );
    add_other_live_story(&mut h);
    // Open (7, 6), step past it before `getGroupCall` answers.
    h.driver.join_live_story(ChatId(7), 6).unwrap();
    h.driver.leave_live_story().unwrap();
    h.driver.join_live_story(ChatId(7), 8).unwrap();
    // The late answer for the first call is not tracked.
    h.ingest(&LIVE_CALL.replace("JOINED", "false"));
    assert_eq!(story_id(&h), None);
    assert_eq!(current_id(&h), None);
    h.ingest(OTHER_LIVE_CALL);
    assert_eq!(story_id(&h), Some(4343));
    assert_eq!(h.sent("joinLiveStory"), 1);
    assert_eq!(
        sent_request(&h.recorder, "joinLiveStory")["group_call_id"],
        4343
    );
    // Nothing was joined, so nothing was left.
    assert_eq!(h.sent("leaveGroupCall"), 0);
}

#[test]
fn a_looked_at_video_chat_stays_beside_a_live_story() {
    let mut h = watching();
    h.driver.leave_live_story().unwrap();
    // Joining (the request is out): left with `leaveGroupCall`.
    assert_eq!(h.sent("leaveGroupCall"), 1);
    // A video chat you only looked at.
    h.ingest(
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"title":"Chat","is_active":true,"is_video_chat":true,"is_joined":false}}"#,
    );
    assert_eq!(current_id(&h), Some(555));
    add_other_live_story(&mut h);
    h.driver.join_live_story(ChatId(7), 8).unwrap();
    h.ingest(OTHER_LIVE_CALL);
    assert_eq!(story_id(&h), Some(4343));
    assert_eq!(current_id(&h), Some(555), "the looked-at chat stays");
    assert_eq!(h.sent("leaveGroupCall"), 1, "nothing more to leave");
}

#[test]
fn a_scheduled_update_does_not_replace_a_joined_video_chat() {
    let mut h = harness();
    h.ingest(
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"title":"Chat","is_active":true,"is_video_chat":true,"is_joined":true}}"#,
    );
    h.ingest(
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":556,"title":"Later","is_active":false,"scheduled_start_date":1900000000,"is_video_chat":true,"is_joined":false}}"#,
    );
    assert_eq!(current_id(&h), Some(555));
}

#[test]
fn another_live_call_does_not_replace_the_watched_one() {
    let mut h = watching();
    // Not joined yet, and an update for another live story's call lands.
    h.ingest(OTHER_LIVE_CALL);
    assert_eq!(story_id(&h), Some(4242));
    assert_eq!(
        current_id(&h),
        None,
        "a live story is never the current call"
    );
    // Nor a scheduled call, nor a looked-up video chat.
    h.ingest(
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":556,"title":"Later","is_active":false,"scheduled_start_date":1900000000,"is_video_chat":true,"is_joined":false}}"#,
    );
    h.ingest(
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"title":"Chat","is_active":true,"is_video_chat":true,"is_joined":false}}"#,
    );
    assert_eq!(story_id(&h), Some(4242));
}

#[test]
fn pending_star_reactions_count_against_a_paid_comment() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.session.payments.hub.balance = Some(crate::premium_hub::StarAmount {
        stars: 60,
        nanos: 0,
    });
    h.driver.add_live_story_reaction(20, 0).unwrap();
    // 60 less 20 waiting leaves 40: a 50 Star comment needs a top-up.
    assert!(h.driver.session.live_needs_top_up(50));
    assert_invalid(h.driver.send_live_story_comment("hi", 50));
    assert_eq!(h.sent("sendGroupCallMessage"), 0);
    h.driver.send_live_story_comment("hi", 40).unwrap();
}

#[test]
fn the_stars_total_holds_between_the_commit_and_the_donors() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.ingest(
        r#"{"@type":"updateLiveStoryTopDonors","group_call_id":4242,"donors":{"@type":"liveStoryDonors","total_star_count":100,"top_donors":[]}}"#,
    );
    h.driver.add_live_story_reaction(5, 0).unwrap();
    let total = |h: &Harness| {
        h.driver
            .session
            .stories
            .live
            .as_ref()
            .map(|l| l.stars_total())
    };
    assert_eq!(total(&h), Some(105));
    assert!(h.driver.live_story_reaction_tick(ACCUMULATE_MS));
    assert_eq!(total(&h), Some(105), "no dip after the commit");
    h.ingest(
        r#"{"@type":"updateLiveStoryTopDonors","group_call_id":4242,"donors":{"@type":"liveStoryDonors","total_star_count":105,"top_donors":[]}}"#,
    );
    assert_eq!(total(&h), Some(105));
}

#[test]
fn comments_send_and_keep_their_formatting() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver
        .send_live_story_comment("**bold** `code` ||secret||", 0)
        .unwrap();
    let sent = sent_request(&h.recorder, "sendGroupCallMessage");
    assert_eq!(sent["text"]["text"], "bold code secret");
    let kinds: Vec<String> = sent["text"]["entities"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|e| e["type"]["@type"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // Code is not allowed in a comment (tdesktop `allowMarkdownTags`).
    assert_eq!(kinds, vec!["textEntityTypeBold", "textEntityTypeSpoiler"]);

    h.ingest(
        r#"{"@type":"updateNewGroupCallMessage","group_call_id":4242,"message":{"@type":"groupCallMessage","message_id":9,"sender_id":{"@type":"messageSenderUser","user_id":7},"date":1700000000,"text":{"@type":"formattedText","text":"hi 😀","entities":[{"@type":"textEntity","offset":0,"length":2,"type":{"@type":"textEntityTypeItalic"}},{"@type":"textEntity","offset":3,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"77"}}]},"paid_message_star_count":0,"is_from_owner":false,"can_be_deleted":true}}"#,
    );
    let entities = h
        .driver
        .session
        .calls
        .story_stream
        .as_ref()
        .and_then(|c| c.messages.first())
        .map(|m| m.entities.len());
    assert_eq!(entities, Some(2));
    // The custom emoji's sticker is looked up.
    assert_eq!(
        sent_request(&h.recorder, "getCustomEmojiStickers")["custom_emoji_ids"],
        serde_json::json!(["77"])
    );
}

#[test]
fn ban_user_bans_from_the_channel_or_blocks() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    // Only on your own live story.
    assert_invalid(
        h.driver
            .ban_live_story_commenter(MessageSender::User { user_id: 9 }),
    );
    h.ingest(&LIVE_CALL.replace("JOINED", "true").replace(
        r#""can_send_messages""#,
        r#""can_be_managed":true,"can_send_messages""#,
    ));
    // Chat 7 is a private chat: the user is blocked.
    h.driver
        .ban_live_story_commenter(MessageSender::User { user_id: 9 })
        .unwrap();
    assert_eq!(
        sent_request(&h.recorder, "setMessageSenderBlockList")["sender_id"]["user_id"],
        9
    );
    assert_invalid(
        h.driver
            .ban_live_story_commenter(MessageSender::Chat { chat_id: -5 }),
    );
}

const LOOKED_AT_CHAT: &str = r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"title":"Chat","is_active":true,"is_video_chat":true,"is_joined":false,"need_rejoin":NEED}}"#;

#[test]
fn a_reconnecting_video_chat_stays_beside_a_live_story() {
    let mut h = watching();
    h.driver.leave_live_story().unwrap();
    // Network loss: TDLib reports the chat as not joined, needing a rejoin.
    h.ingest(&LOOKED_AT_CHAT.replace("NEED", "true"));
    assert!(h.driver.session.group_call_joined_or_joining(None));
    assert_ne!(
        h.driver.session.current_call_for_create(None),
        crate::calls::group_call_create::CurrentCall::None,
        "it is still the call a new join asks about"
    );
    add_other_live_story(&mut h);
    h.driver.join_live_story(ChatId(7), 8).unwrap();
    h.ingest(OTHER_LIVE_CALL);
    assert_eq!(current_id(&h), Some(555));
    assert_eq!(story_id(&h), Some(4343));
}

#[test]
fn a_video_chat_beside_a_live_story_keeps_its_transport() {
    let mut h = watching();
    h.driver.leave_live_story().unwrap();
    let mock = crate::calls::engine::MockEngine::new();
    let handle = mock.clone();
    h.driver.set_call_engine(Box::new(mock));
    h.ingest(&LOOKED_AT_CHAT.replace("NEED", "false"));
    let key = crate::calls::live_video::LiveKey::Video {
        call_id: 555,
        user_id: 42,
        screen: false,
    };
    // 42's camera is on, and the call window asks for its feed.
    h.ingest(&group_participant_json(42, false, CAMERA_INFO, "null"));
    let camera = h.driver.live_sink(key).unwrap();
    handle.emit_live_frame(
        key,
        crate::calls::live_video::LiveFrame::new(
            0,
            2,
            2,
            crate::calls::live_video::LiveRotation::None,
            vec![0; 6],
            std::time::Instant::now(),
        )
        .unwrap(),
    );
    assert!(camera.latest().is_some());
    add_other_live_story(&mut h);
    h.driver.join_live_story(ChatId(7), 8).unwrap();
    h.ingest(OTHER_LIVE_CALL);
    assert_eq!(current_id(&h), Some(555));
    assert!(
        handle.group_leaves().is_empty(),
        "the native call keeps going"
    );
    assert_eq!(camera.snapshot().end, None);
    assert!(h.driver.live_sink(key).is_some());
}

#[test]
fn a_late_joined_update_for_the_call_just_left_does_not_return() {
    let mut h = watching();
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    h.driver.leave_live_story().unwrap();
    add_other_live_story(&mut h);
    h.driver.join_live_story(ChatId(7), 8).unwrap();
    // An update sent before TDLib saw the leave.
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    assert_eq!(story_id(&h), None);
    assert_eq!(current_id(&h), None);
    h.ingest(OTHER_LIVE_CALL);
    assert_eq!(story_id(&h), Some(4343));
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
    assert_eq!(story_id(&h), Some(4343), "the watched story keeps it");
    assert_eq!(current_id(&h), None);
    // TDLib confirms the leave; the call counts as any other again.
    h.ingest(&LIVE_CALL.replace("JOINED", "false"));
    assert_eq!(h.driver.session.calls.left_group_call, None);
}

#[test]
fn a_scheduled_video_chat_stays_beside_a_live_story() {
    let mut h = harness();
    h.ingest(
        r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":556,"title":"Later","is_active":false,"scheduled_start_date":1900000000,"is_video_chat":true,"is_joined":false}}"#,
    );
    assert_eq!(current_id(&h), Some(556));
    add_other_live_story(&mut h);
    h.driver.join_live_story(ChatId(7), 8).unwrap();
    h.ingest(OTHER_LIVE_CALL);
    assert_eq!(current_id(&h), Some(556));
    assert_eq!(story_id(&h), Some(4343));
}

fn joined_answer(h: &mut Harness) {
    h.answer(
        "joinLiveStory",
        r#"{"@type":"text","text":"{\"stream\":true}"}"#,
    );
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
}

fn rejoin(h: &mut Harness) {
    if let Some(live) = h.driver.session.stories.live.as_mut() {
        live.extras_requested = false;
    }
    h.ingest(&LIVE_CALL.replace("JOINED", "true"));
}

#[test]
fn the_first_join_asks_for_the_star_bursts_once() {
    let mut h = watching();
    assert_eq!(h.sent("getCustomEmojiReactionAnimations"), 0);
    joined_answer(&mut h);
    assert_eq!(h.sent("getCustomEmojiReactionAnimations"), 1);
    assert_eq!(
        h.driver.session.stories.live_bursts,
        crate::state::LiveBursts::Requested
    );
    h.answer(
        "getCustomEmojiReactionAnimations",
        r#"{"@type":"stickers","stickers":[{"@type":"sticker","id":"77","set_id":"5","width":512,"height":512,"emoji":"✨","format":{"@type":"stickerFormatTgs"},"full_type":{"@type":"stickerFullTypeRegular","premium_animation":null},"thumbnail":null,"sticker":{"@type":"file","id":901,"size":2000,"expected_size":2000,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":2000}}}]}"#,
    );
    let bursts = h.driver.session.stories.live_bursts.stickers();
    assert_eq!(
        bursts.iter().map(|s| s.file_id.0).collect::<Vec<_>>(),
        vec![901]
    );
    rejoin(&mut h);
    assert_eq!(h.sent("getLiveStoryTopDonors"), 2);
    assert_eq!(h.sent("getCustomEmojiReactionAnimations"), 1);
}

#[test]
fn a_failed_burst_request_is_asked_again_on_the_next_join() {
    let mut h = watching();
    joined_answer(&mut h);
    h.answer(
        "getCustomEmojiReactionAnimations",
        r#"{"@type":"error","code":500,"message":"x"}"#,
    );
    assert_eq!(
        h.driver.session.stories.live_bursts,
        crate::state::LiveBursts::Unasked
    );
    rejoin(&mut h);
    assert_eq!(h.sent("getCustomEmojiReactionAnimations"), 2);
}
