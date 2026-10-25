//! The action-level guard of the UI driver: while the UI thread injects an
//! agent's input ([`injecting`]), the handlers that would write refuse and
//! say so here, and the reply to the agent carries every refusal
//! ([`take_refusals`]). This is the guarantee behind the driver: the tree
//! and the surface checks decide what an agent sees and may point at, and
//! this guard decides what its input can do, however it got there.
//!
//! Two kinds of guard read the flag:
//!
//! - [`refuse_if_injecting`], at the entry of a handler that writes without
//!   TDLib (the agent prompts' Allow, the grants, the switches, opening an
//!   ungranted chat).
//! - [`screen_request`], in the live TDLib sender: every request the window
//!   would send while injecting is checked against a short allowlist of
//!   reads and harmless changes ([`request_allowed`]), and then, whatever
//!   its method, for money in its parameters ([`carries_money`]); anything
//!   else is dropped with a refusal, so a click that reaches a send, a
//!   reaction, a poll vote, a bot button, a payment or a paid search sends
//!   nothing.
//!
//! The flag is per thread: input is injected on the UI thread, and every
//! handler and the sender run on it in the same call.

use crate::agent::tools::{td_policy, td_request};
use serde_json::Value;
use std::cell::{Cell, RefCell};

thread_local! {
    static INJECTING: Cell<bool> = const { Cell::new(false) };
    static REFUSALS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The agent and the log name (`ui_click`) of the latest input command,
    /// for refusals in a step's deferred half ([`with_injecting`]).
    static LAST_STEP: RefCell<Option<(String, &'static str)>> = const { RefCell::new(None) };
    /// Input commands that have not answered yet ([`OpenCommand`]).
    static OPEN: Cell<u32> = const { Cell::new(0) };
}

/// An input command between its first step and its answer. While one is
/// open, refusals from a deferred half stay in the list its reply takes;
/// once the last one closes, what is left goes to the activity log.
pub struct OpenCommand(());

impl OpenCommand {
    /// An input command from `agent` starts; `tool` is its log name.
    pub fn begin(agent: &str, tool: &'static str) -> Self {
        LAST_STEP.with(|last| *last.borrow_mut() = Some((agent.to_string(), tool)));
        OPEN.with(|open| open.set(open.get().saturating_add(1)));
        Self(())
    }
}

impl Drop for OpenCommand {
    fn drop(&mut self) {
        let left = OPEN.with(|open| {
            let left = open.get().saturating_sub(1);
            open.set(left);
            left
        });
        if left == 0 {
            log_deferred(take_refusals());
        }
    }
}

/// Whether the input being handled right now came from the driver.
pub fn injecting() -> bool {
    INJECTING.with(Cell::get)
}

/// Mark the start and end of one injected input on this thread.
pub fn set_injecting(on: bool) {
    INJECTING.with(|flag| flag.set(on));
}

/// Runs `f` under the injecting flag when `on`: the deferred half of an
/// agent's step (a clicked link run at render, a deep link's chat open
/// once TDLib resolved it) meets the same refusals as the step itself.
/// The flag goes back to what it was after.
///
/// While the command is still open ([`OpenCommand`]), its reply carries
/// the refusals. After it answered (TDLib resolved a link later, a render
/// after the reply), nobody waits for them and the next command's reply
/// would carry them to the wrong step: they go to the activity log
/// instead, as declined entries under the latest step's agent and tool.
pub fn with_injecting<R>(on: bool, f: impl FnOnce() -> R) -> R {
    if !on {
        return f();
    }
    let was = injecting();
    if was || OPEN.with(Cell::get) > 0 {
        set_injecting(true);
        let out = f();
        set_injecting(was);
        return out;
    }
    let outer = take_refusals();
    set_injecting(true);
    let out = f();
    set_injecting(false);
    let deferred = take_refusals();
    REFUSALS.with(|refusals| *refusals.borrow_mut() = outer);
    log_deferred(deferred);
    out
}

fn log_deferred(refusals: Vec<String>) {
    if refusals.is_empty() {
        return;
    }
    let (agent, tool) = LAST_STEP
        .with(|last| last.borrow().clone())
        .unwrap_or_else(|| {
            (
                super::takeover::last_agent().unwrap_or_default(),
                "ui_click",
            )
        });
    let account = crate::settings::safe_app_root()
        .map(|root| crate::settings::active_account(&root))
        .unwrap_or_else(crate::ids::AccountKey::primary);
    for summary in refusals {
        crate::agent::activity::record(crate::agent::activity::Action {
            time: 0,
            account: account.clone(),
            agent: agent.clone(),
            tool: tool.to_string(),
            chat_id: 0,
            chat_title: String::new(),
            summary,
            ok: false,
            declined: true,
        });
    }
}

/// Record why something was refused during the current injection.
pub fn refuse(reason: impl Into<String>) {
    let reason = reason.into();
    REFUSALS.with(|refusals| {
        let mut refusals = refusals.borrow_mut();
        if refusals.len() < 32 && !refusals.contains(&reason) {
            refusals.push(reason);
        }
    });
}

/// The refusals recorded since the last call, oldest first.
pub fn take_refusals() -> Vec<String> {
    REFUSALS.with(|refusals| std::mem::take(&mut *refusals.borrow_mut()))
}

/// At a handler's entry: `true` (and a refusal recorded) when the driver is
/// injecting, so the handler returns without doing `what`.
pub fn refuse_if_injecting(what: &str) -> bool {
    if !injecting() {
        return false;
    }
    refuse(format!(
        "Quill does not let agents {what} through the window. Use the agent tools, or ask the user."
    ));
    true
}

/// The surfaces agents never see or use, in any scope. The UI's surface
/// check blocks them once they are on screen; this is the other half: the
/// handler that would open one refuses while injecting, so a chord, a menu
/// item or a click never leaves one open for the user to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardSurface {
    /// The settings window, every page and box of it (Agent access, log
    /// out, sessions, privacy and the shortcuts reference included).
    Settings,
    /// Locking Quill behind the local passcode.
    Lock,
    /// The local passcode settings.
    Passcode,
    /// Two-step verification and the logged-in websites.
    Security,
    /// Log out and account deletion, and the accounts switcher.
    Account,
    /// A checkout, Stars, Premium and gifts.
    Payments,
    /// Quitting Quill, closing, hiding or minimising its window.
    Window,
}

