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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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

pub const OFF_MESSAGE: &str =
    "Agent access is off in Quill. Ask the user to turn it on in Quill: Settings > Agent access.";
/// The answer to an agent the user disconnected with Revoke.
pub const REVOKED_MESSAGE: &str = "The user disconnected this agent in Quill. Don't reconnect; ask the user to press Allow again for it in Quill: Settings > Agent access.";

/// JSON-RPC error codes past the reserved range.
pub mod code {
    pub const ACCESS_OFF: i64 = -32001;
    pub const UNAUTHORIZED: i64 = -32002;
    pub const BUSY: i64 = -32003;
    pub const INTERNAL: i64 = -32603;
    pub const PARSE: i64 = -32700;
    pub const INVALID: i64 = -32600;
    pub const NO_METHOD: i64 = -32601;
}

/// Authenticated agent connections.
static CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
/// Connections still waiting to authenticate. A Windows pipe has no read
/// timeout, so without a cap a client could park any number of threads.
static UNAUTHENTICATED: AtomicUsize = AtomicUsize::new(0);
const MAX_UNAUTHENTICATED: usize = 8;
/// The `auth` line is short; anything longer is not one.
const MAX_AUTH_LINE: usize = 4096;

/// One place in a bounded count, given back on drop.
struct Slot(&'static AtomicUsize);

impl Slot {
    fn take(count: &'static AtomicUsize, max: usize) -> Option<Self> {
        if count.fetch_add(1, Ordering::SeqCst) >= max {
            count.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(count))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
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
pub(super) fn same_user(stream: &Stream) -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    let me = unsafe { libc::geteuid() };
    stream
        .peer_creds()
        .ok()
        .and_then(|creds| creds.euid())
        .is_some_and(|uid| uid == me)
}

/// The other end of the pipe runs as this user (Windows). Quill's own pipe
/// admits only this user already (`pipe_security`); the bridge needs the
/// check because another user could have created the pipe name first.
#[cfg(windows)]
pub(super) fn same_user(stream: &Stream) -> bool {
    stream
        .peer_creds()
        .ok()
        .and_then(|creds| creds.pid())
        .is_some_and(crate::pipe_security::is_current_user_process)
}

/// Read one line, unbuffered, so nothing past it is consumed.
pub(super) fn read_line_raw(stream: &Stream, max: usize) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut reader = stream;
    while line.len() < max {
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

/// Check the `auth` request; returns its id and the agent's name.
fn authenticate(stream: &Stream) -> Result<(Value, String), (Value, i64, &'static str)> {
    let line = read_line_raw(stream, MAX_AUTH_LINE).ok_or((
        Value::Null,
        code::UNAUTHORIZED,
        "Expected auth.",
    ))?;
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
    let agent = request
        .get("params")
        .and_then(|p| p.get("client"))
        .and_then(Value::as_str)
        .map(|name| {
            name.chars()
                .filter(|c| !c.is_control())
                .take(60)
                .collect::<String>()
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "An agent".to_string());
    Ok((id, agent))
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
    let Some(waiting) = Slot::take(&UNAUTHENTICATED, MAX_UNAUTHENTICATED) else {
        return;
    };
    let generation = access::generation();
    let _ = stream.set_recv_timeout(Some(AUTH_TIMEOUT));
    let (auth_id, agent) = match authenticate(&stream) {
        Ok(found) => found,
        Err((id, code, message)) => {
            let _ = write_line(&mut out, &error_line(&id, code, message));
            return;
        }
    };
    // Taken after auth, so a client that never authenticates (a Windows pipe
    // has no read timeout) cannot hold a slot.
    drop(waiting);
    if super::activity::refused(&agent) {
        let _ = write_line(
            &mut out,
            &error_line(&auth_id, code::ACCESS_OFF, REVOKED_MESSAGE),
        );
        return;
    }
    let Some(_slot) = Slot::take(&CONNECTIONS, MAX_CONNECTIONS) else {
        let _ = write_line(
            &mut out,
            &error_line(
                &auth_id,
                code::BUSY,
                "Too many agents are connected to Quill. Close one and retry.",
            ),
        );
        return;
    };
    if write_line(
        &mut out,
        &result_line(&auth_id, json!({"quill_version": crate::version::APP})),
    )
    .is_err()
    {
        return;
    }
    let presence = super::activity::connect(&agent);
    let conn = Conn {
        generation,
        id: presence.id(),
    };
    let _ = stream.set_recv_timeout(Some(IDLE_CHECK));
    let (recv, send) = stream.split();
    let send = Arc::new(Mutex::new(send));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let watching = Arc::new(AtomicU64::new(0));
    // A subscription's watcher stops with its connection.
    let _stop_watching = StopOnDrop(watching.clone());
    let mut reader = BufReader::new(recv);
    let mut line = Vec::new();
    loop {
        let on = conn.ended().is_none();
        // Every read is bounded, so a line past the limit ends the
        // connection however its bytes arrive.
        let room = (MAX_LINE_BYTES + 1).saturating_sub(line.len()) as u64;
        let read = (&mut reader).take(room).read_until(b'\n', &mut line);
        if line.len() > MAX_LINE_BYTES {
            return;
        }
        match read {
            Ok(0) => return,
            Ok(_) if line.ends_with(b"\n") => {}
            Ok(_) => return,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if !on {
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
        if let Some(message) = conn.ended() {
            let mut out = crate::locks::lock_or_recover(&send);
            let _ = write_line(
                &mut *out,
                &error_line(&Value::Null, code::ACCESS_OFF, message),
            );
            return;
        }
        dispatch(text, &send, &in_flight, &watching, conn, &agent);
    }
}

/// One authenticated connection: the switch's generation it opened under
/// and its presence id.
#[derive(Debug, Clone, Copy)]
struct Conn {
    generation: u64,
    id: u64,
}

impl Conn {
    /// Why the connection must end: Agent access went off (or the token
    /// was reset) since it opened, or the user revoked it.
    fn ended(self) -> Option<&'static str> {
        if !still_on(self.generation) {
            Some(OFF_MESSAGE)
        } else if super::activity::is_revoked(self.id) {
            Some(REVOKED_MESSAGE)
        } else {
            None
        }
    }
}

/// The connection's current watcher (0 = none). A watcher runs only while
/// this holds its own number, so an unsubscribe followed by a subscribe
/// retires the old one instead of running two.
struct StopOnDrop(Arc<AtomicU64>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(0, Ordering::SeqCst);
    }
}

static NEXT_WATCHER: AtomicU64 = AtomicU64::new(0);

/// `resources/subscribe` and `resources/unsubscribe`: only `quill://unread`
/// can be watched. While subscribed, a watcher thread sends
/// `notifications/resources/updated` whenever a message arrives in a granted
/// chat.
fn subscription<W: Write + Send + 'static>(
    request: &Value,
    id: &Value,
    send: &Arc<Mutex<W>>,
    watching: &Arc<AtomicU64>,
    conn: Conn,
) -> String {
    let uri = request
        .get("params")
        .and_then(|p| p.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if uri != super::guide::UNREAD_URI {
        return error_line(
            id,
            code::INVALID,
            "Only quill://unread can be subscribed to. For one chat, call wait_for_message.",
        );
    }
    let subscribe = request.get("method").and_then(Value::as_str) == Some("resources/subscribe");
    if !subscribe {
        watching.store(0, Ordering::SeqCst);
        return result_line(id, json!({}));
    }
    let mine = NEXT_WATCHER
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1)
        .max(1);
    if watching
        .compare_exchange(0, mine, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let send = send.clone();
        let flag = watching.clone();
        let started = crate::thread_guard::spawn_guarded("quill-agent-watch", move || {
            let note = json!({
                "jsonrpc": "2.0",
                "method": "notifications/resources/updated",
                "params": {"uri": super::guide::UNREAD_URI},
            })
            .to_string();
            let mut since = super::events::latest_seq();
            while flag.load(Ordering::SeqCst) == mine && conn.ended().is_none() {
                match super::tools::granted_message_arrived(&mut since, WATCH_WAIT) {
                    Ok(true) => {
                        let mut out = crate::locks::lock_or_recover(&send);
                        if write_line(&mut *out, &note).is_err() {
                            break;
                        }
                    }
                    Ok(false) => {}
                    // Signed out or not connected yet: look again later.
                    Err(_) => std::thread::sleep(WATCH_WAIT),
                }
            }
            let _ = flag.compare_exchange(mine, 0, Ordering::SeqCst, Ordering::SeqCst);
        });
        if started.is_err() {
            watching.store(0, Ordering::SeqCst);
            return error_line(id, code::INTERNAL, "Quill could not start watching. Retry.");
        }
    }
    result_line(id, json!({}))
}

/// How long a watcher waits before it checks the switch again.
const WATCH_WAIT: Duration = Duration::from_secs(10);

/// Agent access is on and has not been turned off since `generation`.
fn still_on(generation: u64) -> bool {
    access::enabled() && access::generation() == generation
}

/// One request in flight: frees its slot and writes its answer when
/// dropped, including when the tool panicked, so the agent always hears
/// back. An answer finished after the switch went off is not sent.
struct Reply<W: Write> {
    send: Arc<Mutex<W>>,
    in_flight: Arc<AtomicUsize>,
    id: Value,
    conn: Conn,
    line: Option<String>,
}

impl<W: Write> Drop for Reply<W> {
    fn drop(&mut self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let line = if let Some(message) = self.conn.ended() {
            error_line(&self.id, code::ACCESS_OFF, message)
        } else {
            self.line.take().unwrap_or_else(|| {
                error_line(
                    &self.id,
                    code::INTERNAL,
                    "Quill hit an internal error answering this request. Retry, or try another tool.",
                )
            })
        };
        let mut out = crate::locks::lock_or_recover(&self.send);
        let _ = write_line(&mut *out, &line);
    }
}

/// Answer one request on a thread of its own, so a slow call does not hold
/// up the rest.
fn dispatch<W: Write + Send + 'static>(
    text: String,
    send: &Arc<Mutex<W>>,
    in_flight: &Arc<AtomicUsize>,
    watching: &Arc<AtomicU64>,
    conn: Conn,
    agent: &str,
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
    if matches!(
        request.get("method").and_then(Value::as_str),
        Some("resources/subscribe" | "resources/unsubscribe")
    ) {
        let line = subscription(&request, &id, send, watching, conn);
        let mut out = crate::locks::lock_or_recover(send);
        let _ = write_line(&mut *out, &line);
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
    let mut reply = Reply {
        send: send.clone(),
        in_flight: in_flight.clone(),
        id,
        conn,
        line: None,
    };
    let agent = agent.to_string();
    // A failed spawn drops the closure, and with it `reply`, which answers.
    let _ = crate::thread_guard::spawn_guarded("quill-agent-request", move || {
        let _ = reply
            .line
            .insert(answer(&request, &reply.id, &agent, reply.conn.id));
    });
}

fn answer(request: &Value, id: &Value, agent: &str, conn: u64) -> String {
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
            result_line(id, super::tools::call_on(conn, name, &arguments, agent))
        }
        Some("resources/read") => {
            let uri = params
                .get("uri")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if uri == super::guide::UNREAD_URI {
                return match super::tools::unread_resource() {
                    Ok(result) => result_line(id, result),
                    Err(super::tools::ToolError(message)) => {
                        error_line(id, code::INTERNAL, &message)
                    }
                };
            }
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
