//! The agent protocol over a real local socket. One test, because the
//! Agent access switch is process-wide.

use super::HEADER;
use crate::agent::access;
use crate::single_instance::{Acquired, Endpoint, acquire, connect};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

fn session(endpoint: &Endpoint, lines: &[String]) -> Vec<Value> {
    let stream = connect(endpoint).unwrap_or_else(|e| panic!("connect: {e}"));
    let mut writer = &stream;
    let mut hello = format!("{HEADER}\n");
    for line in lines {
        hello.push_str(line);
        hello.push('\n');
    }
    writer
        .write_all(hello.as_bytes())
        .unwrap_or_else(|e| panic!("write: {e}"));
    let mut reader = BufReader::new(&stream);
    let mut answers = Vec::new();
    // One answer per request (the auth answer included), or until Quill hangs up.
    for _ in 0..lines.len().max(1) {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        answers.push(serde_json::from_str(&line).unwrap_or(Value::Null));
    }
    answers
}

fn auth(token: &str) -> String {
    json!({"jsonrpc": "2.0", "id": "a", "method": "auth", "params": {"token": token}}).to_string()
}

#[test]
fn agents_need_the_switch_and_this_launchs_token() {
    let root = crate::scratch::process_dir("quill-agent-socket");
    let endpoint = Endpoint::for_root(&root);
    assert_eq!(
        acquire(&endpoint, &[], |_| ()).ok(),
        Some(Acquired::Primary)
    );
    let app_root = crate::settings::safe_app_root().unwrap_or_default();

    assert!(!access::enabled(), "off by default");
    assert_eq!(access::read_published_token(&app_root), None);
    let answers = session(&endpoint, &[auth("x")]);
    assert_eq!(answers[0]["error"]["code"], super::code::ACCESS_OFF);

    access::set_enabled(true);
    let token = access::read_published_token(&app_root).unwrap_or_default();
    assert_eq!(token.len(), 64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(access::token_path(&app_root))
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0);
        assert_eq!(mode, 0o600, "token file is owner-only");
    }

    let answers = session(&endpoint, &[auth("0".repeat(64).as_str())]);
    assert_eq!(answers[0]["error"]["code"], super::code::UNAUTHORIZED);
    let answers = session(
        &endpoint,
        &[json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}).to_string()],
    );
    assert_eq!(
        answers[0]["error"]["code"],
        super::code::UNAUTHORIZED,
        "auth comes first"
    );

    let answers = session(
        &endpoint,
        &[
            auth(&token),
            json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}).to_string(),
            json!({"jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": {"uri": "quill://guide"}}).to_string(),
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "nope"}}).to_string(),
            json!({"jsonrpc": "2.0", "id": 5, "method": "resources/subscribe", "params": {"uri": "quill://unread"}}).to_string(),
            json!({"jsonrpc": "2.0", "id": 6, "method": "resources/subscribe", "params": {"uri": "quill://guide"}}).to_string(),
        ],
    );
    assert_eq!(answers[0]["id"], "a");
    assert!(answers[0]["result"]["quill_version"].is_string());
    let by_id = |id: i64| {
        answers
            .iter()
            .find(|a| a["id"] == id)
            .cloned()
            .unwrap_or(Value::Null)
    };
    assert_eq!(by_id(2)["result"], json!({}));
    assert!(
        by_id(3)["result"]["contents"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("quill_status")
    );
    assert_eq!(by_id(4)["result"]["isError"], true);
    assert_eq!(
        by_id(5)["result"],
        json!({}),
        "quill://unread can be watched"
    );
    assert!(
        by_id(6)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("wait_for_message")
    );

    // A request line past the limit ends the connection, whether or not it
    // ever gets a newline.
    let stream = connect(&endpoint).unwrap_or_else(|e| panic!("connect: {e}"));
    let mut writer = &stream;
    let hello = format!("{HEADER}\n{}\n", auth(&token));
    writer
        .write_all(hello.as_bytes())
        .unwrap_or_else(|e| panic!("write: {e}"));
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    assert!(line.contains("quill_version"), "{line}");
    let _ = writer.write_all(&vec![b'x'; super::MAX_LINE_BYTES + 2]);
    line.clear();
    assert_eq!(
        reader.read_line(&mut line).unwrap_or(0),
        0,
        "closed: {line}"
    );

    // The launch protocol still works beside agents.
    assert_eq!(
        acquire(&endpoint, &[], |_| ()).ok(),
        Some(Acquired::Forwarded)
    );

    access::set_enabled(false);
    assert_eq!(
        access::read_published_token(&app_root),
        None,
        "off deletes the token"
    );
    std::thread::sleep(Duration::from_millis(10));
    let answers = session(&endpoint, &[auth(&token)]);
    assert_eq!(answers[0]["error"]["code"], super::code::ACCESS_OFF);
}
