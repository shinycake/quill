//! Exit with TDLib clients open, closing and closed. Each case is a child
//! process re-running one test, since the exit itself is under test.
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::telegram::client::{LiveTdJson, copy_and_parse};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

const CHILD: &str = "QUILL_TEST_HUB_EXIT_CHILD";

/// The status the stand-in for TDLib's teardown ends the child with.
const TEARDOWN_RAN: i32 = 86;
/// The status the child gives `exit`.
const QUIT_STATUS: i32 = 7;

fn closed(client_id: i32) -> OwnedEnvelope {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let json = format!(
        r#"{{"@type":"updateAuthorizationState","authorization_state":{{"@type":"authorizationStateClosed"}},"@client_id":{client_id}}}"#
    );
    copy_and_parse(&json, &AtomicU64::new(0), &sink).unwrap()
}

fn open_ids() -> Vec<i32> {
    crate::locks::lock_or_recover(&MAY_BE_OPEN).clone()
}

#[test]
fn a_client_may_be_open_from_its_making_until_tdlib_says_closed() {
    note_made(9_001);
    note_made(9_002);
    assert!(open_ids().contains(&9_001));
    note_received(&closed(9_001));
    note_received(&closed(9_003));
    let open = open_ids();
    assert!(!open.contains(&9_001));
    assert!(
        open.contains(&9_002),
        "another client's Closed is not its own"
    );
    assert!(tdlib_may_be_busy());
}

#[test]
fn only_the_answer_for_a_client_never_made_counts_as_a_wake() {
    let answer = r#"{"@type":"error","code":400,"message":"Invalid TDLib instance specified"}"#;
    assert!(wakes(Some(answer)));
    assert!(!wakes(None), "nothing came back in time");
    assert!(!wakes(Some(r#"{"@type":"updateOption","@client_id":0}"#)));
}

/// Stands in for TDLib's static destructors, which abort while a client
/// runs: registered before the exit hook, it runs after it.
extern "C" fn teardown() {
    // SAFETY: `_exit` takes a plain status and never returns.
    unsafe { libc::_exit(TEARDOWN_RAN) }
}

#[expect(
    clippy::exit,
    reason = "the re-executed child test process is the exit under test"
)]
fn fake_child_main(mode: &str) -> ! {
    // SAFETY: `teardown` is a plain `extern "C" fn()` that never unwinds.
    unsafe { libc::atexit(teardown) };
    note_made(1);
    register_exit_hook();
    if mode == "closed" {
        note_received(&closed(1));
    }
    set_exit_status(QUIT_STATUS);
    std::process::exit(QUIT_STATUS)
}

fn run_child(test: &str, mode: &str) -> std::process::ExitStatus {
    Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", test])
        .env(CHILD, mode)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
}

const FAKE_CHILD: &str = "telegram::hub::exit::tests::fake_exit_child";

#[test]
#[ignore = "the child of the exit tests below"]
fn fake_exit_child() {
    if let Ok(mode) = std::env::var(CHILD) {
        fake_child_main(&mode);
    }
}

#[test]
fn exit_with_a_client_open_skips_the_teardown_with_the_quit_status() {
    assert_eq!(run_child(FAKE_CHILD, "open").code(), Some(QUIT_STATUS));
}

#[test]
fn exit_with_every_client_closed_runs_the_teardown() {
    assert_eq!(run_child(FAKE_CHILD, "closed").code(), Some(TEARDOWN_RAN));
}

#[expect(
    clippy::exit,
    reason = "the re-executed child test process is the exit under test"
)]
fn live_child_main(mode: &str) -> ! {
    let Ok(live) = LiveTdJson::connect(Arc::new(MemorySink::new())) else {
        std::process::exit(3);
    };
    let bridge = live.bridge();
    match mode {
        "closing" => {
            let _ = live.send(r#"{"@type":"close"}"#);
            std::process::exit(0);
        }
        "open" => {
            // Any answer means TDLib made the client and runs it.
            let _ = live.send(r#"{"@type":"getOption","name":"version","@extra":"made"}"#);
            if bridge.rx.recv().is_ok() {
                std::process::exit(0);
            }
            std::process::exit(4);
        }
        _ => {
            let _ = live.send(r#"{"@type":"close"}"#);
            while let Ok(owned) = bridge.rx.recv() {
                if matches!(
                    owned.envelope.payload,
                    EnvelopePayload::Auth(AuthPayload::UpdateAuthorizationState(
                        AuthorizationState::Closed
                    ))
                ) {
                    std::process::exit(0);
                }
            }
            std::process::exit(4);
        }
    }
}

const LIVE_CHILD: &str = "telegram::hub::exit::tests::live_exit_child";

#[test]
#[ignore = "the child of the live exit tests below"]
fn live_exit_child() {
    if let Ok(mode) = std::env::var(CHILD) {
        live_child_main(&mode);
    }
}

/// Exits `QUILL_TEST_HUB_EXIT_RUNS` children in `mode` and counts the
/// unclean ones.
fn live_exits(mode: &str) {
    let runs: u32 = std::env::var("QUILL_TEST_HUB_EXIT_RUNS")
        .ok()
        .and_then(|runs| runs.parse().ok())
        .unwrap_or(200);
    let mut unclean = 0;
    let mut slowest = Duration::ZERO;
    for _ in 0..runs {
        let started = Instant::now();
        let status = run_child(LIVE_CHILD, mode);
        slowest = slowest.max(started.elapsed());
        if status.code() != Some(0) {
            eprintln!("{status}");
            unclean += 1;
        }
    }
    eprintln!("{mode}: {runs} exits, {unclean} unclean, slowest {slowest:?}");
    assert_eq!(unclean, 0);
}

#[test]
#[ignore = "QUILL_TDJSON_PATH=<libtdjson> cargo test --lib live_exit_after -- --ignored --nocapture"]
fn live_exit_after_close_stays_clean() {
    live_exits("closed");
}

#[test]
#[ignore = "QUILL_TDJSON_PATH=<libtdjson> cargo test --lib live_exit_with -- --ignored --nocapture"]
fn live_exit_with_a_client_open_stays_clean() {
    live_exits("open");
}

#[test]
#[ignore = "QUILL_TDJSON_PATH=<libtdjson> cargo test --lib live_exit_with -- --ignored --nocapture"]
fn live_exit_with_a_close_in_flight_stays_clean() {
    live_exits("closing");
}
