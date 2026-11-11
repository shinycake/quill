//! State tests: loading more of the story tray until TDLib says it is all
//! loaded, and waiting out a page that failed for any other reason.
use super::common::*;
use super::*;
use crate::state::STORIES_RETRY_FIRST;
use crate::telegram::envelope::StoryListView;
use std::time::{Duration, Instant};

fn fail_page(session: &mut Session, sink: &Arc<MemorySink>, code: i32) {
    let seq = AtomicU64::new(session.last_seq);
    let extra = session.request(RequestPurpose::LoadActiveStories, None);
    apply_json(
        session,
        &seq,
        sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":{code},"message":"x"}}"#,
            extra.0
        ),
    );
}

#[test]
fn a_404_ends_the_list_and_only_that_list() {
    let (mut session, sink) = session();
    fail_page(&mut session, &sink, 404);
    assert!(
        !session
            .stories
            .paging(StoryListView::Main)
            .may_load(Instant::now())
    );
    assert!(
        session
            .stories
            .paging(StoryListView::Archive)
            .may_load(Instant::now())
    );
}

#[test]
fn a_500_waits_and_retries_instead_of_ending_the_list() {
    let (mut session, sink) = session();
    fail_page(&mut session, &sink, 500);
    let paging = *session.stories.paging(StoryListView::Main);
    assert_ne!(paging, ActiveStoriesPaging::Exhausted, "not the end");
    let (at, wait) = paging.retry().expect("a retry");
    assert_eq!(wait, STORIES_RETRY_FIRST);
    let before = at
        .checked_sub(Duration::from_millis(1))
        .expect("a later instant");
    assert!(!paging.may_load(before));
    assert!(paging.may_load(at), "after the wait");
    fail_page(&mut session, &sink, 420);
    let (_, wait) = session.stories.main_paging.retry().expect("a retry");
    assert_eq!(wait, STORIES_RETRY_FIRST * 2, "the wait doubles");
    session.stories.reset_active_stories_paging();
    assert_eq!(session.stories.main_paging.retry(), None);
}

#[test]
fn a_page_that_loads_ends_the_wait() {
    let (mut session, sink) = session();
    fail_page(&mut session, &sink, 500);
    assert!(session.stories.main_paging.retry().is_some());
    let seq = AtomicU64::new(session.last_seq);
    let extra = session.request(RequestPurpose::LoadActiveStories, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.stories.main_paging.retry(), None);
    fail_page(&mut session, &sink, 500);
    let (_, wait) = session.stories.main_paging.retry().expect("a retry");
    assert_eq!(wait, STORIES_RETRY_FIRST, "the next failure starts over");
}

#[test]
fn an_error_for_the_story_emoji_fetch_is_counted_and_backs_off() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(session.last_seq);
    let extra = session.request(RequestPurpose::GetStoryCustomEmojiStickers, None);
    let before = Instant::now();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"x"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.stories.custom_emoji_fetch.failures(), 1);
    assert!(session.stories.custom_emoji_fetch_waiting(before));
    assert!(
        !session
            .stories
            .custom_emoji_fetch_waiting(before + STORIES_RETRY_FIRST + Duration::from_secs(1))
    );
    session.accept_story_custom_emoji_stickers(Vec::new());
    assert!(!session.stories.custom_emoji_fetch_waiting(before));
}

/// The story picker's fetch is one stage: failures count up and back off,
/// and an answer returns it to idle with no failures.
#[test]
fn a_custom_emoji_answer_ends_the_backoff_and_the_failure_count() {
    let (mut session, _sink) = session();
    let now = std::time::Instant::now();
    session.stories.custom_emoji_fetch_failed(now);
    session.stories.custom_emoji_fetch_failed(now);
    assert_eq!(session.stories.custom_emoji_fetch.failures(), 2);
    assert!(session.stories.custom_emoji_fetch_waiting(now));
    session.accept_story_custom_emoji_stickers(Vec::new());
    assert_eq!(session.stories.custom_emoji_fetch.failures(), 0);
    assert!(!session.stories.custom_emoji_fetch_waiting(now));
}

/// A 404 ends the list for good: later failures do not turn it back into a
/// wait, and a page that loads does not either.
#[test]
fn an_exhausted_list_stays_exhausted() {
    let mut paging = ActiveStoriesPaging::default();
    let now = std::time::Instant::now();
    paging.failed(404, now);
    paging.failed(500, now);
    assert_eq!(paging, ActiveStoriesPaging::Exhausted);
    paging.answered();
    assert!(!paging.may_load(now + STORIES_RETRY_MAX));
}