impl HardSurface {
    fn what(self) -> &'static str {
        match self {
            HardSurface::Settings => "open Settings",
            HardSurface::Lock => "lock Quill",
            HardSurface::Passcode => "open the passcode settings",
            HardSurface::Security => "open the security settings",
            HardSurface::Account => "log out or switch accounts",
            HardSurface::Payments => "open payments, Stars, Premium or gifts",
            HardSurface::Window => "quit Quill, or close, hide or minimise its window",
        }
    }
}

/// At the entry of a handler that opens a [`HardSurface`]: `true` (and a
/// refusal recorded) when the driver is injecting.
pub fn refuse_hard_if_injecting(surface: HardSurface) -> bool {
    refuse_if_injecting(surface.what())
}

/// An agent's step opened a dialog or overlay agents never use (a payment,
/// Stars, a gift, Premium, an account, security or export box, a delete
/// confirmation): the window closes it before it shows, and this says so.
/// The other half of [`refuse_hard_if_injecting`], for the openers that
/// have no guard of their own.
pub fn refuse_hard_dialog() {
    refuse(
        "Quill closed a dialog an agent's input opened: payments, Stars, gifts, Premium, accounts, security, exports and delete confirmations stay with the user. Use the agent tools, or ask the user.",
    );
}

/// A clipboard read (a paste by chord, menu or mini app) while injecting:
/// the clipboard is the user's, so it reads as empty.
pub fn refuse_clipboard_if_injecting() -> bool {
    refuse_if_injecting("paste the user's clipboard")
}

/// A typed deep link (settings, a proxy, a folder, a sticker set, a
/// background) resolved from an agent's step: none of them is an agent's.
pub fn refuse_typed_link() {
    refuse(
        "Quill does not let agents open that kind of link through the window (settings, proxies, folders, sticker sets, backgrounds). Ask the user.",
    );
}

