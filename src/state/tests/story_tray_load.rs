//! State tests: loading more of the story tray until TDLib says it is all
//! loaded, and waiting out a page that failed for any other reason.
use super::common::*;
use super::*;
use crate::state::STORIES_RETRY_FIRST;
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
fn a_404_from_load_active_stories_stops_loading_more() {
    let (mut session, sink) = session();
    let now = Instant::now();
    session.stories.stories_active_loaded = true;
    assert!(session.stories.rearm_active_stories_load(now));
    assert!(
        !session.stories.stories_active_loaded,
        "one more page may go"
    );
    fail_page(&mut session, &sink, 404);
    assert!(session.stories.stories_active_exhausted);
    assert!(!session.stories.rearm_active_stories_load(now));
}

#[test]
fn a_500_waits_and_retries_instead_of_ending_the_tray() {
    let (mut session, sink) = session();
    fail_page(&mut session, &sink, 500);
    assert!(!session.stories.stories_active_exhausted, "not the end");
    let (at, wait) = session.stories.stories_active_retry.expect("a retry");
    assert_eq!(wait, STORIES_RETRY_FIRST);
    let before = at
        .checked_sub(Duration::from_millis(1))
        .expect("a later instant");
    assert!(!session.stories.rearm_active_stories_load(before));
    assert!(
        session.stories.rearm_active_stories_load(at),
        "after the wait"
    );
    fail_page(&mut session, &sink, 420);
    let (_, wait) = session.stories.stories_active_retry.expect("a retry");
    assert_eq!(wait, STORIES_RETRY_FIRST * 2, "the wait doubles");
    session.stories.reset_active_stories_paging();
    assert_eq!(session.stories.stories_active_retry, None);
    assert!(session.stories.rearm_active_stories_load(Instant::now()));
}

#[test]
fn a_page_that_loads_ends_the_wait() {
    let (mut session, sink) = session();
    fail_page(&mut session, &sink, 500);
    assert!(session.stories.stories_active_retry.is_some());
    let seq = AtomicU64::new(session.last_seq);
    let extra = session.request(RequestPurpose::LoadActiveStories, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.stories.stories_active_retry, None);
    fail_page(&mut session, &sink, 500);
    let (_, wait) = session.stories.stories_active_retry.expect("a retry");
    assert_eq!(wait, STORIES_RETRY_FIRST, "the next failure starts over");
}
