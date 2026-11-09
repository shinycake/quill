//! The tools agents call, as MCP describes them: a catalog for `tools/list`
//! and one dispatcher for `tools/call`.
//!
//! Every tool answers with readable text (what the model reads) and the same
//! data as JSON (`structuredContent`). Every error says what to do next.

mod ask;
mod chats;
mod history;
mod messages;
mod people;
mod status;
mod target;
mod td_call;
mod td_offset;
pub(crate) mod td_policy;
mod td_redact;
pub(crate) mod td_request;
mod td_schema;
mod ui;
mod ui_shot;
mod write;

use crate::agent::td::{self, Client, TdError};
use crate::ids::AccountKey;
use serde_json::{Value, json};

/// What one tool call produced.
pub struct ToolOutput {
    pub text: String,
    pub data: Value,
}

/// A failed call, worded as the next step for the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError(pub String);

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<TdError> for ToolError {
    fn from(error: TdError) -> Self {
        match error {
            TdError::Td { code: 401, .. } => ToolError::new(
                "Quill is not signed in to Telegram yet. Ask the user to sign in to Quill, then retry.",
            ),
            TdError::Td { code: 429, message } => ToolError::new(format!(
                "Telegram is rate limiting this account ({message}). Wait the number of seconds it names, then retry."
            )),
            TdError::Td { code, message } => {
                ToolError::new(format!("Telegram returned error {code}: {message}."))
            }
            TdError::Timeout => ToolError::new(
                "Quill got no answer from Telegram within 30 seconds. Call quill_status to check the connection, then retry.",
            ),
            TdError::Send => ToolError::new(
                "Quill could not pass the request to Telegram. Call quill_status, then retry.",
            ),
        }
    }
}

/// The account a call acts on and its live client.
pub(crate) struct Ctx {
    pub account: AccountKey,
    pub client: Client,
}

impl Ctx {
    fn active() -> Result<Self, ToolError> {
        let root = crate::settings::safe_app_root().ok_or_else(|| {
            ToolError::new("Quill has no data folder. Restart Quill, then retry.")
        })?;
        let account = crate::settings::active_account(&root);
        let client = td::client_for(&account).ok_or_else(|| {
            ToolError::new(
                "Quill has no connected account yet. Ask the user to open Quill and sign in, then retry.",
            )
        })?;
        Ok(Self { account, client })
    }

    pub fn call(&self, request: Value) -> Result<Value, ToolError> {
        td::call(&self.client, request).map_err(ToolError::from)
    }
}

/// Every tool: the read tools, the write tools, `td_call`, then the UI driver.
fn all_tools() -> impl Iterator<Item = &'static Tool> {
    grouped().map(|(_, tool)| tool)
}

/// Every tool with the group the Agent access page shows it in.
fn grouped() -> impl Iterator<Item = (ToolGroup, &'static Tool)> {
    let group = |group: ToolGroup, tools: &'static [Tool]| tools.iter().map(move |t| (group, t));
    group(ToolGroup::Read, READ_TOOLS)
        .chain(group(ToolGroup::Write, write::TOOLS))
        .chain(group(ToolGroup::Raw, td_call::TOOLS))
        .chain(group(ToolGroup::Window, ui::TOOLS))
        .chain(group(ToolGroup::Window, ui_shot::TOOLS))
}

/// How the Agent access page groups the tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolGroup {
    /// Reading chats, people and files, and asking for access.
    Read,
    /// Sending and changing messages and chats.
    Write,
    /// `td_call`.
    Raw,
    /// Driving Quill's window.
    Window,
}

impl ToolGroup {
    pub const ALL: [ToolGroup; 4] = [
        ToolGroup::Read,
        ToolGroup::Write,
        ToolGroup::Raw,
        ToolGroup::Window,
    ];

    /// The word agents read (`quill_status`).
    pub fn as_str(self) -> &'static str {
        match self {
            ToolGroup::Read => "read",
            ToolGroup::Write => "write",
            ToolGroup::Raw => "raw_tdlib",
            ToolGroup::Window => "window_control",
        }
    }
}

