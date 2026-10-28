//! `quill agent …`: the command line for scripts and agents that don't
//! speak MCP. It is a thin client: every tool command sends one `tools/call`
//! to the running Quill over the same socket and token as `quill mcp`
//! ([`super::link`]), and the commands, flags and help come from the tool
//! registry ([`super::tools::catalog`]), so they cannot drift from it.
//!
//! Exit codes are [`Exit`]; `quill agent --help` and the guide list them.

mod client;
mod connect;
mod describe;
mod files;
mod flags;
mod skill;

use serde_json::{Value, json};
use std::io::Write;

/// The setup lines the Agent access page shows, built like the ones
/// `quill agent connect` prints. Each is `(shell label, line)`; the label is
/// empty unless PowerShell and Command Prompt need different lines.
pub struct SetupLines {
    pub claude_code: Vec<(&'static str, String)>,
    pub claude_desktop: Vec<(&'static str, String)>,
    pub cursor: Vec<(&'static str, String)>,
    pub mcp: Vec<(&'static str, String)>,
}

/// [`SetupLines`] for this Quill. On Windows the `quill agent` lines run the
/// console helper next to `quill.exe`, which waits and passes the exit code
/// on.
pub fn setup_lines() -> SetupLines {
    let windows = cfg!(windows);
    let exe = super::link::executable();
    let gui = exe
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "quill".into());
    let cli = match &exe {
        Some(path) if windows => path
            .with_file_name("quill-cli.exe")
            .to_string_lossy()
            .into_owned(),
        _ => gui.clone(),
    };
    SetupLines {
        claude_code: connect::claude_code_commands(&gui, windows),
        // Typed into a terminal: the path is the command.
        claude_desktop: connect::command_lines(&cli, windows, "agent connect claude-desktop --yes"),
        cursor: connect::command_lines(&cli, windows, "agent connect cursor --yes"),
        // Pasted into an app's server settings, not run by a shell.
        mcp: connect::shell_lines(&gui, windows, |quoted| format!("{quoted} mcp")),
    }
}

/// How `quill agent` ended. The numbers are part of its interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Ok,
    /// The tool ran and answered with an error (not granted, not found...).
    ToolError,
    /// The command line was wrong.
    Usage,
    /// Quill is not running (pass --start to start it).
    NotRunning,
    /// Agent access is off in Quill.
    AccessOff,
    /// This client could not authenticate: no token on disk, a refused
    /// token, no data folder, or another user's Quill.
    Token,
    /// "Let agents drive Quill's window" is off (UI tools only).
    WindowControlOff,
    /// Quill closed the connection or did not answer.
    Connection,
    /// A local file could not be read or written, or holds edits that
    /// --force would replace.
    LocalFile,
}

impl Exit {
    pub const ALL: [Exit; 9] = [
        Exit::Ok,
        Exit::ToolError,
        Exit::Usage,
        Exit::NotRunning,
        Exit::AccessOff,
        Exit::Token,
        Exit::WindowControlOff,
        Exit::Connection,
        Exit::LocalFile,
    ];

    pub const fn code(self) -> i32 {
        match self {
            Exit::Ok => 0,
            Exit::ToolError => 1,
            Exit::Usage => 2,
            Exit::NotRunning => 3,
            Exit::AccessOff => 4,
            Exit::Token => 5,
            Exit::WindowControlOff => 6,
            Exit::Connection => 7,
            Exit::LocalFile => 8,
        }
    }

    /// The name scripts see in `{"error": {"kind": …}}`.
    pub const fn kind(self) -> &'static str {
        match self {
            Exit::Ok => "ok",
            Exit::ToolError => "tool_error",
            Exit::Usage => "usage",
            Exit::NotRunning => "not_running",
            Exit::AccessOff => "access_off",
            Exit::Token => "token",
            Exit::WindowControlOff => "window_control_off",
            Exit::Connection => "connection",
            Exit::LocalFile => "local_file",
        }
    }

    /// What it means and what to do, for help and `describe`.
    pub const fn meaning(self) -> &'static str {
        match self {
            Exit::Ok => "Done.",
            Exit::ToolError => {
                "The tool answered with an error, such as \"Chat not granted\". The message says what to do next."
            }
            Exit::Usage => "The command or its flags are wrong. See quill agent help <command>.",
            Exit::NotRunning => "Quill is not running. Open Quill, or pass --start to start it.",
            Exit::AccessOff => {
                concat!(
                    "Agent access is off. The user turns it on in Quill: ",
                    switch_place!(),
                    "."
                )
            }
            Exit::Token => {
                "This client could not authenticate: Quill has no agent token on disk yet or refused it, Quill's data folder was not found, or the Quill answering is another user's Quill. Retry; if it keeps failing, restart Quill."
            }
            Exit::WindowControlOff => {
                "\"Let agents drive Quill's window\" is off, so the UI tools refuse. The chat tools still work."
            }
            Exit::Connection => "Quill closed the connection or did not answer. Retry.",
            Exit::LocalFile => {
                "A local file could not be read or written, or it has edits that only --force replaces."
            }
        }
    }
}

