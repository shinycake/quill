//! `quill agent` against Quill's real agent socket: the same server the MCP
//! bridge reaches, with the switch and token as in `server_tests.rs`.

use super::Exit;
use super::client::request_at;
use crate::agent::access;
use crate::single_instance::{Acquired, Endpoint, acquire};
use serde_json::json;

#[test]
fn the_command_line_reaches_quill_and_names_each_refusal() {
    // Taken first: listening publishes the token (see server_tests.rs).
    let _guard = crate::locks::lock_or_recover(&access::TEST_LOCK);
    let root = crate::scratch::process_dir("quill-agent-cli-socket");
    let endpoint = Endpoint::for_root(&root);
    assert_eq!(
        acquire(&endpoint, &[], |_| ()).ok(),
        Some(Acquired::Primary)
    );
    let nobody = Endpoint::for_root(&crate::scratch::process_dir("quill-agent-cli-nobody"));
    let app_root = crate::settings::safe_app_root().unwrap_or_default();
    let call = |endpoint: &Endpoint, name: &str| {
        request_at(
            &app_root,
            endpoint,
            false,
            "Test CLI",
            "tools/call",
            json!({"name": name, "arguments": {}}),
        )
    };

    let _reset = Reset(app_root.clone());
    access::set_enabled(false);
    assert_eq!(
        call(&endpoint, "quill_status").err().map(|f| f.exit),
        Some(Exit::AccessOff)
    );
    assert_eq!(
        call(&nobody, "quill_status").err().map(|f| f.exit),
        Some(Exit::AccessOff)
    );

    access::set_enabled(true);
    // Tests don't save settings; `quill agent` runs in its own process and
    // reads the switch from disk.
    std::fs::write(app_root.join("agent_access.json"), r#"{"enabled": true}"#).unwrap_or_default();
    let not_running = call(&nobody, "quill_status").err();
    assert_eq!(not_running.as_ref().map(|f| f.exit), Some(Exit::NotRunning));
    assert!(
        not_running
            .map(|f| f.message)
            .unwrap_or_default()
            .contains("--start")
    );

    // A tool call goes through the same dispatcher as MCP's tools/call.
    let unknown = call(&endpoint, "send_everything").unwrap_or_default();
    assert_eq!(unknown["isError"], true);
    let shaped = super::shape(&unknown, false, &[]).err().map(|f| f.exit);
    assert_eq!(shaped, Some(Exit::ToolError));
    let status = call(&endpoint, "quill_status").unwrap_or_default();
    assert!(status["content"][0]["text"].is_string(), "{status}");
    let guide = request_at(
        &app_root,
        &endpoint,
        false,
        "Test CLI",
        "resources/read",
        json!({"uri": "quill://guide"}),
    )
    .unwrap_or_default();
    assert!(
        guide["contents"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("quill agent")
    );

    // A token that is not this launch's is refused, after one retry.
    let token_path = access::token_path(&app_root);
    let real = std::fs::read_to_string(&token_path).unwrap_or_default();
    std::fs::write(&token_path, "0".repeat(64)).unwrap_or_default();
    assert_eq!(
        call(&endpoint, "quill_status").err().map(|f| f.exit),
        Some(Exit::Token)
    );
    std::fs::write(&token_path, real).unwrap_or_default();
    std::fs::remove_file(&token_path).unwrap_or_default();
    assert_eq!(
        call(&endpoint, "quill_status").err().map(|f| f.exit),
        Some(Exit::Token)
    );
}

/// Turns the switch off and removes the settings file, even after a failed
/// assertion, so the other socket test starts from off.
struct Reset(std::path::PathBuf);

impl Drop for Reset {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("agent_access.json"));
        access::set_enabled(false);
    }
}
