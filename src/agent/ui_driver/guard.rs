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
//!   reads and harmless changes ([`request_allowed`]); anything else is
//!   dropped with a refusal, so a click that reaches a send, a reaction, a
//!   poll vote, a bot button or a payment sends nothing.
//!
//! The flag is per thread: input is injected on the UI thread, and every
//! handler and the sender run on it in the same call.

use std::cell::{Cell, RefCell};

thread_local! {
    static INJECTING: Cell<bool> = const { Cell::new(false) };
    static REFUSALS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Whether the input being handled right now came from the driver.
pub fn injecting() -> bool {
    INJECTING.with(Cell::get)
}

/// Mark the start and end of one injected input on this thread.
pub fn set_injecting(on: bool) {
    INJECTING.with(|flag| flag.set(on));
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

/// Reads that act for the user or start a flow with a bot or a site; never
/// while injecting even though their names start like a read.
const DENIED_READS: &[&str] = &[
    "getCallbackQueryAnswer",
    "getLoginUrl",
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
    crate::agent::tools::td_policy::is_read(method) || ALLOWED_CHANGES.contains(&method)
}

/// The `@type` of a request's JSON text, without parsing all of it.
fn method_of(request: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(request).ok()?;
    value.get("@type")?.as_str().map(str::to_string)
}

/// Screen a request the window is about to send. `Ok` when it may go
/// (always while nothing is injected); `Err` names the method and records
/// the refusal.
pub fn screen_request(request: &str) -> Result<(), String> {
    if !injecting() {
        return Ok(());
    }
    let method = method_of(request).unwrap_or_default();
    if request_allowed(&method) {
        return Ok(());
    }
    let reason = format!(
        "Quill did not send {} while an agent was driving the window: writes go through the agent tools (send_message, react, delete_messages, mark_read...) or the user.",
        if method.is_empty() {
            "a request"
        } else {
            method.as_str()
        }
    );
    refuse(reason.clone());
    Err(reason)
}

#[cfg(test)]
mod tests {
    use super::{
        injecting, refuse_if_injecting, request_allowed, screen_request, set_injecting,
        take_refusals,
    };

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
}