/// A failed command: the exit code and the message for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub exit: Exit,
    pub message: String,
}

impl Failure {
    pub fn new(exit: Exit, message: impl Into<String>) -> Self {
        Self {
            exit,
            message: message.into(),
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self::new(Exit::Usage, message)
    }
}

/// Options any command takes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Global {
    /// Print JSON instead of text.
    pub json: bool,
    /// Start Quill when it is not running.
    pub start: bool,
    /// Asked for help.
    pub help: bool,
    /// Where to save images a tool returns (take_screenshot).
    pub images: Option<String>,
    /// The name Quill shows the user for this client.
    pub client: Option<String>,
}

/// What a command printed: text for people, or JSON for scripts.
pub(crate) enum Output {
    Text(String),
    Json(Value),
}

/// Run `quill agent` with the arguments after `agent`. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let (global, rest) = match flags::split_global(args) {
        Ok(split) => split,
        // The options read before the error still count: --json gets JSON.
        Err((global, failure)) => return report(&global, &failure),
    };
    match dispatch(&global, &rest) {
        Ok(output) => {
            print_output(&output);
            Exit::Ok.code()
        }
        Err(failure) => report(&global, &failure),
    }
}

fn dispatch(global: &Global, rest: &[String]) -> Result<Output, Failure> {
    let Some((command, tail)) = rest.split_first() else {
        if global.help {
            return Ok(Output::Text(flags::main_help()));
        }
        return Err(Failure::usage(flags::main_help()));
    };
    match command.as_str() {
        "help" => match tail.first() {
            Some(topic) => help_for(topic).map(Output::Text),
            None => Ok(Output::Text(flags::main_help())),
        },
        "call" if global.help => help_for(tail.first().unwrap_or(command)).map(Output::Text),
        _ if global.help => help_for(command).map(Output::Text),
        "describe" => Ok(Output::Json(describe::describe())),
        "guide" => Ok(Output::Text(crate::agent::guide::GUIDE.to_string())),
        "install-skill" => skill::run(global, tail),
        "connect" => connect::run(global, tail),
        "call" => {
            let (tool, flag_args) = tail.split_first().ok_or_else(|| {
                Failure::usage("Name the tool: quill agent call <tool> --json '{…}'. quill agent --help lists the tools.")
            })?;
            let tool = flags::find_tool(tool).ok_or_else(|| unknown(tool))?;
            run_tool(global, &tool, flag_args)
        }
        other => {
            let tool = flags::find_tool(other).ok_or_else(|| unknown(other))?;
            run_tool(global, &tool, tail)
        }
    }
}

fn unknown(command: &str) -> Failure {
    Failure::usage(format!(
        "quill agent has no command or tool named {command}. Run quill agent --help to see them."
    ))
}

fn help_for(topic: &str) -> Result<String, Failure> {
    if let Some(text) = flags::builtin_help(topic) {
        return Ok(text);
    }
    flags::find_tool(topic)
        .map(|tool| flags::tool_help(&tool))
        .ok_or_else(|| unknown(topic))
}