/// One tool as the access summary sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: &'static str,
    pub title: &'static str,
    pub group: ToolGroup,
    /// "Confirm each send" can put this tool's call behind a click.
    pub confirms_sends: bool,
}

/// Every tool agents can call, in catalog order, with its group.
pub fn registry() -> Vec<ToolInfo> {
    grouped()
        .map(|(group, tool)| ToolInfo {
            name: tool.name,
            title: tool.title,
            group,
            confirms_sends: tool.confirms_sends,
        })
        .collect()
}

struct Tool {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    run: fn(&Value) -> Result<ToolOutput, ToolError>,
    read_only: bool,
    /// Its run reaches `write::common::confirm` (directly, or for a window
    /// tool through a send the message box handed over), so "Confirm each
    /// send" asks the user first.
    confirms_sends: bool,
}

const READ_TOOLS: &[Tool] = &[
    Tool {
        name: "quill_status",
        title: "Quill status",
        description: "Start here. Shows whether Quill is signed in, which account agents act on, \
which chats this agent may read or write (grants), and which tools work right now. \
Example input: {}. Output: account (name, username, user_id), grants (defaults, all_chats, contacts, chats), tools, ui_driver.",
        schema: no_arguments,
        run: status::run,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "list_chats",
        title: "List chats",
        description: "Use to find chats and their ids: the granted chats of the chat list in the user's order, with unread counts \
and the last message. Only granted chats appear. \
Example input: {\"unread_only\": true, \"limit\": 20}, or {\"folder\": \"Work\"}. \
Output: list, chats [{id, chat_ref, title, kind, scope, unread, mentions, last_message}].",
        schema: chats::list_chats_schema,
        run: chats::list_chats,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "list_folders",
        title: "List chat folders",
        description: "Use to see the user's chat folders, to pass one to list_chats. \
Example input: {}. Output: folders [{id, name}].",
        schema: no_arguments,
        run: chats::list_folders,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "list_unread",
        title: "Unread digest",
        description: "Use to catch up: every granted chat with unread messages and its newest unread messages, one line each. \
Reading them here does not mark them read. \
Example input: {\"chats\": 10, \"per_chat\": 3}. Output: chats [{chat {id, title, chat_ref, scope}, unread, messages [{id, date, from, kind, text}]}].",
        schema: chats::unread_schema,
        run: chats::list_unread,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "list_messages",
        title: "List recent messages",
        description: "Use for a quick look at one chat: its newest messages as one-line summaries (long texts cut to 200 \
characters), oldest first. For full texts, reply and forward details, or older and newer pages, use read_history. \
Example input: {\"chat\": \"Ann Lee\", \"limit\": 20}. Output: chat, messages [{id, date, from, kind, text}], older_cursor \
(pass it to read_history as before_message_id).",
        schema: history::list_schema,
        run: history::list_messages,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "read_history",
        title: "Read chat history",
        description: "Use to read a chat in full detail (whole texts, reply targets, forward origins, files), paging older \
or newer from a message id; with no id it starts at the newest message. For a quick summary use list_messages. \
Example input: {\"chat\": \"saved\", \"before_message_id\": 4194304, \"limit\": 50}, or {\"chat\": -1001234567890, \
\"after_message_id\": 4194304}. Output: chat, messages [{id, date, from, kind, text, reply_to, forwarded_from, file}] \
oldest first, older_cursor and newer_cursor for the next page (null when there is none).",
        schema: history::history_schema,
        run: history::read_history,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "search_messages",
        title: "Search messages",
        description: "Use to find messages containing words, in one chat or (leave chat out) across every granted chat. \
Matches whole words and prefixes, not substrings. \
Example input: {\"query\": \"invoice\"}, or {\"chat\": \"saved\", \"query\": \"invoice\", \"limit\": 20}. Output: messages \
[{id, date, from, kind, text, chat}] newest first, next_cursor (in one chat pass it back as before_message_id, across chats as cursor).",
        schema: history::search_schema,
        run: history::search_messages,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "get_message",
        title: "Get one message",
        description: "Use to read one message in full, with its reply target, forward origin, file details and links. \
Example input: {\"chat\": \"saved\", \"message_id\": 4194304}. Output: one message {id, date, edited, from, kind, text, \
reply_to, forwarded_from, file, links}.",
        schema: history::get_schema,
        run: history::get_message,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "get_chat_info",
        title: "Chat details",
        description: "Use to learn about one chat: its kind, username, description or bio, member count and invite link. \
Example input: {\"chat\": \"@durov\"}. Output: {id, chat_ref, title, kind, scope, unread, muted, username, about, member_count, \
invite_link, user_id, is_contact} (fields that do not apply are left out).",
        schema: chats::chat_schema_only,
        run: chats::get_chat_info,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "list_members",
        title: "List members",
        description: "Use to see who is in a group or channel, or to find a member by name. \
Example input: {\"chat\": \"Project team\", \"query\": \"ann\"}. Output: chat, members [{user_id, name, username, role, bot}].",
        schema: chats::members_schema,
        run: chats::list_members,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "list_contacts",
        title: "List contacts",
        description: "Use to find a person among the user's contacts (needs the contacts grant; no phone numbers). \
Example input: {\"query\": \"ann\"}. Output: contacts [{user_id, name, username}].",
        schema: people::contacts_schema,
        run: people::list_contacts,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "download_file",
        title: "Download a file",
        description: "Use to get the file a message carries (photo, document, voice note...) onto disk. \
Example input: {\"chat\": \"saved\", \"message_id\": 4194304, \"to\": \"/Users/me/Downloads\"}. Output: {path, name, size}.",
        schema: people::download_schema,
        run: people::download_file,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "wait_for_message",
        title: "Wait for a message",
        description: "Use to wait until a new message arrives in a granted chat (or one chat), e.g. a bot's reply. Returns as soon as one does. \
Example input: {\"chat\": \"@some_bot\", \"timeout_seconds\": 60}. Output: messages [{id, date, from, kind, text, chat}] \
(empty when the time ran out).",
        schema: people::wait_schema,
        run: people::wait_for_message,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "request_access",
        title: "Ask the user for access",
        description: "Use when a tool says a chat or the contact list is not granted. Shows a prompt in Quill and waits up to \
two minutes for the user's answer. Ask for one thing per call and say why. \
Example input: {\"chat\": \"Ann Lee\", \"scope\": \"read\", \"reason\": \"To summarize what Ann sent this week.\"}, \
or {\"contacts\": true, \"reason\": \"To find Ann's chat.\"}. Output: {granted, scope}.",
        schema: ask::schema,
        run: ask::run,
        read_only: false,
        confirms_sends: false,
    },
];

