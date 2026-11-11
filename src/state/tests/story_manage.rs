//! Story management calls (`editStory`, the cover, the privacy editor) are
//! each answered to their own sender, whatever call took the status slot.
use super::common::*;
use super::*;
use crate::state::StoryManageOutcome;

fn error(request: crate::ids::RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"STORY_NOT_EDITABLE"}}"#,
        request.0
    )
}

/// The cover call is replaced in the slot by the privacy call, then fails:
/// the cover panel still hears its own failure, and the privacy call keeps
/// the slot.
#[test]
fn a_replaced_call_still_gets_its_own_answer() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let cover = session.request_for_story(RequestPurpose::EditStoryCover, ChatId(11), 5);
    session.stories.manage.start(cover, ChatId(11), 5);
    let privacy = session.request_for_story(RequestPurpose::SetStoryPrivacySettings, ChatId(11), 5);
    session.stories.manage.start(privacy, ChatId(11), 5);
    apply_json(&mut session, &seq, &sink, &error(cover));
    let manage = &session.stories.manage;
    assert_eq!(manage.outcome(cover), StoryManageOutcome::Failed);
    assert!(manage.failure(cover).is_some());
    assert_eq!(manage.outcome(privacy), StoryManageOutcome::InFlight);
    assert!(manage.pending_for(11, 5), "the privacy call holds the slot");
    assert_eq!(
        manage.error_for(11, 5),
        None,
        "the cover failure is not the privacy call's line"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, privacy.0),
    );
    let manage = &session.stories.manage;
    assert_eq!(manage.outcome(privacy), StoryManageOutcome::Saved);
    assert!(!manage.is_pending());
}

/// A call the slot let go of (another call failed locally) is still
/// settled by its answer for whoever sent it.
#[test]
fn a_call_let_go_of_by_the_slot_still_settles() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let edit = session.request_for_story(RequestPurpose::EditStory, ChatId(11), 5);
    session.stories.manage.start(edit, ChatId(11), 5);
    session
        .stories
        .manage
        .fail_locally(11, 5, "Pick at least one user".into());
    assert_eq!(
        session.stories.manage.outcome(edit),
        StoryManageOutcome::InFlight
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, edit.0),
    );
    assert_eq!(
        session.stories.manage.outcome(edit),
        StoryManageOutcome::Saved
    );
    assert_eq!(
        session
            .stories
            .manage
            .outcome(crate::ids::RequestId(99_999)),
        StoryManageOutcome::Lost
    );
}