/// At the edge of the process: `true` (and a refusal recorded) when the
/// driver is injecting, so nothing leaves Quill for the OS (a link in the
/// browser, a file in its app, a folder in the file manager, a mail
/// client). `what` is the verb phrase, like "open a link in the browser".
pub fn refuse_outside_if_injecting(what: &str) -> bool {
    if !injecting() {
        return false;
    }
    refuse(format!(
        "Quill did not {what} while an agent was driving the window: anything outside Quill (the browser, files, folders, other apps) stays with the user. Give the user the link or the file name instead."
    ));
    true
}

/// Reads that act for the user or start a flow with a bot or a site; never
/// while injecting even though their names start like a read.
const DENIED_READS: &[&str] = &[
    "getCallbackQueryAnswer",
    "getLoginUrl",
    "getLoginUrlInfo",
    "getInlineQueryResults",
    "getPaymentForm",
    "getWebAppUrl",
    "getWebAppLinkUrl",
    "getMainWebApp",
    "getExternalLink",
    "getAttachmentMenuBot",
    "getStarPaymentOptions",
    "getStarGiftPaymentOptions",
    "getPremiumGiftPaymentOptions",
];

/// Changes the window makes while looking, typing a draft or scrolling:
/// opening and reading a chat, drafts and typing, file downloads, loading
/// more of a list. Marking a chat read is what opening it does anyway.
const ALLOWED_CHANGES: &[&str] = &[
    "openChat",
    "closeChat",
    "viewMessages",
    "readAllChatMentions",
    "readAllChatReactions",
    "readAllMessageThreadMentions",
    "readAllMessageThreadReactions",
    "setChatDraftMessage",
    "sendChatAction",
    "downloadFile",
    "cancelDownloadFile",
    "readFilePart",
    "loadChats",
    "loadSavedMessagesTopics",
    "loadQuickReplyShortcuts",
    "loadActiveStories",
    "loadGroupCallParticipants",
    "recognizeSpeech",
    "translateText",
    "translateMessageText",
    "openStory",
    "closeStory",
    "viewSponsoredChat",
];

/// Whether the window may send `method` to TDLib while the driver injects.
pub fn request_allowed(method: &str) -> bool {
    if DENIED_READS.contains(&method) {
        return false;
    }
    td_policy::is_read(method) || ALLOWED_CHANGES.contains(&method)
}

/// Fields that carry a price, whatever the method: a positive number, or
/// an object (a `suggestedPostPrice`, a `starAmount`) in one of them is
/// money on the move.
const MONEY_FIELDS: &[&str] = &[
    "star_count",
    "paid_message_star_count",
    "price",
    "amount",
    "star_amount",
    "total_amount",
];

/// How deep a request is walked for money; TDLib requests are shallow.
const MONEY_DEPTH: usize = 32;

fn money_value(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.as_f64().is_some_and(|n| n > 0.0),
        Value::String(text) => text.trim().parse::<i64>().is_ok_and(|n| n > 0),
        Value::Object(_) | Value::Array(_) => true,
        Value::Null | Value::Bool(_) => false,
    }
}

fn money_below(value: &Value, depth: usize) -> bool {
    if depth == 0 {
        return true;
    }
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            (key == "@type" && value.as_str().is_some_and(td_policy::names_money))
                || (MONEY_FIELDS.contains(&key.as_str()) && money_value(value))
                || money_below(value, depth.saturating_sub(1))
        }),
        Value::Array(items) => items
            .iter()
            .any(|item| money_below(item, depth.saturating_sub(1))),
        _ => false,
    }
}

/// Whether a request would spend or move Stars or money, read from its
/// parameters at any depth and from every type name in it, never from the
/// method's prefix alone: `searchPublicPosts` with a positive `star_count`
/// pays for the search, a `sendMessage` with `paid_message_star_count`
/// pays per message, and any `*Payment*`, `*Star*`, `*Gift*` type at any
/// depth is a payment. [`td_request::spends`] is the reading the `td_call`
/// tool applies; this adds the method's own name and the price-shaped
/// fields ([`MONEY_FIELDS`]).
pub fn carries_money(request: &Value) -> bool {
    request
        .get("@type")
        .and_then(Value::as_str)
        .is_some_and(td_policy::names_money)
        || td_request::spends(request)
        || money_below(request, MONEY_DEPTH)
}