/// Parse the flags against the tool's schema, call it in Quill and shape
/// the answer.
fn run_tool(
    global: &Global,
    tool: &flags::ToolSpec,
    flag_args: &[String],
) -> Result<Output, Failure> {
    let arguments = flags::parse_tool_args(tool, flag_args)?;
    let client = global.client.as_deref().unwrap_or("Command line");
    let result = client::call_tool(&tool.name, &arguments, global.start, client)?;
    let images = save_images(global, &result)?;
    shape(&result, global.json, &images)
}

/// Write the image blocks of a tool result to `--images DIR`.
fn save_images(global: &Global, result: &Value) -> Result<Vec<String>, Failure> {
    use base64::Engine as _;
    let blocks: Vec<&Value> = result
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("image"))
                .collect()
        })
        .unwrap_or_default();
    let Some(dir) = global.images.as_deref().filter(|_| !blocks.is_empty()) else {
        return Ok(Vec::new());
    };
    let dir = std::path::Path::new(dir);
    std::fs::create_dir_all(dir).map_err(|e| {
        Failure::new(
            Exit::LocalFile,
            format!("Could not create {} ({e}).", dir.display()),
        )
    })?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let mut saved = Vec::new();
    for (n, block) in blocks.iter().enumerate() {
        let data = block
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            continue;
        };
        let path = dir.join(format!("quill-{stamp}-{}.png", n.saturating_add(1)));
        std::fs::write(&path, bytes).map_err(|e| {
            Failure::new(
                Exit::LocalFile,
                format!("Could not write {} ({e}).", path.display()),
            )
        })?;
        saved.push(path.display().to_string());
    }
    Ok(saved)
}

/// A `tools/call` result as output, or the failure it carries.
fn shape(result: &Value, json: bool, images: &[String]) -> Result<Output, Failure> {
    let text: Vec<&str> = result
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let text = text.join("\n");
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(Failure::new(classify_tool_error(&text), text));
    }
    let image_count = result
        .get("content")
        .and_then(Value::as_array)
        .map_or(0, |b| {
            b.iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("image"))
                .count()
        });
    if json {
        let mut data = result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| json!({"text": text}));
        if !images.is_empty()
            && let Some(object) = data.as_object_mut()
        {
            object.insert("saved_images".into(), json!(images));
        }
        return Ok(Output::Json(data));
    }
    let mut out = text;
    if !images.is_empty() {
        for path in images {
            out.push_str(&format!("\nSaved image: {path}"));
        }
    } else if image_count > 0 {
        out.push_str(&format!(
            "\n({image_count} image(s) not shown; pass --images DIR to save them.)"
        ));
    }
    Ok(Output::Text(out))
}

/// The exit code a tool's error message maps to.
fn classify_tool_error(message: &str) -> Exit {
    if message.starts_with(super::ui_driver::policy::DRIVER_OFF) {
        Exit::WindowControlOff
    } else if message.starts_with("Agent access is off")
        || message.starts_with("Agent access was turned off")
    {
        Exit::AccessOff
    } else {
        Exit::ToolError
    }
}

fn print_output(output: &Output) {
    let mut out = std::io::stdout().lock();
    let _ = match output {
        Output::Text(text) => writeln!(out, "{}", text.trim_end()),
        Output::Json(value) => writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        ),
    };
}

/// Print a failure (JSON on stdout with --json, text on stderr otherwise)
/// and return its exit code.
fn report(global: &Global, failure: &Failure) -> i32 {
    if global.json {
        print_output(&Output::Json(error_json(failure)));
    } else {
        let _ = writeln!(std::io::stderr().lock(), "{}", failure.message.trim_end());
    }
    failure.exit.code()
}

fn error_json(failure: &Failure) -> Value {
    json!({"error": {
        "kind": failure.exit.kind(),
        "exit_code": failure.exit.code(),
        "message": failure.message,
    }})
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "socket_tests.rs"]
mod socket_tests;

#[cfg(test)]
#[path = "flags_tests.rs"]
mod flags_tests;

#[cfg(test)]
#[path = "files_tests.rs"]
mod files_tests;
