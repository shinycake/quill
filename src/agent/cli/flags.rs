//! The command line, derived from the tool registry: which command names a
//! tool, how flags become its JSON arguments, and the help text.

use super::{Failure, Global};
use serde_json::{Map, Value, json};

/// Short names for the tools people reach for most. Every tool also answers
/// to its own name, with `-` or `_`.
pub const ALIASES: &[(&str, &str)] = &[
    ("status", "quill_status"),
    ("chats", "list_chats"),
    ("folders", "list_folders"),
    ("unread", "list_unread"),
    ("messages", "list_messages"),
    ("read", "read_history"),
    ("search", "search_messages"),
    ("message", "get_message"),
    ("info", "get_chat_info"),
    ("send", "send_message"),
    ("wait", "wait_for_message"),
    ("screenshot", "take_screenshot"),
];

/// Flags every command takes; no tool argument may use these names.
#[cfg(test)]
pub const RESERVED: &[&str] = &["json", "args", "start", "help", "images", "client"];

/// One tool as the command line sees it, from `tools::catalog()`.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub title: String,
    pub description: String,
    pub schema: Value,
    pub read_only: bool,
}

impl ToolSpec {
    fn from_catalog(entry: &Value) -> Option<Self> {
        Some(Self {
            name: entry.get("name")?.as_str()?.to_string(),
            title: entry.get("title")?.as_str()?.to_string(),
            description: entry.get("description")?.as_str()?.to_string(),
            schema: entry.get("inputSchema")?.clone(),
            read_only: entry
                .get("annotations")
                .and_then(|a| a.get("readOnlyHint"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// The short command for this tool, if it has one.
    pub fn alias(&self) -> Option<&'static str> {
        ALIASES
            .iter()
            .find(|(_, tool)| *tool == self.name)
            .map(|(alias, _)| *alias)
    }

    fn properties(&self) -> Vec<(&String, &Value)> {
        self.schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|p| p.iter().collect())
            .unwrap_or_default()
    }

    fn required(&self) -> Vec<&str> {
        self.schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    /// The description up to "Example input".
    pub fn summary(&self) -> &str {
        self.description
            .split("Example input")
            .next()
            .unwrap_or_default()
            .trim()
    }

    /// The first example input, parsed.
    pub fn example(&self) -> Option<Value> {
        let after = self
            .description
            .split_once("Example input:")?
            .1
            .trim_start();
        serde_json::Deserializer::from_str(after)
            .into_iter::<Value>()
            .next()?
            .ok()
    }

    /// What the description says the output is.
    pub fn output(&self) -> &str {
        self.description
            .split_once("Output:")
            .map_or("", |(_, rest)| rest.trim())
    }
}

/// Every tool, in registry order.
pub fn tools() -> Vec<ToolSpec> {
    crate::agent::tools::catalog()
        .iter()
        .filter_map(ToolSpec::from_catalog)
        .collect()
}

/// The tool a command names: an alias, or a tool name with `-` or `_`.
pub fn find_tool(command: &str) -> Option<ToolSpec> {
    let name = ALIASES
        .iter()
        .find(|(alias, _)| *alias == command)
        .map_or_else(
            || command.replace('-', "_"),
            |(_, tool)| (*tool).to_string(),
        );
    tools().into_iter().find(|tool| tool.name == name)
}

/// Pull the global options out of the arguments, wherever they are.
pub fn split_global(args: &[String]) -> Result<(Global, Vec<String>), Failure> {
    let mut global = Global::default();
    let mut rest = Vec::new();
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => global.help = true,
            "--start" => global.start = true,
            // `--json '{…}'` is the arguments of a tool; plain `--json` is output.
            "--json"
                if iter
                    .peek()
                    .is_some_and(|next| next.trim_start().starts_with('{')) =>
            {
                global.json = true;
                rest.push(arg.clone());
                if let Some(next) = iter.next() {
                    rest.push(next.clone());
                }
            }
            "--json" => global.json = true,
            "--images" | "--client" => {
                let value = iter
                    .next()
                    .ok_or_else(|| Failure::usage(format!("{arg} needs a value.")))?;
                if arg == "--images" {
                    global.images = Some(value.clone());
                } else {
                    global.client = Some(value.clone());
                }
            }
            _ => rest.push(arg.clone()),
        }
    }
    Ok((global, rest))
}

/// Turn `--flag value` pairs into the tool's JSON arguments, typed by its
/// schema. `--json '{…}'` or `--args '{…}'` gives them all at once; flags
/// after it override single fields.
pub fn parse_tool_args(tool: &ToolSpec, args: &[String]) -> Result<Value, Failure> {
    let properties = tool.properties();
    let mut arguments = Map::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let Some(flag) = arg.strip_prefix("--") else {
            return Err(Failure::usage(format!(
                "Unexpected {arg}. Pass arguments as flags, like --chat saved. See quill agent help {}.",
                tool.name
            )));
        };
        let (flag, inline) = match flag.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_string())),
            None => (flag, None),
        };
        if flag == "json" || flag == "args" {
            let text = inline
                .or_else(|| iter.next().cloned())
                .ok_or_else(|| Failure::usage(format!("--{flag} needs a JSON object.")))?;
            let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&text) else {
                return Err(Failure::usage(format!(
                    "--{flag} takes a JSON object, like --{flag} '{{\"chat\": \"saved\"}}'."
                )));
            };
            arguments.extend(object);
            continue;
        }
        let key = flag.replace('-', "_");
        let (key, negated) = match key.strip_prefix("no_") {
            Some(base) if !properties.iter().any(|(k, _)| **k == key) => (base.to_string(), true),
            _ => (key, false),
        };
        let Some((name, schema)) = properties.iter().find(|(k, _)| **k == key) else {
            return Err(Failure::usage(format!(
                "{} has no flag --{flag}. Its flags: {}.",
                tool.name,
                flag_list(tool)
            )));
        };
        let value = if is_boolean(schema) && inline.is_none() {
            Value::Bool(!negated)
        } else {
            let text = inline
                .or_else(|| iter.next().cloned())
                .ok_or_else(|| Failure::usage(format!("--{flag} needs a value.")))?;
            parse_value(schema, &text).ok_or_else(|| {
                Failure::usage(format!("--{flag}: {text} is not {}.", type_hint(schema)))
            })?
        };
        match (arguments.get_mut(name.as_str()), value) {
            // A repeated array flag adds to the list.
            (Some(Value::Array(list)), Value::Array(more)) => list.extend(more),
            (_, value) => {
                arguments.insert((*name).clone(), value);
            }
        }
    }
    let missing: Vec<&str> = tool
        .required()
        .into_iter()
        .filter(|key| !arguments.contains_key(*key))
        .collect();
    if !missing.is_empty() {
        return Err(Failure::usage(format!(
            "{} needs {}. See quill agent help {}.",
            tool.name,
            missing
                .iter()
                .map(|k| format!("--{}", k.replace('_', "-")))
                .collect::<Vec<_>>()
                .join(" and "),
            tool.name
        )));
    }
    Ok(Value::Object(arguments))
}

