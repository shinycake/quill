//! The host against stand-in helpers: `true`, and the scripts in
//! `tests/fixtures/carrier-helper` (ones that wait for the close command,
//! exit like a missing library, say `unsupported`, post a page message, or
//! ignore everything).
use super::*;
use std::sync::mpsc::Receiver;

fn request(id: i32) -> CarrierRequest {
    CarrierRequest {
        id,
        proxy_id: 1,
        url: super::super::CarrierUrl::new("https://proxy.example.com/?bridge=x".into()),
        origin: "https://proxy.example.com".into(),
        path_prefix: "/".into(),
        bridge_object_name: "TelegramWebProxy".into(),
        temporary: false,
    }
}

fn host(program: Option<PathBuf>) -> (HelperCarrierHost, Receiver<String>) {
    host_with(program, None)
}

fn host_with(
    program: Option<PathBuf>,
    data_dir: Option<PathBuf>,
) -> (HelperCarrierHost, Receiver<String>) {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let relay: RelaySink = Arc::new(move |request: &str| {
        tx.lock()
            .map(|tx| tx.send(request.to_string()).is_ok())
            .unwrap_or(false)
    });
    let host = HelperCarrierHost::new(HelperConfig {
        program,
        data_dir,
        probe_cache: None,
        relay,
        capability: Capability::default(),
    });
    (host, rx)
}

/// A stand-in helper from `tests/fixtures/carrier-helper`, checked in with
/// its executable bit. Never written by a test: a script written and then
/// started at once failed now and then with `ETXTBSY` (the open was
/// `Terminated`), because a child another test thread was starting at that
/// moment had inherited the write descriptor and held it until its exec.
fn stand_in(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/carrier-helper")
        .join(name)
}

/// The first failure the carrier's threads report (what the next `poll`
/// returns), waited for on the host's channel instead of polled against a
/// clock. The host's own sender is dropped, so if no report comes the wait
/// ends, and the test fails, once every carrier thread has ended: the
/// reader at the helper's exit, the writer at its first failed write or at
/// the latest at its start watchdog ([`START_TIMEOUT`]).
fn next_failure(host: &mut HelperCarrierHost) -> (i32, CarrierError) {
    host.failures_tx = channel().0;
    host.failures
        .recv()
        .expect("carrier threads ended without reporting a failure")
}

/// Every failure reported until all carrier threads have ended, for a host
/// whose carriers are all closed. With the host's own sender dropped, the
/// channel closes with the last thread's, and each ends on an event: the
/// writer once it sent the close, the reader once the helper's output ends
/// (at the latest when the helper is killed after its grace).
fn reports_until_threads_end(host: &mut HelperCarrierHost) -> Vec<(i32, CarrierError)> {
    host.failures_tx = channel().0;
    host.failures.iter().collect()
}

/// A span no test outlives: as a ceiling or a grace, a wait for helpers
/// ends only once they are gone, never at its end.
const LONGER_THAN_ANY_TEST: Duration = Duration::from_secs(24 * 60 * 60);

/// What [`CarrierHost::shutdown`] does (`stop(true)`), and what its wait
/// for exiting helpers came to.
fn shutdown_outcome(host: &mut HelperCarrierHost) -> Waited {
    host.stop(true)
}

/// The process id of carrier `carrier_id`'s helper.
fn helper_pid(host: &HelperCarrierHost, carrier_id: i32) -> u32 {
    let running = host.carriers.get(&carrier_id).unwrap();
    let child = running.shared.child.lock().unwrap();
    child.as_ref().map(Child::id).unwrap()
}

/// Whether carrier `carrier_id`'s helper process is still running.
fn helper_running(host: &HelperCarrierHost, carrier_id: i32) -> bool {
    let Some(running) = host.carriers.get(&carrier_id) else {
        return false;
    };
    let Ok(mut child) = running.shared.child.lock() else {
        return false;
    };
    child
        .as_mut()
        .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
}

#[test]
fn no_program_hosts_nothing() {
    let (mut host, _) = host(None);
    assert!(!host.available());
    assert_eq!(host.open(&request(1)), Err(CarrierError::Unsupported));
}

#[test]
fn a_helper_that_cannot_start_lets_tdlib_try_again() {
    let (mut host, _) = host(Some("/definitely/not/quill-webview".into()));
    assert!(host.available());
    assert_eq!(host.open(&request(1)), Err(CarrierError::Terminated));
    assert!(host.available());
}

