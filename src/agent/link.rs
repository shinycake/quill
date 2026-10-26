//! Reaching the running Quill from another process: connect to its socket,
//! start it when asked, and authenticate. `quill mcp` ([`super::mcp`]) and
//! `quill agent` ([`super::cli`]) both connect through here.

use super::server::{self, HEADER, code};
use crate::single_instance::Endpoint;
use interprocess::local_socket::{Stream, prelude::*};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long to wait for a Quill this process started to open its socket.
const LAUNCH_WAIT: Duration = Duration::from_secs(45);
const CONNECT_POLL: Duration = Duration::from_millis(250);

/// Why Quill could not be reached or would not let this client in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Nothing listens on the socket.
    NotRunning,
    /// Agent access is off (read from disk, or Quill said so).
    Off,
    /// Quill runs, but no token is on disk: it is still starting, or it was
    /// started before the switch could publish one.
    NoToken,
    /// The token on disk is not this launch's: a Quill that just started
    /// writes its own once its socket listens.
    StaleToken,
    /// The socket belongs to another user.
    OtherUser,
    /// Anything else, worded for the agent.
    Other(String),
}

impl Refused {
    /// The next step, for an agent or a person at a terminal.
    pub fn message(&self) -> String {
        match self {
            Self::NotRunning => "Quill is not running. Open Quill, then retry.".into(),
            Self::Off => server::OFF_MESSAGE.into(),
            Self::NoToken => "Quill is running but has not published its agent token. Wait a moment and retry; if it keeps failing, ask the user to restart Quill.".into(),
            Self::StaleToken => "Quill refused this client's token. Retry; if it keeps failing, ask the user to restart Quill.".into(),
            Self::OtherUser => "The Quill socket on this computer belongs to another user. Ask the user to check Quill.".into(),
            Self::Other(message) => message.clone(),
        }
    }
}

/// The executable to start or to name in a setup command. An AppImage runs
/// from a mount that goes away with its process, so it is the AppImage file.
pub fn executable() -> Option<std::path::PathBuf> {
    cfg!(target_os = "linux")
        .then(|| std::env::var_os("APPIMAGE"))
        .flatten()
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

/// Start Quill detached from this process: no shared stdio (it would corrupt
/// an MCP stream) and its own process group, so it outlives the caller.
fn launch_quill() -> Result<(), Refused> {
    let exe =
        executable().ok_or_else(|| Refused::Other("Could not find Quill to start it.".into()))?;
    let mut command = std::process::Command::new(exe);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        // Windows passes every inheritable handle to the child, and the MCP
        // client's pipes on our stdio are usually inheritable: a Quill that
        // kept them open would keep the client waiting for EOF.
        use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            // SAFETY: GetStdHandle takes a constant and returns a handle or
            // null/INVALID; SetHandleInformation fails harmlessly on either.
            unsafe {
                SetHandleInformation(GetStdHandle(which), HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
    command.spawn().map(drop).map_err(|e| {
        Refused::Other(format!(
            "Could not start Quill ({e}). Ask the user to open Quill, then retry."
        ))
    })
}

/// Connect to the running Quill; with `start`, start it first when it is
/// not running and wait for its socket.
pub fn connect(root: &Path, endpoint: &Endpoint, start: bool) -> Result<Stream, Refused> {
    if let Ok(stream) = crate::single_instance::connect(endpoint) {
        return Ok(stream);
    }
    // Starting Quill would only end in the same refusal.
    if !super::access::enabled_on_disk(root) {
        return Err(Refused::Off);
    }
    if !start {
        return Err(Refused::NotRunning);
    }
    launch_quill()?;
    let deadline = Instant::now() + LAUNCH_WAIT;
    while Instant::now() < deadline {
        std::thread::sleep(CONNECT_POLL);
        if let Ok(stream) = crate::single_instance::connect(endpoint) {
            return Ok(stream);
        }
    }
    Err(Refused::Other(
        "Started Quill, but it did not get ready within 45 seconds. Ask the user to check Quill, then retry.".into(),
    ))
}

/// Check who owns the socket, then send the header and this launch's token
/// with `client` (the name Quill shows the user).
pub fn handshake(root: &Path, stream: Stream, client: Option<&str>) -> Result<Stream, Refused> {
    // The socket may sit in a shared temp folder; only talk to our own user.
    if !server::same_user(&stream) {
        return Err(Refused::OtherUser);
    }
    let token = super::access::read_published_token(root).ok_or_else(|| {
        if super::access::enabled_on_disk(root) {
            Refused::NoToken
        } else {
            Refused::Off
        }
    })?;
    let mut writer = &stream;
    let hello = format!(
        "{HEADER}\n{}\n",
        json!({"jsonrpc": "2.0", "id": 0, "method": "auth", "params": {"token": token, "client": client}})
    );
    writer
        .write_all(hello.as_bytes())
        .and_then(|()| writer.flush())
        .map_err(|_| Refused::Other("Quill closed the connection. Retry.".into()))?;
    let _ = stream.set_recv_timeout(Some(Duration::from_secs(10)));
    let answer = server::read_line_raw(&stream, 64 * 1024)
        .and_then(|line| serde_json::from_str::<Value>(&line).ok())
        .ok_or_else(|| Refused::Other("Quill did not answer. Retry.".into()))?;
    if let Some(error) = answer.get("error") {
        return Err(match error.get("code").and_then(Value::as_i64) {
            Some(code::UNAUTHORIZED) => Refused::StaleToken,
            Some(code::ACCESS_OFF) => Refused::Off,
            _ => Refused::Other(
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Quill refused the connection. Retry.")
                    .to_string(),
            ),
        });
    }
    let _ = stream.set_recv_timeout(None);
    Ok(stream)
}

/// Connect and authenticate, retrying once when the token is stale (a Quill
/// that just started).
pub fn open(
    root: &Path,
    endpoint: &Endpoint,
    start: bool,
    client: Option<&str>,
) -> Result<Stream, Refused> {
    let mut retried = false;
    loop {
        let stream = connect(root, endpoint, start)?;
        match handshake(root, stream, client) {
            Err(Refused::StaleToken | Refused::NoToken) if !retried => {
                retried = true;
                std::thread::sleep(Duration::from_secs(1));
            }
            other => return other,
        }
    }
}
