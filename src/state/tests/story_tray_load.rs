//! State tests: loading more of the story tray until TDLib says it is all
//! loaded.
use super::common::*;
use super::*;

#[test]
fn a_404_from_load_active_stories_stops_loading_more() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.stories.stories_active_loaded = true;
    assert!(session.stories.rearm_active_stories_load());
    assert!(
        !session.stories.stories_active_loaded,
        "one more page may go"
    );
    let extra = session.request(RequestPurpose::LoadActiveStories, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"Not Found"}}"#,
            extra.0
        ),
    );
    assert!(session.stories.stories_active_exhausted);
    assert!(!session.stories.rearm_active_stories_load());
}