thread_local! {
    /// The agent whose request this thread runs, for prompts.
    static AGENT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// What held when the current call began; `None` off a call, which
    /// [`still_allowed`] then refuses.
    static CALL: std::cell::Cell<Option<CallStart>> = const { std::cell::Cell::new(None) };
}

/// The switches a call's writes are checked against at their queue turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CallStart {
    /// The connection the call came on (0 when none).
    conn: u64,
    /// [`crate::agent::access::generation`]: the Agent access switch.
    access: u64,
    /// [`crate::agent::access::lock_count`].
    locks: u64,
    /// [`crate::agent::grants::generation`]: which chats agents may use.
    grants: u64,
}

impl CallStart {
    fn now(conn: u64) -> Self {
        Self {
            conn,
            access: crate::agent::access::generation(),
            locks: crate::agent::access::lock_count(),
            grants: crate::agent::grants::generation(),
        }
    }
}

fn call_start() -> Option<CallStart> {
    CALL.try_with(std::cell::Cell::get).ok().flatten()
}

/// Marks the thread as running a call for as long as it lives, and off a
/// call again when the call ends (or unwinds).
struct CallScope;

impl CallScope {
    fn begin(start: CallStart) -> Self {
        let _ = CALL.try_with(|current| current.set(Some(start)));
        CallScope
    }
}

impl Drop for CallScope {
    fn drop(&mut self) {
        let _ = CALL.try_with(|current| current.set(None));
    }
}

