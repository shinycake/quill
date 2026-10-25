//! The agent side of Quill's local socket.
//!
//! After the `quill-agent 1` header line, the client speaks line-delimited
//! JSON-RPC 2.0. Its first request must be `auth` with this launch's token
//! (`{"method": "auth", "params": {"token": "…"}}`); anything else, or a
//! wrong token, ends the connection. Then it may call `tools/call`,
//! `resources/read` and `ping`, answered in MCP's result shapes, with
//! several requests in flight at once.

use super::access;
use interprocess::local_socket::{Stream, prelude::*};
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The header line that selects this protocol.
pub const HEADER: &str = "quill-agent 1";
/// Upper bound on one request line.
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
/// Agent connections served at once.
const MAX_CONNECTIONS: usize = 4;
/// Requests one connection may have in flight.
const MAX_IN_FLIGHT: usize = 8;
/// How long the client has to authenticate (Unix sockets only).
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a connection waiting for a request checks the switch (Unix
/// sockets only; a Windows pipe notices at its next request).
const IDLE_CHECK: Duration = Duration::from_secs(1);

pub const OFF_MESSAGE: &str = "Agent access is off in Quill. Ask the user to turn it on in Quill: Settings > Advanced > Agent access.";

/// JSON-RPC error codes past the reserved range.
pub mod code {
    pub const ACCESS_OFF: i64 = -32001;
    pub const UNAUTHORIZED: i64 = -32002;
    pub const BUSY: i64 = -32003;
    pub const PARSE: i64 = -32700;
    pub const INVALID: i64 = -32600;
    pub const NO_METHOD: i64 = -32601;
}

static CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

struct ConnectionSlot;

impl ConnectionSlot {
    fn take() -> Option<Self> {
        if CONNECTIONS.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self)
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Whether agents are connected right now.
pub fn connected_agents() -> usize {
    CONNECTIONS.load(Ordering::SeqCst)
}

pub fn error_line(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

fn result_line(id: &Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn write_line(out: &mut impl Write, line: &str) -> io::Result<()> {
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

/// The peer runs as this user (Unix). The socket file is 0600 already; this
/// also covers a socket in a shared temp folder.
#[cfg(unix)]
fn same_user(stream: &Stream) -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    let me = unsafe { libc::geteuid() };
    stream
        .peer_creds()
        .ok()
        .and_then(|creds| creds.euid())
        .is_some_and(|uid| uid == me)
}

#[cfg(windows)]
fn same_user(_stream: &Stream) -> bool {
    // The pipe's DACL admits only this user (`pipe_security`).
    true
}

/// Read one line, unbuffered, so nothing past it is consumed.
pub(super) fn read_line_raw(stream: &Stream) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut reader = stream;
    while line.len() < MAX_LINE_BYTES {
        match reader.read(&mut byte) {
            Ok(1) => {
                let [b] = byte;
                if b == b'\n' {
                    return String::from_utf8(line).ok();
                }
                line.push(b);
            }
            _ => return None,
        }
    }
    None
}

fn authenticate(stream: &Stream) -> Result<Value, (Value, i64, &'static str)> {
    let line = read_line_raw(stream).ok_or((Value::Null, code::UNAUTHORIZED, "Expected auth."))?;
    let request: Value =
        serde_json::from_str(&line).map_err(|_| (Value::Null, code::PARSE, "Expected auth."))?;
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let token = request
        .get("params")
        .and_then(|p| p.get("token"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if request.get("method").and_then(Value::as_str) != Some("auth")
        || !access::token_matches(token)
    {
        return Err((
            id,
            code::UNAUTHORIZED,
            "Wrong or stale token. Quill makes a new one on every launch; reconnect to read it again.",
        ));
    }
    Ok(id)
}

/// Serve one agent connection whose header line was already read.
pub fn serve(stream: Stream) {
    let mut out = &stream;
    if !access::enabled() {
        let _ = write_line(
            &mut out,
            &error_line(&Value::Null, code::ACCESS_OFF, OFF_MESSAGE),
        );
        return;
    }
    if !same_user(&stream) {
        return;
    }
    let Some(_slot) = ConnectionSlot::take() else {
        let _ = write_line(
            &mut out,
            &error_line(
                &Value::Null,
                code::BUSY,
                "Too many agents are connected to Quill. Close one and retry.",
            ),
        );
        return;
    };
    let generation = access::generation();
    let _ = stream.set_recv_timeout(Some(AUTH_TIMEOUT));
    let auth_id = match authenticate(&stream) {
        Ok(id) => id,
        Err((id, code, message)) => {
            let _ = write_line(&mut out, &error_line(&id, code, message));
            return;
        }
    };
    if write_line(
        &mut out,
        &result_line(&auth_id, json!({"quill_version": crate::version::APP})),
    )
    .is_err()
    {
        return;
    }
    let _ = stream.set_recv_timeout(Some(IDLE_CHECK));
    let (recv, send) = stream.split();
    let send = Arc::new(Mutex::new(send));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let mut reader = BufReader::new(recv);
    let mut line = Vec::new();
    loop {
        let still_on = access::enabled() && access::generation() == generation;
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => return,
            Ok(_) if line.ends_with(b"\n") => {}
            Ok(_) => return,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if !still_on {
                    return;
                }
                if line.len() > MAX_LINE_BYTES {
                    return;
                }
                continue;
            }
            Err(_) => return,
        }
        let text = String::from_utf8_lossy(&line).trim().to_string();
        line.clear();
        if text.is_empty() {
            continue;
        }
        if !(access::enabled() && access::generation() == generation) {
            let mut out = crate::locks::lock_or_recover(&send);
            let _ = write_line(
                &mut *out,
                &error_line(&Value::Null, code::ACCESS_OFF, OFF_MESSAGE),
            );
            return;
        }
        dispatch(text, &send, &in_flight);
    }
}

/// Answer one request on a thread of its own, so a slow call does not hold
/// up the rest.
fn dispatch<W: Write + Send + 'static>(
    text: String,
    send: &Arc<Mutex<W>>,
    in_flight: &Arc<AtomicUsize>,
) {
    let request: Value = match serde_json::from_str(&text) {
        Ok(request) => request,
        Err(_) => {
            let mut out = crate::locks::lock_or_recover(send);
            let _ = write_line(
                &mut *out,
                &error_line(&Value::Null, code::PARSE, "Not JSON."),
            );
            return;
        }
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    // A notification gets no answer.
    if request.get("id").is_none() {
        return;
    }
    if in_flight.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
        in_flight.fetch_sub(1, Ordering::SeqCst);
        let mut out = crate::locks::lock_or_recover(send);
        let _ = write_line(
            &mut *out,
            &error_line(
                &id,
                code::BUSY,
                "Too many requests in flight. Wait for some answers, then retry.",
            ),
        );
        return;
    }
    let send = send.clone();
    let counter = in_flight.clone();
    let spawned = crate::thread_guard::spawn_guarded("quill-agent-request", move || {
        let line = answer(&request, &id);
        counter.fetch_sub(1, Ordering::SeqCst);
        let mut out = crate::locks::lock_or_recover(&send);
        let _ = write_line(&mut *out, &line);
    });
    if spawned.is_err() {
        in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

fn answer(request: &Value, id: &Value) -> String {
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    match request.get("method").and_then(Value::as_str) {
        Some("ping") => result_line(id, json!({})),
        Some("tools/call") => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return error_line(id, code::INVALID, "tools/call needs params.name.");
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            result_line(id, super::tools::call(name, &arguments))
        }
        Some("resources/read") => {
            let uri = params
                .get("uri")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match super::guide::read_resource(uri) {
                Some(result) => result_line(id, result),
                None => error_line(id, code::INVALID, "Unknown resource. Call resources/list."),
            }
        }
        Some(other) => error_line(
            id,
            code::NO_METHOD,
            &format!("Quill has no method {other}."),
        ),
        None => error_line(id, code::INVALID, "Missing method."),
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