fn types(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn is_boolean(schema: &Value) -> bool {
    types(schema) == ["boolean"]
}

/// One flag value, typed by the property's schema.
fn parse_value(schema: &Value, text: &str) -> Option<Value> {
    let kinds = types(schema);
    if kinds.is_empty() {
        return Some(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.into())));
    }
    for kind in &kinds {
        let parsed = match *kind {
            "integer" => text.trim().parse::<i64>().ok().map(Value::from),
            "number" => text
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(Value::from),
            "boolean" => match text.trim() {
                "true" | "yes" | "1" => Some(Value::Bool(true)),
                "false" | "no" | "0" => Some(Value::Bool(false)),
                _ => None,
            },
            "array" => parse_array(schema, text),
            "object" => serde_json::from_str::<Value>(text)
                .ok()
                .filter(Value::is_object),
            "null" => (text == "null").then_some(Value::Null),
            _ => None,
        };
        if parsed.is_some() {
            return parsed;
        }
    }
    kinds
        .contains(&"string")
        .then(|| Value::String(text.to_string()))
}

/// `[1, 2]` as JSON, or `1,2` split on commas when the items are numbers,
/// or one item otherwise (repeat the flag for more).
fn parse_array(schema: &Value, text: &str) -> Option<Value> {
    if text.trim_start().starts_with('[') {
        return serde_json::from_str::<Value>(text)
            .ok()
            .filter(Value::is_array);
    }
    let items = schema.get("items").cloned().unwrap_or_else(|| json!({}));
    let numeric = types(&items)
        .iter()
        .any(|t| matches!(*t, "integer" | "number"));
    let parts: Vec<&str> = if numeric {
        text.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        vec![text]
    };
    parts
        .into_iter()
        .map(|part| parse_value(&items, part))
        .collect::<Option<Vec<_>>>()
        .map(Value::Array)
}