/// What a write answers when Quill locked while it waited its turn: the
/// check runs before anything goes out, so nothing did.
pub(crate) const LOCKED_BEFORE_SENDING_MESSAGE: &str = "Quill was locked while this waited its turn, so nothing was sent or changed. Ask the user to unlock Quill, then retry.";

/// What a write answers when the user changed which chats agents may use
/// while it waited (at the confirm prompt or in the paced queue): the grant
/// it was let through on may no longer hold, so nothing went out.
pub(crate) const GRANTS_CHANGED_MESSAGE: &str = "The user changed which chats agents may use while this waited, so nothing was sent or changed. Call quill_status to see the grants now, then retry if the chat is still granted for writing.";

/// Refuses once Agent access went off (or the strip's Revoke was clicked,
/// or the token was reset) after the current call began, Quill locked
/// since then, the user changed the grants since then, or the user revoked
/// this agent's connection: a write that waited in the queue or at the
/// confirm prompt must not go out.
pub(crate) fn still_allowed() -> Result<(), ToolError> {
    let Some(began) = call_start() else {
        return Err(ToolError::new(LOCKED_BEFORE_SENDING_MESSAGE));
    };
    if began.conn != 0 && crate::agent::activity::is_revoked(began.conn) {
        return Err(ToolError::new(crate::agent::server::REVOKED_MESSAGE));
    }
    if locked_since_call_began() {
        return Err(ToolError::new(LOCKED_BEFORE_SENDING_MESSAGE));
    }
    if !crate::agent::access::enabled() || crate::agent::access::generation() != began.access {
        return Err(ToolError::new(
            "Agent access was turned off in Quill, so nothing was sent or changed. Ask the user to turn it back on.",
        ));
    }
    if crate::agent::grants::generation() != began.grants {
        return Err(ToolError::new(GRANTS_CHANGED_MESSAGE));
    }
    Ok(())
}

/// Quill is locked, or locked at some point since the current call began:
/// a step with an effect outside the answer (a send, a file copy) must not
/// run, since [`withheld_if_locked`] hides only the answer.
pub(crate) fn locked_since_call_began() -> bool {
    call_start().is_none_or(|began| crate::agent::access::locked_since(began.locks))
}

/// The name of the agent making the current call.
pub(crate) fn current_agent() -> String {
    AGENT
        .try_with(|agent| agent.borrow().clone())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "An agent".to_string())
}

fn no_arguments() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

/// The `tools` array of a `tools/list` answer.
pub fn catalog() -> Vec<Value> {
    all_tools()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": (tool.schema)(),
                "annotations": {
                    "readOnlyHint": tool.read_only,
                    "openWorldHint": true,
                },
            })
        })
        .collect()
}

/// The `quill://unread` resource: what `list_unread` returns, as JSON.
pub fn unread_resource() -> Result<Value, ToolError> {
    let output = chats::list_unread(&json!({}))?;
    Ok(json!({"contents": [{
        "uri": crate::agent::guide::UNREAD_URI,
        "mimeType": "application/json",
        "text": output.data.to_string(),
    }]}))
}

/// Wait up to `wait` for a message from someone else in a granted chat of
/// the active account after `since`; moves `since` past what it saw.
pub fn granted_message_arrived(
    since: &mut u64,
    wait: std::time::Duration,
) -> Result<bool, ToolError> {
    // Locked, the wait below returns at once with nothing: an error makes
    // the watcher sleep between looks instead of spinning a core.
    if crate::agent::access::paused_by_lock() {
        return Err(ToolError::new(crate::agent::access::LOCKED_MESSAGE));
    }
    let ctx = Ctx::active()?;
    let me = target::my_user_id(&ctx)?;
    let mut known = std::collections::HashMap::new();
    let (arrived, checked) = crate::agent::events::wait_for(ctx.client.id(), *since, wait, |new| {
        !new.outgoing
            && *known.entry(new.chat_id).or_insert_with(|| {
                target::granted(&ctx, new.chat_id, me)
                    .ok()
                    .flatten()
                    .is_some()
            })
    });
    *since = checked.max(*since);
    Ok(!arrived.is_empty())
}

