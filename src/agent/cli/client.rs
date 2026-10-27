//! One request to the running Quill: connect, authenticate, send a
//! `tools/call` (or any method), read its answer.

use super::{Exit, Failure};
use crate::agent::link::{self, Refused};
use crate::agent::server::code;
use crate::single_instance::Endpoint;
use interprocess::local_socket::Stream;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

impl From<Refused> for Failure {
    fn from(refused: Refused) -> Self {
        let exit = match &refused {
            Refused::NotRunning => Exit::NotRunning,
            Refused::Off => Exit::AccessOff,
            Refused::NoToken | Refused::StaleToken | Refused::OtherUser => Exit::Token,
            Refused::Other(_) => Exit::Connection,
        };
        let mut message = refused.message();
        if exit == Exit::NotRunning {
            message.push_str(" Or pass --start to start it.");
        }
        Failure::new(exit, message)
    }
}

/// Call tool `name` with `arguments` in the running Quill; returns the MCP
/// `tools/call` result.
pub fn call_tool(
    name: &str,
    arguments: &Value,
    start: bool,
    client: &str,
) -> Result<Value, Failure> {
    let root = crate::settings::safe_app_root().ok_or_else(no_data_folder)?;
    let endpoint = Endpoint::for_root(&root);
    request_at(
        &root,
        &endpoint,
        start,
        client,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// Without a data folder there is no token to read.
pub(super) fn no_data_folder() -> Failure {
    Failure::new(
        Exit::Token,
        "Quill's data folder was not found, so there is no agent token to read. Open Quill once, then retry.",
    )
}

/// Send one request to the Quill at `endpoint` (whose token is under
/// `root`) and return its `result`.
pub(super) fn request_at(
    root: &Path,
    endpoint: &Endpoint,
    start: bool,
    client: &str,
    method: &str,
    params: Value,
) -> Result<Value, Failure> {
    let stream = link::open(root, endpoint, start, Some(client))?;
    exchange(&stream, method, params)
}

const REQUEST_ID: i64 = 1;

fn exchange(stream: &Stream, method: &str, params: Value) -> Result<Value, Failure> {
    let lost = || {
        Failure::new(
            Exit::Connection,
            "Quill closed the connection: it quit, or Agent access was turned off. Retry.",
        )
    };
    let line = json!({"jsonrpc": "2.0", "id": REQUEST_ID, "method": method, "params": params});
    let mut writer = stream;
    writer
        .write_all(format!("{line}\n").as_bytes())
        .and_then(|()| writer.flush())
        .map_err(|_| lost())?;
    let mut reader = BufReader::new(stream);
    loop {
        let mut answer = String::new();
        if reader.read_line(&mut answer).map_err(|_| lost())? == 0 {
            return Err(lost());
        }
        let Ok(answer) = serde_json::from_str::<Value>(&answer) else {
            continue;
        };
        // Notifications and answers to anything else are not ours.
        if answer.get("id").and_then(Value::as_i64) != Some(REQUEST_ID) {
            if let Some(error) = answer.get("error") {
                return Err(rpc_failure(error));
            }
            continue;
        }
        if let Some(error) = answer.get("error") {
            return Err(rpc_failure(error));
        }
        return Ok(answer.get("result").cloned().unwrap_or(Value::Null));
    }
}

fn rpc_failure(error: &Value) -> Failure {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Quill answered with an error. Retry.")
        .to_string();
    let exit = match error.get("code").and_then(Value::as_i64) {
        Some(code::ACCESS_OFF) => Exit::AccessOff,
        Some(code::UNAUTHORIZED) => Exit::Token,
        Some(code::INVALID | code::NO_METHOD | code::PARSE) => Exit::Usage,
        _ => Exit::Connection,
    };
    Failure::new(exit, message)
}
