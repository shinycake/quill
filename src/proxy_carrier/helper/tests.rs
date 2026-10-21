//! The host against stand-in helpers (`true`, `cat`, a script that exits
//! like a missing library).
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

#[test]
fn a_closed_carrier_reports_no_failure() {
    let (mut host, _) = host(Some("/bin/cat".into()));
    assert_eq!(host.open(&request(5)), Ok(()));
    host.close(5, false);
    std::thread::sleep(Duration::from_millis(300));
    assert!(host.poll().is_empty());
}

#[test]
fn data_for_a_carrier_the_host_does_not_run_fails_it() {
    let (mut host, _) = host(Some("/bin/cat".into()));
    host.deliver(9, "AAAA".into());
    assert_eq!(host.poll(), vec![(9, CarrierError::Terminated)]);
}

#[test]
fn shutdown_ends_every_helper_at_once() {
    let (mut host, _) = host(Some("/bin/cat".into()));
    assert_eq!(host.open(&request(5)), Ok(()));
    let shared = host
        .carriers
        .get(&5)
        .map(|running| running.shared.clone())
        .unwrap();
    host.shutdown();
    assert!(shared.take_child().is_none());
    assert!(host.poll().is_empty());
}

#[test]
fn an_exit_code_of_127_is_a_loader_failure() {
    use std::os::unix::process::ExitStatusExt;
    assert!(loader_failure(ExitStatus::from_raw(127 << 8)));
    assert!(!loader_failure(ExitStatus::from_raw(1 << 8)));
    assert!(!loader_failure(ExitStatus::from_raw(9)));
}
