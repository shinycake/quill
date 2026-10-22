//! The host against stand-in helpers (`true`, scripts that wait for the
//! close command, exit like a missing library, or ignore everything).
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
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let relay: RelaySink = Arc::new(move |request: &str| {
        tx.lock()
            .map(|tx| tx.send(request.to_string()).is_ok())
            .unwrap_or(false)
    });
    let host = HelperCarrierHost::new(HelperConfig {
        program,
        data_dir: None,
        probe_cache: None,
        relay,
        capability: Capability::default(),
    });
    (host, rx)
}

fn wait_failure(host: &mut HelperCarrierHost) -> Option<(i32, CarrierError)> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(failure) = host.poll().into_iter().next() {
            return Some(failure);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// An executable script in a fresh temp folder.
fn script(name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = crate::scratch::process_dir("carrier-host").join(name);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("helper.sh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A stand-in that stays up until the `close` command (or the end of its
/// input), as the real helper does. Not `/bin/cat`: the host starts
/// helpers with `--carrier`, which cat rejects, exiting at once. Not one
/// that waits for the end of its input either: on macOS a child another
/// test starts at the same moment can inherit this helper's stdin and keep
/// it open. One per test (`name`), so no test rewrites a script another is
/// starting.
fn stays_up(name: &str) -> PathBuf {
    script(
        name,
        r#"while read -r line; do case "$line" in *'"kind":"close"'*) exit 0 ;; esac; done"#,
    )
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
    assert_eq!(wait_failure(&mut host), Some((4, CarrierError::Terminated)));
    assert!(host.available());
}

#[test]
fn a_missing_web_view_library_turns_hosting_off() {
    // 127 is what the dynamic loader exits with when a library is missing.
    let (mut host, _) = host(Some(script("missing-library", "exit 127")));
    assert_eq!(host.open(&request(4)), Ok(()));
    assert_eq!(
        wait_failure(&mut host),
        Some((4, CarrierError::Unsupported))
    );
    assert!(!host.available());
    assert_eq!(host.open(&request(5)), Err(CarrierError::Unsupported));
}

#[test]
fn a_helper_saying_unsupported_turns_hosting_off() {
    let (mut host, _) = host(Some(script(
        "says-unsupported",
        r#"read line; echo '{"kind":"failed","failure":{"kind":"unsupported"}}'; read line"#,
    )));
    assert_eq!(host.open(&request(4)), Ok(()));
    assert_eq!(
        wait_failure(&mut host),
        Some((4, CarrierError::Unsupported))
    );
    assert!(!host.available());
}

#[test]
fn page_messages_go_straight_to_tdlib() {
    let (mut host, relayed) = host(Some(script(
        "posts",
        r#"read line; echo '{"kind":"ready"}'; echo '{"kind":"text","text":"hi"}'; sleep 5"#,
    )));
    assert_eq!(host.open(&request(7)), Ok(()));
    let sent = relayed.recv_timeout(Duration::from_secs(10)).unwrap();
    let sent: serde_json::Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(sent["@type"], "processWebProxyCarrierMessage");
    assert_eq!(sent["@extra"], CARRIER_RELAY_EXTRA.as_extra());
    assert_eq!(sent["carrier_id"], 7);
    assert_eq!(sent["message"]["text"], "hi");
    host.shutdown();
}

/// The close tests below need a helper that is still up when they close
/// it; one that exits by itself first is a real failure, and the test
/// then only passes when the close wins the race.
#[test]
fn the_stand_in_helper_stays_up_until_closed() {
    let (mut host, _) = host(Some(stays_up("stays-up")));
    assert_eq!(host.open(&request(5)), Ok(()));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(host.poll(), Vec::new());
    assert!(helper_running(&host, 5));
    host.shutdown();
}

#[test]
fn a_closed_carrier_reports_no_failure() {
    let (mut host, _) = host(Some(stays_up("closed")));
    assert_eq!(host.open(&request(5)), Ok(()));
    std::thread::sleep(Duration::from_millis(100));
    assert!(helper_running(&host, 5));
    host.close(5, false);
    // It exits once the close reaches it; that exit is no failure.
    let deadline = Instant::now() + Duration::from_secs(2);
    while host.shutdown_waits() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!host.shutdown_waits());
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(host.poll(), Vec::new());
}

#[test]
fn data_for_a_carrier_the_host_does_not_run_fails_it() {
    let (mut host, _) = host(Some(stays_up("not-running")));
    host.deliver(9, "AAAA".into());
    assert_eq!(host.poll(), vec![(9, CarrierError::Terminated)]);
}

#[test]
fn shutdown_ends_every_helper_at_once() {
    let (mut host, _) = host(Some(stays_up("shutdown")));
    assert_eq!(host.open(&request(5)), Ok(()));
    std::thread::sleep(Duration::from_millis(100));
    assert!(helper_running(&host, 5));
    let shared = host
        .carriers
        .get(&5)
        .map(|running| running.shared.clone())
        .unwrap();
    host.shutdown();
    assert!(shared.take_child().is_none());
    assert_eq!(host.poll(), Vec::new());
}

#[test]
fn a_shutdown_with_nothing_running_or_exiting_does_not_wait() {
    let (mut host, _) = host(Some(stays_up("idle-shutdown")));
    assert!(!host.shutdown_waits());
    let started = Instant::now();
    host.shutdown();
    assert!(started.elapsed() < Duration::from_millis(200));
}

/// A helper that ignores the close command: shutdown gives up at its
/// ceiling.
#[test]
fn a_shutdown_waits_for_helpers_at_most_its_ceiling() {
    let (mut host, _) = host(Some(script("stuck", "exec sleep 30")));
    assert_eq!(host.open(&request(5)), Ok(()));
    assert!(host.shutdown_waits());
    let started = Instant::now();
    host.shutdown();
    let took = started.elapsed();
    assert!(
        took >= SHUTDOWN_WAIT.saturating_sub(Duration::from_millis(100)),
        "{took:?}"
    );
    assert!(
        took < SHUTDOWN_WAIT + Duration::from_millis(500),
        "{took:?}"
    );
    assert!(!host.shutdown_waits());
}

#[test]
fn a_closed_helper_that_exits_ends_the_wait() {
    let (mut host, _) = host(Some(stays_up("exits")));
    assert_eq!(host.open(&request(5)), Ok(()));
    if let Some(running) = host.carriers.get(&5) {
        running.shared.kill();
    }
    host.close(5, false);
    let started = Instant::now();
    host.shutdown();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn an_exit_code_of_127_is_a_loader_failure() {
    use std::os::unix::process::ExitStatusExt;
    assert!(loader_failure(ExitStatus::from_raw(127 << 8)));
    assert!(!loader_failure(ExitStatus::from_raw(1 << 8)));
    assert!(!loader_failure(ExitStatus::from_raw(9)));
}