/// The names of every tool, for `quill_status`.
pub(crate) fn names() -> Vec<&'static str> {
    all_tools().map(|tool| tool.name).collect()
}

/// Run tool `name` for `agent` and shape the MCP `tools/call` result.
pub fn call(name: &str, arguments: &Value, agent: &str) -> Value {
    call_on(0, name, arguments, agent)
}

/// [`call`] for a request on connection `conn`, which then lists the tool
/// among those it used ([`crate::agent::activity::note_tool`]).
pub fn call_on(conn: u64, name: &str, arguments: &Value, agent: &str) -> Value {
    let _ = AGENT.try_with(|current| current.replace(agent.to_string()));
    let _ = ui_shot::take_attached();
    let start = CallStart::now(conn);
    let locks = start.locks;
    let _call = CallScope::begin(start);
    let result = match all_tools().find(|tool| tool.name == name) {
        // Locked: no read, send or window step, `quill_status` included
        // (it reads the account).
        Some(_) if crate::agent::access::paused_by_lock() => {
            Err(ToolError::new(crate::agent::access::LOCKED_MESSAGE))
        }
        Some(tool) => {
            crate::agent::activity::note_tool(conn, tool.name);
            withheld_if_locked(tool.read_only, locks, (tool.run)(arguments))
        }
        None => Err(ToolError::new(format!(
            "There is no tool named {name}. Call tools/list, or quill_status, to see the tools."
        ))),
    };
    // Screenshots ride along as image blocks after the text, never once
    // Quill has locked.
    let mut images = ui_shot::take_attached();
    if crate::agent::access::locked_since(locks) {
        images.clear();
    }
    match result {
        Ok(output) => {
            let mut content = vec![json!({"type": "text", "text": output.text})];
            content.extend(images);
            json!({
                "content": content,
                "structuredContent": output.data,
                "isError": false,
            })
        }
        Err(ToolError(message)) => json!({
            "content": [{"type": "text", "text": message}],
            "isError": true,
        }),
    }
}

/// A tool's result, unless Quill locked at any point since `locks` was
/// read: the lock holds for the whole call, so what a read found (a wait
/// that ended at the lock included) is withheld, and a write says it may
/// have been done.
pub(crate) fn withheld_if_locked(
    read_only: bool,
    locks: u64,
    result: Result<ToolOutput, ToolError>,
) -> Result<ToolOutput, ToolError> {
    if !crate::agent::access::locked_since(locks) {
        return result;
    }
    // Refused at its queue turn ([`still_allowed`]): known not done.
    if let Err(ToolError(message)) = &result
        && message == LOCKED_BEFORE_SENDING_MESSAGE
    {
        return result;
    }
    Err(ToolError::new(if read_only {
        crate::agent::access::LOCKED_MESSAGE
    } else {
        crate::agent::access::LOCKED_WHILE_RUNNING_MESSAGE
    }))
}

