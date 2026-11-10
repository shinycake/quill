//! Where a channel's ads join its history (tdesktop
//! `Data::SponsoredMessages::append` / `inject` / `clearItems`).

use super::common::{AtomicU64, MemorySink, apply_json, session};
use super::{Arc, ChatId, MessageContent, MessageId, ParsedMessage, RequestPurpose, Session};
use crate::state::{SPONSORED_REFETCH_AFTER, SponsoredLayout, SponsoredPlacement};
use std::time::Instant;

fn post(chat: i64, id: i64) -> ParsedMessage {
    ParsedMessage {
        sender: None,
        id: MessageId(id),
        chat_id: ChatId(chat),
        date: 0,
        is_outgoing: false,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: None,
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text("post".into()),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    }
}

/// Channel 1 open with posts 1..=5 and a list of two text ads.
fn channel(between: i32) -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_chat(ChatId(1));
    for id in 1..=5 {
        session.upsert_message(post(1, id), false);
    }
    answer(&mut session, &seq, &sink, between);
    (session, sink, seq)
}

fn answer(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, between: i32) {
    let ad = |id: i64| {
        format!(
            r#"{{"@type":"sponsoredMessage","message_id":{id},"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"ad","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":""}},"title":"T","button_text":"Go","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}"#
        )
    };
    let extra = session.request(RequestPurpose::GetChatSponsoredMessages, Some(ChatId(1)));
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":{between},"messages":[{},{}]}}"#,
            extra.0,
            ad(9001),
            ad(9002)
        ),
    );
}

fn shown(session: &Session) -> Vec<(i64, i64)> {
    session
        .open_sponsored_in_history()
        .iter()
        .map(|(after, ad)| (after.0, ad.message_id))
        .collect()
}

#[test]
fn an_ad_joins_the_end_only_when_the_reader_scrolls_past_it() {
    let (mut session, _sink, _seq) = channel(0);
    let now = Instant::now();
    assert_eq!(
        session.open_sponsored_layout(),
        Some(SponsoredLayout::AppendAtEnd)
    );
    assert!(shown(&session).is_empty(), "no ad before the reader asks");
    assert!(session.sponsored_unshown_at_end(ChatId(1), now));

    assert!(session.place_sponsored_at_end(now));
    assert_eq!(shown(&session), vec![(5, 9001)]);
    assert!(
        session.sponsored_unshown_at_end(ChatId(1), now),
        "the second ad still waits"
    );

    // A new post lands under the ad; the next one joins after it.
    session.upsert_message(post(1, 6), false);
    assert!(session.place_sponsored_at_end(now));
    assert_eq!(shown(&session), vec![(5, 9001), (6, 9002)]);
    assert!(!session.sponsored_unshown_at_end(ChatId(1), now));
    assert!(!session.place_sponsored_at_end(now), "every ad is shown");
}

#[test]
fn a_list_older_than_five_minutes_adds_nothing() {
    let (mut session, _sink, _seq) = channel(0);
    let later = Instant::now() + SPONSORED_REFETCH_AFTER;
    assert!(!session.sponsored_unshown_at_end(ChatId(1), later));
    assert!(!session.place_sponsored_at_end(later));
}

#[test]
fn leaving_the_channel_takes_its_ads_out_of_the_history() {
    let (mut session, _sink, _seq) = channel(0);
    let now = Instant::now();
    assert!(session.place_sponsored_at_end(now));
    session.open_chat(ChatId(2));
    session.open_chat(ChatId(1));
    assert!(shown(&session).is_empty());
    assert!(
        session.place_sponsored_at_end(now),
        "placed again on return"
    );
    assert_eq!(shown(&session), vec![(5, 9001)]);
    session.leave_open_chat();
    session.open_chat(ChatId(1));
    assert!(shown(&session).is_empty());
}

#[test]
fn a_refetch_keeps_the_ads_already_placed() {
    let (mut session, sink, seq) = channel(0);
    assert!(session.place_sponsored_at_end(Instant::now()));
    answer(&mut session, &seq, &sink, 0);
    assert_eq!(shown(&session), vec![(5, 9001)]);
}

#[test]
fn ads_between_posts_count_from_the_first_unread_post() {
    let (mut session, _sink, _seq) = channel(3);
    assert_eq!(
        session.open_sponsored_layout(),
        Some(SponsoredLayout::BetweenPosts(3))
    );
    assert!(!session.place_sponsored_at_end(Instant::now()), "no end ad");
    assert_eq!(session.sponsored_inject_start(), None, "nothing unread");
    if let Some(history) = session.histories.get_mut(&1) {
        history.unread_anchor = Some(MessageId(2));
    }
    assert_eq!(
        session.sponsored_inject_start(),
        Some((MessageId(3), false))
    );
    assert!(session.place_sponsored_after(MessageId(5)));
    assert_eq!(session.sponsored_inject_start(), Some((MessageId(5), true)));
    assert!(session.place_sponsored_after(MessageId(5)));
    assert_eq!(session.sponsored_inject_start(), None, "every ad placed");
    let placed = session
        .messages
        .sponsored
        .get(&1)
        .map(|entry| entry.placed.clone());
    assert_eq!(
        placed,
        Some(vec![
            SponsoredPlacement {
                ad_id: 9001,
                after: MessageId(5)
            },
            SponsoredPlacement {
                ad_id: 9002,
                after: MessageId(5)
            },
        ])
    );
}
