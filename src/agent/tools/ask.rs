//! `request_access`: ask the user, in Quill, for a chat, all chats or the
//! contact list. The call waits for the answer.

use super::target::{Reach, chat_schema, lookup};
use super::{Ctx, ToolError, ToolOutput};
use crate::agent::grants::{self, ChatClass, Scope};
use crate::agent::prompts::{self, Ask, Decision};
use serde_json::{Value, json};
use std::time::Duration;

/// How long the user has to answer before the prompt is withdrawn.
const ANSWER_WAIT: Duration = Duration::from_secs(120);

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "all_chats": {"type": "boolean", "description": "Ask for every chat except secret chats."},
            "contacts": {"type": "boolean", "description": "Ask to read the contact list."},
            "scope": {"type": "string", "enum": ["read", "read_write"], "default": "read"},
            "reason": {"type": "string", "description": "One sentence the user sees: why you need it."},
        },
        "required": ["reason"],
        "additionalProperties": false,
    })
}

pub fn run(args: &Value) -> Result<ToolOutput, ToolError> {
    let reason = args
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| {
            ToolError::new("reason is required: one sentence the user reads in Quill, e.g. \"To summarize this week's messages from Ann.\"")
        })?
        .chars()
        .take(300)
        .collect::<String>();
    let scope = match args.get("scope").and_then(Value::as_str) {
        None => Scope::Read,
        Some(word) => Scope::parse(word)
            .ok_or_else(|| ToolError::new("scope is \"read\" or \"read_write\"."))?,
    };
    let wants_all = args.get("all_chats").and_then(Value::as_bool) == Some(true);
    let wants_contacts = args.get("contacts").and_then(Value::as_bool) == Some(true);
    let wants_chat = args.get("chat").is_some_and(|c| !c.is_null());
    if [wants_all, wants_contacts, wants_chat]
        .iter()
        .filter(|w| **w)
        .count()
        != 1
    {
        return Err(ToolError::new(
            "Ask for one thing per call: \"chat\", \"all_chats\": true, or \"contacts\": true.",
        ));
    }
    let ctx = Ctx::active()?;
    let current = grants::for_account(&ctx.account);
    let request = if wants_contacts {
        if current.contacts {
            return Ok(already("the contact list"));
        }
        Request::Contacts
    } else if wants_all {
        if current.all_chats.is_some_and(|s| s.allows(scope)) {
            return Ok(already("all chats"));
        }
        Request::AllChats
    } else {
        let found = lookup(&ctx, args, Reach::Any)?;
        if current
            .scope_for(found.chat_id, found.class)
            .is_some_and(|s| s.allows(scope))
        {
            return Ok(already(&found.title));
        }
        // Until the user allows it, the agent hears only its own words back.
        let what = match args.get("chat") {
            Some(Value::String(name)) => format!("\"{}\"", name.trim()),
            _ => format!("chat {}", found.chat_id),
        };
        Request::Chat {
            chat_id: found.chat_id,
            title: found.title,
            secret: found.class == ChatClass::Secret,
            what,
        }
    };
    let what = request.what();
    let ask = request.ask(scope);
    let agent = super::current_agent();
    let (id, rx) = prompts::file(ctx.account.clone(), agent, ask, reason).ok_or_else(|| {
        ToolError::new("Several requests are already waiting for the user in Quill. Wait for those answers, then retry.")
    })?;
    match prompts::wait(id, &rx, ANSWER_WAIT) {
        Some(Decision::Allowed) => Ok(match request {
            Request::Chat { chat_id, title, .. } => ToolOutput {
                text: format!(
                    "The user allowed \"{title}\" (chat {chat_id}, {}). Go ahead.",
                    scope.as_str()
                ),
                data: json!({"granted": true, "scope": scope.as_str(), "chat": {"id": chat_id, "title": title}}),
                images: Vec::new(),
            },
            Request::Contacts | Request::AllChats => ToolOutput {
                text: format!("The user allowed {what} ({}). Go ahead.", scope.as_str()),
                data: json!({"granted": true, "scope": scope.as_str()}),
                images: Vec::new(),
            },
        }),
        Some(Decision::Denied) => Ok(ToolOutput {
            text: format!(
                "The user declined access to {what}. Do not ask again for the same thing in this task."
            ),
            data: json!({"granted": false}),
            images: Vec::new(),
        }),
        None => Err(ToolError::new(format!(
            "The user did not answer within {} seconds, so the request was withdrawn. Tell the user Quill is asking for {what}, then call request_access again.",
            ANSWER_WAIT.as_secs()
        ))),
    }
}

/// What the agent asked for: one value that gives both the prompt the user
/// sees and the words the agent hears back.
enum Request {
    Contacts,
    AllChats,
    Chat {
        chat_id: i64,
        title: String,
        secret: bool,
        /// How the agent named the chat, until the user allows it.
        what: String,
    },
}

impl Request {
    fn what(&self) -> String {
        match self {
            Self::Contacts => "the contact list".to_string(),
            Self::AllChats => "all chats".to_string(),
            Self::Chat { what, .. } => what.clone(),
        }
    }

    fn ask(&self, scope: Scope) -> Ask {
        match self {
            Self::Contacts => Ask::Contacts,
            Self::AllChats => Ask::AllChats { scope },
            Self::Chat {
                chat_id,
                title,
                secret,
                ..
            } => Ask::Chat {
                chat_id: *chat_id,
                title: title.clone(),
                secret: *secret,
                scope,
            },
        }
    }
}

fn already(what: &str) -> ToolOutput {
    ToolOutput {
        text: format!("Agents already have {what} at that scope. No need to ask."),
        data: json!({"granted": true, "already": true}),
        images: Vec::new(),
    }
}
