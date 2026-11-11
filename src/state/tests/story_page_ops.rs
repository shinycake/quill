//! Story-page calls are tracked one per request: a read and a mutation in
//! flight together never hide each other's answer.
use super::common::*;
use super::*;
use crate::story_page::{StoryPageOpState, story_page_op_label};

fn error(request: crate::ids::RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"ALBUM_NOT_FOUND"}}"#,
        request.0
    )
}

/// A delete answered with an error while the archive is paging still
/// fails, and the paging read keeps showing as loading.
#[test]
fn a_failed_delete_shows_while_a_read_is_in_flight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let delete = session.request_for_story_album(
        RequestPurpose::DeleteStoryAlbum,
        ChatId(11),
        None,
        Some(4),
    );
    session.begin_story_page_op(
        story_page_op_label(RequestPurpose::DeleteStoryAlbum),
        delete,
    );
    let read = session.request(RequestPurpose::GetChatArchivedStories, Some(ChatId(11)));
    session.begin_story_page_check(
        story_page_op_label(RequestPurpose::GetChatArchivedStories),
        read,
    );
    apply_json(&mut session, &seq, &sink, &error(delete));
    let ops = &session.stories.page_ops;
    assert!(matches!(
        ops.get(delete).map(|op| &op.state),
        Some(StoryPageOpState::Failed(_))
    ));
    assert!(ops.loading(
        11,
        &story_page_op_label(RequestPurpose::GetChatArchivedStories)
    ));
    assert!(matches!(
        ops.status_for(Some(11)).map(|op| &op.state),
        Some(StoryPageOpState::Failed(_))
    ));
}

/// A mutation started while a read is in flight does not take the read's
/// loading state with it.
#[test]
fn a_mutation_does_not_hide_the_loading_read() {
    let (mut session, _sink) = session();
    let label = story_page_op_label(RequestPurpose::GetChatPostedToChatPageStories);
    let read = session.request(
        RequestPurpose::GetChatPostedToChatPageStories,
        Some(ChatId(11)),
    );
    session.begin_story_page_check(label.clone(), read);
    let create = session.request(RequestPurpose::CreateStoryAlbum, Some(ChatId(11)));
    session.begin_story_page_op(
        story_page_op_label(RequestPurpose::CreateStoryAlbum),
        create,
    );
    let ops = &session.stories.page_ops;
    assert!(ops.loading(11, &label));
    assert!(ops.mutating(11));
    assert!(!ops.mutating(12), "another chat's page is not busy");
    assert!(!ops.loading(12, &label), "nor loading");
}

/// A request that could not be sent fails its own op and leaves the ones
/// in flight alone.
#[test]
fn a_failed_send_keeps_the_other_ops() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let read = session.request(RequestPurpose::GetChatArchivedStories, Some(ChatId(11)));
    session.begin_story_page_check(
        story_page_op_label(RequestPurpose::GetChatArchivedStories),
        read,
    );
    let lost = session.request(RequestPurpose::GetChatStoryAlbums, Some(ChatId(11)));
    session.fail_story_page_send(
        story_page_op_label(RequestPurpose::GetChatStoryAlbums),
        lost,
        Some(ChatId(11)),
        "could not send".into(),
    );
    assert_eq!(
        session.stories.page_ops.get(read).map(|op| &op.state),
        Some(&StoryPageOpState::Checking)
    );
    // The read still settles by its own answer.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stories","@extra":"{}","total_count":0,"stories":[]}}"#,
            read.0
        ),
    );
    assert!(session.stories.page_ops.get(read).is_none());
    assert!(session.stories.page_ops.get(lost).is_some());
}

/// Leaving the page drops what it was waiting on, so a call lost without
/// an answer cannot show "Sending" for good; the account going away
/// drops all.
#[test]
fn leaving_the_page_forgets_its_calls() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let pin = session.request(RequestPurpose::SetChatPinnedStories, Some(ChatId(11)));
    session.begin_story_page_op(
        story_page_op_label(RequestPurpose::SetChatPinnedStories),
        pin,
    );
    let other = session.request(RequestPurpose::SetChatPinnedStories, Some(ChatId(12)));
    session.begin_story_page_op(
        story_page_op_label(RequestPurpose::SetChatPinnedStories),
        other,
    );
    session.leave_story_page(11);
    assert!(session.stories.page_ops.get(pin).is_none());
    assert!(session.stories.page_ops.get(other).is_some());
    // A late answer for the forgotten call changes nothing.
    apply_json(&mut session, &seq, &sink, &error(pin));
    assert!(session.stories.page_ops.get(pin).is_none());
    session.settle_account_requests();
    assert!(session.stories.page_ops.is_empty());
}
