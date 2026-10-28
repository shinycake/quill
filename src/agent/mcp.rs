//! `quill mcp`: an MCP server on stdio that bridges to the running Quill.
//!
//! The catalog, the instructions and the guide come from this binary, so
//! `initialize`, `tools/list` and `resources/list` answer even before Quill
//! runs. `tools/call` and `resources/read` go to Quill over the local socket
//! verbatim, and Quill's answers come back verbatim. The first forwarded
//! request starts Quill when it is not running and waits for its socket.

use super::link::SharedStream;
use super::server::code;
use crate::single_instance::Endpoint;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const LATEST_PROTOCOL: &str = "2025-06-18";
const PROTOCOL_VERSIONS: [&str; 3] = [LATEST_PROTOCOL, "2025-03-26", "2024-11-05"];

type Out = Arc<Mutex<io::Stdout>>;

fn emit(out: &Out, message: &Value) {
    let mut out = crate::locks::lock_or_recover(out);
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn emit_raw(out: &Out, line: &str) {
    let mut out = crate::locks::lock_or_recover(out);
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// A failure the agent should read: a tool error for `tools/call`, a
/// JSON-RPC error otherwise.
fn failure(id: &Value, is_tool: bool, message: &str) -> Value {
    if is_tool {
        result(
            id,
            json!({"content": [{"type": "text", "text": message}], "isError": true}),
        )
    } else {
        error(id, code::ACCESS_OFF, message)
    }
}

/// Requests sent to Quill and not yet answered: id → whether it is a tool call.
type Pending = Arc<Mutex<HashMap<String, (Value, bool)>>>;

struct Link {
    send: SharedStream,
    alive: Arc<AtomicBool>,
    pending: Pending,
}

/// The MCP client's name from `initialize`, which Quill shows in prompts.
static CLIENT_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn initialize(id: &Value, params: &Value) -> Value {
    let info = params.get("clientInfo");
    if let Some(name) = info
        .and_then(|i| i.get("title"))
        .or_else(|| info.and_then(|i| i.get("name")))
        .and_then(Value::as_str)
    {
        let _ = CLIENT_NAME.set(friendly_client_name(name));
    }
    let asked = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = PROTOCOL_VERSIONS
        .iter()
        .find(|v| **v == asked)
        .copied()
        .unwrap_or(LATEST_PROTOCOL);
    result(
        id,
        json!({
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": false}, "resources": {"subscribe": true}},
            "serverInfo": {"name": "quill", "title": "Quill", "version": crate::version::APP},
            "instructions": super::guide::INSTRUCTIONS,
        }),
    )
}

/// "claude-code" → "Claude Code"; names that already look written stay.
fn friendly_client_name(name: &str) -> String {
    if name.contains(' ') || name.chars().any(|c| c.is_ascii_uppercase()) {
        return name.to_string();
    }
    name.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run the bridge until stdin closes. Returns the process exit code.
pub fn run() -> i32 {
    let out: Out = Arc::new(Mutex::new(io::stdout()));
    let mut link: Option<Link> = None;
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            emit(&out, &error(&Value::Null, code::PARSE, "Not JSON."));
            continue;
        };
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match message.get("method").and_then(Value::as_str) {
            Some("initialize") => emit(&out, &initialize(&id, &params)),
            Some("ping") => emit(&out, &result(&id, json!({}))),
            Some("tools/list") => emit(
                &out,
                &result(&id, json!({"tools": super::tools::catalog()})),
            ),
            Some("resources/list") => emit(
                &out,
                &result(&id, json!({"resources": super::guide::resources()})),
            ),
            Some("resources/templates/list") => {
                emit(&out, &result(&id, json!({"resourceTemplates": []})))
            }
            Some("resources/read")
                if params.get("uri").and_then(Value::as_str) == Some(super::guide::GUIDE_URI) =>
            {
                if let Some(read) = super::guide::read_resource(super::guide::GUIDE_URI) {
                    emit(&out, &result(&id, read));
                }
            }
            Some(
                method @ ("tools/call"
                | "resources/read"
                | "resources/subscribe"
                | "resources/unsubscribe"),
            ) => forward(&out, &mut link, &id, method == "tools/call", line),
            Some(other) => emit(
                &out,
                &error(&id, code::NO_METHOD, &format!("Unknown method {other}.")),
            ),
            None => emit(&out, &error(&id, code::INVALID, "Missing method.")),
        }
    }
    0
}

fn forward(out: &Out, link: &mut Option<Link>, id: &Value, is_tool: bool, line: &str) {
    if link
        .as_ref()
        .is_none_or(|l| !l.alive.load(Ordering::SeqCst))
    {
        *link = None;
        match open_link(out) {
            Ok(opened) => *link = Some(opened),
            Err(message) => {
                emit(out, &failure(id, is_tool, &message));
                return;
            }
        }
    }
    let Some(live) = link.as_mut() else {
        return;
    };
    crate::locks::lock_or_recover(&live.pending).insert(id.to_string(), (id.clone(), is_tool));
    let sent = live
        .send
        .write_all(line.as_bytes())
        .and_then(|()| live.send.write_all(b"\n"))
        .and_then(|()| live.send.flush());
    if sent.is_err() {
        crate::locks::lock_or_recover(&live.pending).remove(&id.to_string());
        live.alive.store(false, Ordering::SeqCst);
        emit(
            out,
            &failure(id, is_tool, "Lost the connection to Quill. Retry."),
        );
    }
}

fn open_link(out: &Out) -> Result<Link, String> {
    let root = crate::settings::safe_app_root()
        .ok_or_else(|| "Quill's data folder is unavailable on this computer.".to_string())?;
    let endpoint = Endpoint::for_root(&root);
    let stream = super::link::open(
        &root,
        &endpoint,
        true,
        CLIENT_NAME.get().map(String::as_str),
    )
    .map_err(|refused| refused.message())?;
    let send = SharedStream::new(stream);
    let recv = send.clone();
    let alive = Arc::new(AtomicBool::new(true));
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let reader = {
        let out = out.clone();
        let alive = alive.clone();
        let pending = pending.clone();
        move || relay(recv, &out, &alive, &pending)
    };
    crate::thread_guard::spawn_guarded("quill-mcp-relay", reader)
        .map_err(|_| "Could not start the bridge thread. Retry.".to_string())?;
    Ok(Link {
        send,
        alive,
        pending,
    })
}

/// Copy Quill's answers to stdout; when Quill goes away, fail what is still
/// waiting so the agent is never left hanging.
fn relay(recv: SharedStream, out: &Out, alive: &AtomicBool, pending: &Pending) {
    let reader = BufReader::new(recv);
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        if let Ok(answer) = serde_json::from_str::<Value>(&line)
            && let Some(id) = answer.get("id")
        {
            crate::locks::lock_or_recover(pending).remove(&id.to_string());
        }
        emit_raw(out, &line);
    }
    alive.store(false, Ordering::SeqCst);
    let waiting: Vec<(Value, bool)> = crate::locks::lock_or_recover(pending)
        .drain()
        .map(|(_, v)| v)
        .collect();
    for (id, is_tool) in waiting {
        emit(
            out,
            &failure(
                &id,
                is_tool,
                "Quill closed the connection: it quit, or Agent access was turned off. Retry; Quill is started again if needed.",
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{failure, initialize};
    use serde_json::json;

    #[test]
    fn initialize_echoes_a_supported_version_and_orients() {
        let answer = initialize(&json!(1), &json!({"protocolVersion": "2025-03-26"}));
        assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");
        assert!(
            answer["result"]["instructions"]
                .as_str()
                .unwrap_or_default()
                .contains("quill_status")
        );
        let answer = initialize(&json!(1), &json!({"protocolVersion": "1999-01-01"}));
        assert_eq!(answer["result"]["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn client_names_read_naturally() {
        assert_eq!(super::friendly_client_name("claude-code"), "Claude Code");
        assert_eq!(super::friendly_client_name("Cursor"), "Cursor");
    }

    #[test]
    fn tool_failures_reach_the_model_as_tool_errors() {
        assert_eq!(failure(&json!(3), true, "x")["result"]["isError"], true);
        assert!(failure(&json!(3), false, "x")["error"].is_object());
    }
}