/// An optional bounded integer argument.
pub(crate) fn int_arg(
    args: &Value,
    key: &str,
    default: i64,
    min: i64,
    max: i64,
) -> Result<i64, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_i64()
            .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
            .map(|n| n.clamp(min, max))
            .ok_or_else(|| {
                ToolError::new(format!(
                    "{key} must be a whole number between {min} and {max}."
                ))
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::{all_tools, call, catalog};
    use crate::agent::access;

    /// A write that waited in the paced queue while Quill locked is refused
    /// at its turn, with a message that says nothing went out.
    #[test]
    fn a_lock_while_a_write_waits_stops_it() {
        let _guard = crate::locks::lock_or_recover(&access::TEST_LOCK);
        // The switch stays in memory: `quill agent` tests read it from disk.
        crate::fast_buttons::set_persistence(false);
        access::set_enabled(true);
        access::set_paused_by_lock(false);
        let begin = || super::CallScope::begin(super::CallStart::now(0));
        let call = begin();
        assert!(super::still_allowed().is_ok(), "nothing changed");
        access::set_paused_by_lock(true);
        access::set_paused_by_lock(false);
        let refused = super::still_allowed();
        assert!(
            matches!(&refused, Err(super::ToolError(message)) if message == super::LOCKED_BEFORE_SENDING_MESSAGE),
            "locked and unlocked while it waited"
        );
        let started = access::lock_count().saturating_sub(1);
        let answer = super::withheld_if_locked(
            false,
            started,
            refused.map(|()| super::ToolOutput {
                text: "sent".into(),
                data: serde_json::json!({}),
            }),
        );
        assert!(
            matches!(&answer, Err(super::ToolError(message)) if message == super::LOCKED_BEFORE_SENDING_MESSAGE),
            "the call says nothing went out, not that it may have"
        );
        drop(call);
        let call = begin();
        assert!(super::still_allowed().is_ok(), "a call begun after it");
        drop(call);
        assert!(super::still_allowed().is_err(), "off a call");
        assert!(super::locked_since_call_began(), "off a call");
        access::set_enabled(false);
    }

    /// A write that waited while the user changed the grants is refused at
    /// its turn: the grant it was let through on may be gone.
    #[test]
    fn a_grant_change_while_a_write_waits_stops_it() {
        let _guard = crate::locks::lock_or_recover(&access::TEST_LOCK);
        crate::fast_buttons::set_persistence(false);
        access::set_enabled(true);
        access::set_paused_by_lock(false);
        let call = super::CallScope::begin(super::CallStart::now(0));
        assert!(super::still_allowed().is_ok());
        crate::agent::grants::bump_generation_for_test();
        assert!(matches!(
            super::still_allowed(),
            Err(super::ToolError(message)) if message == super::GRANTS_CHANGED_MESSAGE
        ));
        drop(call);
        let call = super::CallScope::begin(super::CallStart::now(0));
        assert!(super::still_allowed().is_ok(), "a call begun after it");
        drop(call);
        access::set_enabled(false);
    }

    /// An Allow on a one-off confirmation runs a grants update that changes
    /// nothing: the write waiting on it still goes. A real change stops it.
    #[test]
    fn an_update_that_changes_nothing_lets_a_waiting_write_go() {
        let _guard = crate::locks::lock_or_recover(&access::TEST_LOCK);
        crate::fast_buttons::set_persistence(false);
        access::set_enabled(true);
        access::set_paused_by_lock(false);
        let account = crate::ids::AccountKey("grants-noop-test".into());
        let call = super::CallScope::begin(super::CallStart::now(0));
        crate::agent::grants::update(&account, |_| {});
        crate::agent::grants::update(&account, |g| g.contacts = false);
        assert!(super::still_allowed().is_ok(), "nothing changed");
        crate::agent::grants::update(&account, |g| g.contacts = true);
        assert!(matches!(
            super::still_allowed(),
            Err(super::ToolError(message)) if message == super::GRANTS_CHANGED_MESSAGE
        ));
        drop(call);
        crate::agent::grants::forget_account(&account);
        access::set_enabled(false);
    }

    /// Locked, the `quill://unread` watcher's look fails at once, so the
    /// watcher sleeps between looks instead of spinning.
    #[test]
    fn a_locked_watch_look_fails_without_waiting() {
        let _guard = crate::locks::lock_or_recover(&access::TEST_LOCK);
        access::set_paused_by_lock(true);
        let mut since = 0;
        let look = super::granted_message_arrived(&mut since, std::time::Duration::MAX);
        assert!(
            matches!(&look, Err(super::ToolError(message)) if message == access::LOCKED_MESSAGE),
            "{:?}",
            look.as_ref().err().map(|e| &e.0)
        );
        access::set_paused_by_lock(false);
    }

    #[test]
    fn every_tool_is_described_with_an_example_and_output() {
        for tool in all_tools() {
            assert!(tool.description.contains("Example input"), "{}", tool.name);
            assert!(tool.description.contains("Output"), "{}", tool.name);
            assert!(
                tool.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
            );
        }
        assert_eq!(catalog().len(), all_tools().count());
    }

    #[test]
    fn unknown_tools_say_what_to_do() {
        let result = call("send_everything", &serde_json::json!({}), "Test");
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("tools/list")
        );
    }
}
