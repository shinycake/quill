//! S14 (review fix-up): prove the actual feature behavior — a
//! `canPostStory` error answer surfaces the TGX-verbatim restriction
//! notice on `Session::story_post.check_error` through the public
//! reducer interface (`Session::request` + `Session::apply`). The unit
//! tests in `src/story_restriction.rs` only prove classification; this
//! proves the reducer arm (state.rs `CheckCanPostStory`) actually
//! surfaces the notice the composer renders.

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn session() -> (Session, Arc<MemorySink>) {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    (Session::new(AccountKey::primary(), dyn_sink), sink)
}

fn apply(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
    session.apply(owned);
}

/// A `canPostStory` error on a `CheckCanPostStory` request lands the
/// TGX-verbatim notice on `check_error` — both for the headline
/// client-side-gate message and the `CHAT_ADMIN_REQUIRED` stale-cache
/// race, and for `USER_RESTRICTED` (403).
#[test]
fn can_post_story_error_surfaces_tgx_notice() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (message, code, expected) in [
        (
            "CHAT_ADMIN_REQUIRED",
            400,
            "Only admins can send stories in this group",
        ),
        // Headline case: TDLib's `StoryManager::can_send_story`
        // client-side gate (pinned 1.8.67).
        (
            "Not enough rights to post stories in the chat",
            400,
            "Only admins can send stories in this group",
        ),
        (
            "USER_RESTRICTED",
            403,
            "The admins of this group have restricted your ability to send stories.",
        ),
    ] {
        let extra = session.request(RequestPurpose::CheckCanPostStory, None);
        apply(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":{code},"message":"{message}"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.stories.post.check_error.as_deref(),
            Some(expected),
            "message {message}"
        );
    }
}

/// An unrecognized `canPostStory` error keeps the generic eligibility
/// failure text — the S14 classifier does not change other flows.
#[test]
fn can_post_story_unknown_error_keeps_generic_failure() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::CheckCanPostStory, None);
    apply(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHANNEL_INVALID"}}"#,
            extra.0
        ),
    );
    let error = session.stories.post.check_error.expect("check error");
    assert!(error.starts_with("Eligibility check failed:"), "{error}");
}