#[test]
fn a_helper_that_exits_before_ready_is_terminated() {
    // `true` reads nothing and exits 0: a crash, not a missing library.
    let (mut host, _) = host(Some("/usr/bin/true".into()));
    assert_eq!(host.open(&request(4)), Ok(()));
    assert_eq!(next_failure(&mut host), (4, CarrierError::Terminated));
    assert!(host.available());
}

#[test]
fn a_missing_web_view_library_turns_hosting_off() {
    // It exits with 127, as the dynamic loader does when a library is
    // missing.
    let (mut host, _) = host(Some(stand_in("missing-library.sh")));
    assert_eq!(host.open(&request(4)), Ok(()));
    assert_eq!(next_failure(&mut host), (4, CarrierError::Unsupported));
    assert!(!host.available());
    assert_eq!(host.open(&request(5)), Err(CarrierError::Unsupported));
}

#[test]
fn a_helper_saying_unsupported_turns_hosting_off() {
    let (mut host, _) = host(Some(stand_in("says-unsupported.sh")));
    assert_eq!(host.open(&request(4)), Ok(()));
    assert_eq!(next_failure(&mut host), (4, CarrierError::Unsupported));
    assert!(!host.available());
}

#[test]
fn page_messages_go_straight_to_tdlib() {
    let (mut host, relayed) = host(Some(stand_in("posts.sh")));
    assert_eq!(host.open(&request(7)), Ok(()));
    // Only the carrier's reader holds the relay now: had the message not
    // come, the wait would end with that thread (the watchdog ends a helper
    // that goes quiet) instead of hanging.
    host.relay = Arc::new(|_: &str| false);
    let sent = relayed.recv().unwrap();
    let sent: serde_json::Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(sent["@type"], "processWebProxyCarrierMessage");
    assert_eq!(sent["@extra"], CARRIER_RELAY_EXTRA.as_extra());
    assert_eq!(sent["carrier_id"], 7);
    assert_eq!(sent["message"]["text"], "hi");
    host.shutdown();
}

/// `stays-up.sh` leaves only on the close command (or the end of its
/// input): it is running when closed here, and its exit follows the close.
#[test]
fn a_closed_carrier_reports_no_failure() {
    let (mut host, _) = host(Some(stand_in("stays-up.sh")));
    assert_eq!(host.open(&request(5)), Ok(()));
    assert!(helper_running(&host, 5));
    host.close(5, false);
    // It exits once the close reaches it; that exit is no failure.
    assert_eq!(reports_until_threads_end(&mut host), Vec::new());
}

#[test]
fn data_for_a_carrier_the_host_does_not_run_fails_it() {
    let (mut host, _) = host(Some(stand_in("stays-up.sh")));
    host.deliver(9, "AAAA".into());
    assert_eq!(host.poll(), vec![(9, CarrierError::Terminated)]);
}

#[test]
fn shutdown_ends_every_helper_at_once() {
    let (mut host, _) = host(Some(stand_in("stays-up.sh")));
    assert_eq!(host.open(&request(5)), Ok(()));
    assert!(helper_running(&host, 5));
    let shared = host
        .carriers
        .get(&5)
        .map(|running| running.shared.clone())
        .unwrap();
    host.shutdown();
    assert!(shared.take_child().is_none());
    assert_eq!(reports_until_threads_end(&mut host), Vec::new());
}

#[test]
fn a_shutdown_with_nothing_running_or_exiting_has_nothing_to_wait_for() {
    let (mut host, _) = host(Some(stand_in("stays-up.sh")));
    assert!(!host.exiting());
    assert_eq!(shutdown_outcome(&mut host), Waited::NotAtAll);
}

/// A ceiling already reached (zero) gives up on a helper still exiting
/// without waiting for it. A ceiling that runs out during the wait is not
/// covered: that needs a clock.
#[test]
fn a_shutdown_past_its_ceiling_gives_up_on_a_stuck_helper() {
    let (mut host, _) = host(Some(stand_in("stuck.sh")));
    host.shutdown_wait = Duration::ZERO;
    assert_eq!(host.open(&request(5)), Ok(()));
    assert!(host.exiting());
    assert_eq!(shutdown_outcome(&mut host), Waited::GaveUp);
}