/// Screen a request the window is about to send. `Ok` when it may go
/// (always while nothing is injected); `Err` names the method and records
/// the refusal. The method must be a read or a harmless change, and the
/// request must carry no money ([`carries_money`]), so a paid read is
/// refused for the money.
pub fn screen_request(request: &str) -> Result<(), String> {
    if !injecting() {
        return Ok(());
    }
    let value: Value = serde_json::from_str(request).unwrap_or(Value::Null);
    let method = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let named = if method.is_empty() {
        "a request"
    } else {
        method.as_str()
    };
    let reason = if !request_allowed(&method) {
        format!(
            "Quill did not send {named} while an agent was driving the window: writes go through the agent tools (send_message, react, delete_messages, mark_read...) or the user."
        )
    } else if carries_money(&value) {
        format!(
            "Quill did not send {named} while an agent was driving the window: it would spend Stars or money. Payments, paid searches and paid messages stay with the user."
        )
    } else {
        return Ok(());
    };
    refuse(reason.clone());
    Err(reason)
}

/// The answer for a request [`screen_request`] refused that is a read (or
/// a harmless change) refused only for money: TDLib's own error shape for
/// its `@extra`, so the request's error path runs as for any failed read
/// and clears what the caller set loading. `None` for a write, a read that
/// acts ([`DENIED_READS`]) or a request without an `@extra`: the sender
/// refuses those outright and the caller's send-failure path runs.
pub fn refused_read_answer(request: &str) -> Option<String> {
    let value: Value = serde_json::from_str(request).ok()?;
    let method = value.get("@type").and_then(Value::as_str)?;
    let extra = value
        .get("@extra")
        .filter(|extra| extra.is_string() || extra.is_number())?;
    request_allowed(method).then(|| {
        serde_json::json!({
            "@type": "error",
            "code": 403,
            "message": "QUILL_AGENT_DRIVING",
            "@extra": extra,
        })
        .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        OpenCommand, carries_money, injecting, refuse_if_injecting, refused_read_answer,
        request_allowed, screen_request, set_injecting, take_refusals, with_injecting,
    };
    use crate::ids::RequestId;
    use crate::telegram::requests::search_public_posts_paid;

    #[test]
    fn a_deferred_half_logs_its_refusals_once_its_command_answered() {
        let _ = take_refusals();
        let account = crate::settings::safe_app_root()
            .map(|root| crate::settings::active_account(&root))
            .unwrap_or_else(crate::ids::AccountKey::primary);
        // Still open: the command's reply takes them.
        let open = OpenCommand::begin("Guard test agent", "ui_click");
        assert!(with_injecting(true, || refuse_if_injecting(
            "open the open-test box"
        )));
        assert!(!injecting());
        let refusals = take_refusals();
        assert!(
            refusals.iter().any(|r| r.contains("open-test")),
            "{refusals:?}"
        );
        drop(open);
        // Answered: they go to the activity log, not to the next command.
        assert!(with_injecting(true, || refuse_if_injecting(
            "open the late-test box"
        )));
        assert!(
            take_refusals().is_empty(),
            "nothing left for the next reply"
        );
        let logged = crate::agent::activity::recent(&account)
            .into_iter()
            .find(|action| action.summary.contains("late-test"))
            .unwrap_or_else(|| panic!("logged"));
        assert_eq!(logged.agent, "Guard test agent");
        assert_eq!(logged.tool, "ui_click");
        assert!(logged.declined && !logged.ok);
    }
    use serde_json::json;

    #[test]
    fn reads_and_looking_pass_and_writes_do_not() {
        for method in [
            "getChatHistory",
            "searchMessages",
            "openChat",
            "viewMessages",
            "setChatDraftMessage",
            "sendChatAction",
            "downloadFile",
            "loadChats",
        ] {
            assert!(request_allowed(method), "{method}");
        }
        for method in [
            "sendMessage",
            "sendMessageAlbum",
            "forwardMessages",
            "deleteMessages",
            "deleteChatHistory",
            "addMessageReaction",
            "setPollAnswer",
            "getCallbackQueryAnswer",
            "getInlineQueryResults",
            "openWebApp",
            "sendPaymentForm",
            "toggleChatIsPinned",
            "setChatNotificationSettings",
            "leaveChat",
            "toggleMessageSenderIsBlocked",
            "reportChat",
            "logOut",
            "deleteAccount",
            "terminateSession",
            "createCall",
            "setOption",
            "",
        ] {
            assert!(!request_allowed(method), "{method}");
        }
    }

    #[test]
    fn screening_applies_only_while_injecting_and_records_refusals() {
        let _ = take_refusals();
        assert_eq!(screen_request(r#"{"@type":"sendMessage"}"#), Ok(()));
        set_injecting(true);
        assert!(injecting());
        assert_eq!(screen_request(r#"{"@type":"getChat","chat_id":1}"#), Ok(()));
        let refused = screen_request(r#"{"@type":"sendMessage","chat_id":1}"#);
        assert!(refused.as_ref().is_err_and(|r| r.contains("sendMessage")));
        assert!(screen_request("not json").is_err());
        assert!(refuse_if_injecting("answer its own request"));
        set_injecting(false);
        assert!(!refuse_if_injecting("answer its own request"));
        let refusals = take_refusals();
        assert_eq!(refusals.len(), 3, "{refusals:?}");
        assert!(
            refusals
                .iter()
                .any(|r| r.contains("answer its own request"))
        );
        assert!(take_refusals().is_empty());
    }

    #[test]
    fn money_is_read_from_the_parameters_not_the_prefix() {
        let paid = search_public_posts_paid(RequestId(1), "cats", 20, 50);
        let free = paid.replace("\"star_count\":50", "\"star_count\":0");
        let value = |text: &str| serde_json::from_str(text).unwrap_or_default();
        assert!(carries_money(&value(&paid)), "{paid}");
        assert!(!carries_money(&value(&free)), "{free}");
        assert!(carries_money(&json!({
            "@type": "sendMessage",
            "options": {"paid_message_star_count": 5}
        })));
        assert!(!carries_money(&json!({
            "@type": "sendMessage",
            "options": {"paid_message_star_count": 0}
        })));
        assert!(carries_money(&json!({
            "@type": "setChatDraftMessage",
            "draft_message": {"suggested_post_info": {"price": {"@type": "suggestedPostPriceStar", "star_count": 10}}}
        })));
        assert!(carries_money(&json!({"@type": "getStarTransactions"})));
        assert!(carries_money(&json!({"@type": "getPremiumFeatures"})));
        assert!(carries_money(&json!({
            "@type": "sendPaymentForm",
            "credentials": {"@type": "inputCredentialsSaved", "saved_credentials_id": "x"}
        })));
        assert!(carries_money(&json!({
            "@type": "x",
            "deep": [{"a": {"b": {"star_amount": {"@type": "starAmount", "star_count": 1}}}}]
        })));
        assert!(!carries_money(
            &json!({"@type": "getChatHistory", "chat_id": 1, "limit": 20})
        ));
        assert!(!carries_money(
            &json!({"@type": "viewMessages", "chat_id": 1, "message_ids": [1]})
        ));

        let _ = take_refusals();
        set_injecting(true);
        let refused = screen_request(&paid);
        assert!(
            refused.as_ref().is_err_and(|r| r.contains("Stars")),
            "{refused:?}"
        );
        assert_eq!(screen_request(&free), Ok(()));
        set_injecting(false);
        assert_eq!(screen_request(&paid), Ok(()), "only while injecting");
        assert_eq!(take_refusals().len(), 1);
    }

    #[test]
    fn a_read_refused_for_money_is_answered_like_a_tdlib_error() {
        let answer = refused_read_answer(r#"{"@type":"getReceivedGifts","@extra":"42"}"#)
            .map(|text| serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default());
        assert_eq!(
            answer,
            Some(json!({
                "@type": "error", "code": 403, "message": "QUILL_AGENT_DRIVING", "@extra": "42"
            }))
        );
        // Writes, reads that act and requests nobody waits for are refused
        // outright instead.
        for request in [
            r#"{"@type":"sendMessage","@extra":"1"}"#,
            r#"{"@type":"getLoginUrlInfo","@extra":"1"}"#,
            r#"{"@type":"getReceivedGifts"}"#,
            "not json",
        ] {
            assert_eq!(refused_read_answer(request), None, "{request}");
        }
    }
}