fn type_hint(schema: &Value) -> String {
    let kinds = types(schema);
    if kinds.is_empty() {
        "JSON".into()
    } else {
        format!("a {}", kinds.join(" or "))
    }
}

fn flag_list(tool: &ToolSpec) -> String {
    let flags: Vec<String> = tool
        .properties()
        .iter()
        .map(|(k, _)| format!("--{}", k.replace('_', "-")))
        .collect();
    if flags.is_empty() {
        "none".into()
    } else {
        flags.join(", ")
    }
}

/// Wrap `text` at `width` columns, each line starting with `indent`.
pub fn wrap(text: &str, width: usize, indent: &str) -> String {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(format!("{indent}{line}"));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(format!("{indent}{line}"));
    }
    lines.join("\n")
}

/// Quote a word for a POSIX shell when it needs it.
pub fn shell_word(word: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._-+:@,".contains(c);
    if !word.is_empty() && word.chars().all(plain) {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// The example input as flags (unquoted words): one flag per field when
/// every value is plain, `--args '{…}'` otherwise.
pub fn example_args(tool: &ToolSpec) -> Option<Vec<String>> {
    let example = tool.example()?;
    let object = example.as_object()?;
    let mut words = Vec::new();
    for (key, value) in object {
        let flag = format!("--{}", key.replace('_', "-"));
        match value {
            Value::Bool(true) => words.push(flag),
            Value::Bool(false) => words.push(format!("--no-{}", key.replace('_', "-"))),
            Value::String(s) if !s.starts_with('-') => words.extend([flag, s.clone()]),
            Value::Number(n) => words.extend([flag, n.to_string()]),
            Value::Array(items) if items.iter().all(Value::is_number) => {
                let list: Vec<String> = items.iter().map(Value::to_string).collect();
                words.extend([flag, list.join(",")]);
            }
            _ => return Some(vec!["--args".into(), example.to_string()]),
        }
    }
    Some(words)
}

/// The example input as a command line to paste.
pub fn example_command(tool: &ToolSpec) -> Option<String> {
    let command = tool
        .alias()
        .map_or_else(|| tool.name.clone(), str::to_string);
    let mut words = vec!["quill agent".to_string(), command];
    words.extend(example_args(tool)?.iter().map(|w| shell_word(w)));
    Some(words.join(" "))
}

/// "One of a, b. 1 to 99, default 50.", from the schema.
fn limits(schema: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let values: Vec<String> = values
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
            .collect();
        parts.push(format!("One of {}.", values.join(", ")));
    }
    match (schema.get("minimum"), schema.get("maximum")) {
        (Some(min), Some(max)) => parts.push(format!("{min} to {max}.")),
        (Some(min), None) => parts.push(format!("At least {min}.")),
        (None, Some(max)) => parts.push(format!("At most {max}.")),
        (None, None) => {}
    }
    if let Some(default) = schema.get("default") {
        parts.push(format!("Default {default}."));
    }
    parts.join(" ")
}

/// `quill agent help <tool>`.
pub fn tool_help(tool: &ToolSpec) -> String {
    let command = tool
        .alias()
        .map_or_else(|| tool.name.clone(), str::to_string);
    let mut out = format!("quill agent {command}: {}\n", tool.title);
    if tool.alias().is_some() {
        out.push_str(&format!("Also: quill agent {}\n", tool.name));
    }
    out.push('\n');
    out.push_str(&wrap(tool.summary(), 76, ""));
    out.push_str("\n\nFlags:\n");
    let required = tool.required();
    let mut properties = tool.properties();
    // Required flags first.
    properties.sort_by_key(|(key, _)| !required.contains(&key.as_str()));
    if properties.is_empty() {
        out.push_str("  (none)\n");
    }
    for (key, schema) in &properties {
        let value = if is_boolean(schema) {
            String::new()
        } else {
            format!(" <{}>", types(schema).join("|"))
        };
        let mut about = schema
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if required.contains(&key.as_str()) {
            about = format!("Required. {about}");
        }
        let limits = limits(schema);
        if !limits.is_empty() {
            about = format!("{} {limits}", about.trim_end());
        }
        let flag = key.replace('_', "-");
        if is_boolean(schema) && schema.get("default") == Some(&Value::Bool(true)) {
            out.push_str(&format!("  --{flag}, --no-{flag}\n"));
        } else {
            out.push_str(&format!("  --{flag}{value}\n"));
        }
        if !about.trim().is_empty() {
            out.push_str(&wrap(&about, 72, "      "));
            out.push('\n');
        }
    }
    out.push_str("  --args '{…}'\n      Every argument as one JSON object; flags after it override fields.\n");
    if let Some(example) = example_command(tool) {
        out.push_str(&format!("\nExample:\n  {example}\n"));
    }
    if !tool.output().is_empty() {
        out.push_str("\nOutput (with --json):\n");
        out.push_str(&wrap(tool.output(), 76, "  "));
        out.push('\n');
    }
    out
}

/// `quill agent --help`.
pub fn main_help() -> String {
    let mut out = String::from(
        "quill agent: use your Telegram account from scripts and agents, through the Quill running on this computer.

Usage: quill agent <command> [flags] [--json] [--start]

Agent access must be on in Quill (Settings > Advanced > Agent access). Each
command is one call to a tool in Quill, under the same grants, pacing and
confirmations as MCP agents. MCP apps use `quill mcp` instead.

Commands:
",
    );
    for (alias, tool) in ALIASES {
        let title = find_tool(tool).map(|t| t.title).unwrap_or_default();
        out.push_str(&format!("  {alias:<14}{title} ({tool})\n"));
    }
    out.push_str(
        "  call <tool>   Call any tool: quill agent call <tool> --json '{…}'
  describe      Every tool, resource and exit code as JSON
  guide         The guide for agents (also the quill://guide resource)
  install-skill Install the Quill skill for Claude Code
  connect       Set up Claude Code, Claude Desktop or Cursor
  help <cmd>    Help for a command or tool

Every tool is also a command under its own name:
",
    );
    for tool in tools() {
        out.push_str(&format!("  {:<20}{}\n", tool.name, tool.title));
    }
    out.push_str(
        "
Options:
  --json         Print JSON (the tool's structured result, or {\"error\": …})
  --start        Start Quill if it is not running
  --images DIR   Save images a tool returns (take_screenshot) as PNG files
  --client NAME  The name Quill shows the user for this client
  -h, --help     Help

Exit codes:
",
    );
    for exit in super::Exit::ALL {
        out.push_str(&format!("  {}  {}\n", exit.code(), exit.meaning()));
    }
    out.push_str("\nRead the guide first: quill agent guide.\n");
    out
}

/// Help for the commands that are not tools.
pub fn builtin_help(topic: &str) -> Option<String> {
    let text = match topic {
        "call" => {
            "quill agent call <tool> --json '{…}'

Call any tool with its arguments as one JSON object, and print the tool's
structured result as JSON. Use --args '{…}' instead of --json to get the
readable text. quill agent describe lists every tool's input schema.

Example:
  quill agent call read_history --json '{\"chat\": \"saved\", \"limit\": 10}'
"
        }
        "guide" => {
            "quill agent guide

Prints the guide for agents: what Quill can do, grants, every tool, recipes,
errors and the rules. It is the same text as the quill://guide resource, and
works when Quill is not running.
"
        }
        "describe" => {
            "quill agent describe

Prints, as JSON, every tool (name, title, description, input schema,
example, output), the resources, and the exit codes. It comes from the
same registry Quill serves, and works when Quill is not running.
"
        }
        "install-skill" => {
            "quill agent install-skill [--dir DIR] [--force] [--print]

Writes the Quill Agent Skill (SKILL.md and guide.md) to ~/.claude/skills/quill,
or to DIR. Running it again updates files you haven't changed. If you edited
one, it shows the difference and keeps your version; --force replaces it
and saves yours as <file>.bak. --print writes SKILL.md to the terminal.
"
        }
        "connect" => {
            "quill agent connect claude-code|claude-desktop|cursor [--print] [--yes]

Shows how to add Quill to an MCP app, with this Quill's real path.
  claude-code     prints the `claude mcp add` command
  claude-desktop  shows the entry for claude_desktop_config.json
  cursor          shows the entry for ~/.cursor/mcp.json
--yes writes the entry into the app's config file. It merges with what is
there and saves a backup first. --print prints only the command or JSON.

Config files:
  Claude Desktop  macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
                  Windows: %APPDATA%\\Claude\\claude_desktop_config.json
                  Linux (unofficial builds): ~/.config/Claude/claude_desktop_config.json
  Cursor          ~/.cursor/mcp.json (Windows: %USERPROFILE%\\.cursor\\mcp.json)
"
        }
        _ => return None,
    };
    Some(text.to_string())
}