/// A closed helper that leaves on the close ends the shutdown's wait.
/// Neither the shutdown's ceiling nor the close's grace (after which the
/// helper would be killed) can end it here, so only that exit can.
#[test]
fn a_closed_helper_that_exits_ends_the_wait() {
    let (mut host, _) = host(Some(stand_in("stays-up.sh")));
    host.close_grace = LONGER_THAN_ANY_TEST;
    host.shutdown_wait = LONGER_THAN_ANY_TEST;
    assert_eq!(host.open(&request(5)), Ok(()));
    host.close(5, false);
    assert_eq!(shutdown_outcome(&mut host), Waited::UntilAllGone);
}

#[test]
fn an_exit_code_of_127_is_a_loader_failure() {
    use std::os::unix::process::ExitStatusExt;
    assert!(loader_failure(ExitStatus::from_raw(127 << 8)));
    assert!(!loader_failure(ExitStatus::from_raw(1 << 8)));
    assert!(!loader_failure(ExitStatus::from_raw(9)));
}

fn cookie_in(dir: &Path) -> PathBuf {
    let cookie = dir.join("Cookies");
    std::fs::write(&cookie, b"cookie").unwrap();
    cookie
}

/// The account's next host starts while the last one is still tearing
/// down (a helper that ignores the close keeps it waiting, with no
/// ceiling, until the test kills it): neither the new host's sweep nor the
/// old teardown touches the other's folder.
#[test]
fn a_late_teardown_leaves_the_next_hosts_folder_alone() {
    let root = crate::scratch::process_dir("carrier-host")
        .join("late-teardown")
        .join("webview-carrier");
    let (mut old, _) = host_with(Some(stand_in("stuck.sh")), Some(root.clone()));
    old.shutdown_wait = LONGER_THAN_ANY_TEST;
    assert_eq!(old.open(&request(5)), Ok(()));
    let stuck = i32::try_from(helper_pid(&old, 5)).unwrap();
    let old_cookie = cookie_in(old.data_folder().unwrap());
    let teardown =
        crate::thread_guard::spawn_guarded("test-teardown", move || old.shutdown()).unwrap();
    let (new, _) = host_with(Some(stand_in("stays-up.sh")), Some(root.clone()));
    let new_cookie = cookie_in(new.data_folder().unwrap());
    if let Some(sweep) = data_folder::sweep_in_background(root.clone()) {
        let _ = sweep.join();
    }
    // The teardown cannot end while its helper runs.
    assert!(old_cookie.exists());
    // SAFETY: a plain syscall on the id of the old host's own child, not
    // reaped yet so the id is still its; it dereferences nothing. Not the
    // `kill` command: a child started here is what made these tests flaky.
    assert_eq!(unsafe { libc::kill(stuck, libc::SIGKILL) }, 0);
    let _ = teardown.join();
    assert!(!old_cookie.exists());
    assert!(new_cookie.exists());
    drop(new);
}

/// No helper runs without a folder of its own once an account folder is
/// configured: an open fails until a generation can be claimed.
#[test]
fn without_a_folder_of_its_own_no_carrier_opens() {
    let base = crate::scratch::process_dir("carrier-host").join("no-folder");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let root = base.join("webview-carrier");
    std::fs::write(&root, b"not a folder").unwrap();
    let failed_at = Instant::now();
    let (mut host, _) = host_with(Some(stand_in("stays-up.sh")), Some(root.clone()));
    assert_eq!(host.data_folder(), None);
    assert_eq!(host.open(&request(5)), Err(CarrierError::Terminated));
    std::fs::remove_file(&root).unwrap();
    // Not tried again on every open: the next claim waits out the backoff,
    // and an open before then is refused although a claim would now work.
    assert!(host.claim_after >= Some(failed_at + CLAIM_BACKOFF));
    host.claim_after = Some(Instant::now() + LONGER_THAN_ANY_TEST);
    assert_eq!(host.open(&request(6)), Err(CarrierError::Terminated));
    host.claim_after = Some(Instant::now());
    assert_eq!(host.open(&request(6)), Ok(()));
    let folder = host.data_folder().map(Path::to_path_buf).unwrap();
    assert!(folder.starts_with(&root) && folder.is_dir());
    host.shutdown();
    assert!(!folder.exists());
}
