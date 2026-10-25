//! Requests from agents that only the user can answer, shown as a dialog in
//! Quill: `request_access`, and each write while "Confirm each send" is on.
//!
//! An agent thread files a [`Prompt`] and waits; the UI reads [`current`]
//! on every render and answers with [`answer`]. Nothing here touches the
//! grants: the UI applies what the user chose before it answers.

use crate::ids::AccountKey;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

/// What an agent asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// One chat at `scope`.
    Chat {
        chat_id: i64,
        title: String,
        secret: bool,
        scope: super::grants::Scope,
    },
    /// Every chat that is not secret, at `scope`.
    AllChats { scope: super::grants::Scope },
    /// The contact list.
    Contacts,
    /// One write in `chat_id`, while "Confirm each send" is on. `action` is a
    /// short sentence ("Send a message"), `preview` what will be sent.
    Write {
        chat_id: i64,
        title: String,
        action: String,
        preview: String,
    },
}

#[derive(Debug, Clone)]
pub struct Prompt {
    pub id: u64,
    pub account: AccountKey,
    /// The MCP client's name ("Claude Code"), when it gave one.
    pub agent: String,
    pub ask: Ask,
    /// The agent's own words for why.
    pub reason: String,
}

/// The user's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied,
}

struct Pending {
    prompt: Prompt,
    reply: SyncSender<Decision>,
}

static QUEUE: Mutex<VecDeque<Pending>> = Mutex::new(VecDeque::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
/// Prompts waiting at once, across agents.
const MAX_WAITING: usize = 4;

/// File a prompt. `None` when too many are already waiting.
pub fn file(
    account: AccountKey,
    agent: String,
    ask: Ask,
    reason: String,
) -> Option<(u64, Receiver<Decision>)> {
    let mut queue = crate::locks::lock_or_recover(&QUEUE);
    if queue.len() >= MAX_WAITING {
        return None;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let (reply, rx) = mpsc::sync_channel(1);
    queue.push_back(Pending {
        prompt: Prompt {
            id,
            account,
            agent,
            ask,
            reason,
        },
        reply,
    });
    drop(queue);
    crate::wake::AGENT.poke();
    Some((id, rx))
}

/// Wait for the user's answer to prompt `id`; on timeout the prompt is
/// withdrawn and `None` returned.
pub fn wait(id: u64, rx: &Receiver<Decision>, timeout: Duration) -> Option<Decision> {
    if let Ok(decision) = rx.recv_timeout(timeout) {
        return Some(decision);
    }
    withdraw(id);
    // An answer given just before the withdrawal is already in the channel.
    rx.try_recv().ok()
}

/// Take prompt `id` off the screen without an answer.
pub fn withdraw(id: u64) {
    crate::locks::lock_or_recover(&QUEUE).retain(|p| p.prompt.id != id);
    crate::wake::AGENT.poke();
}

/// The prompt the user sees now (the oldest).
pub fn current() -> Option<Prompt> {
    crate::locks::lock_or_recover(&QUEUE)
        .front()
        .map(|pending| pending.prompt.clone())
}

/// The oldest prompt for `account`.
pub fn current_for(account: &AccountKey) -> Option<Prompt> {
    crate::locks::lock_or_recover(&QUEUE)
        .iter()
        .find(|pending| pending.prompt.account == *account)
        .map(|pending| pending.prompt.clone())
}

/// Answer prompt `id` and take it off the screen.
pub fn answer(id: u64, decision: Decision) {
    let mut queue = crate::locks::lock_or_recover(&QUEUE);
    if let Some(index) = queue.iter().position(|p| p.prompt.id == id)
        && let Some(pending) = queue.remove(index)
    {
        let _ = pending.reply.try_send(decision);
    }
    drop(queue);
    crate::wake::AGENT.poke();
}

/// Turning access off answers every prompt with Denied.
pub fn deny_all() {
    let drained: Vec<Pending> = crate::locks::lock_or_recover(&QUEUE).drain(..).collect();
    for pending in drained {
        let _ = pending.reply.try_send(Decision::Denied);
    }
    crate::wake::AGENT.poke();
}

#[cfg(test)]
mod tests {
    use super::{Ask, Decision, answer, current, file, wait};
    use crate::ids::AccountKey;
    use std::time::Duration;

    #[test]
    fn a_prompt_waits_for_the_user_and_withdraws_on_timeout() {
        let _guard = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
        let (id, rx) = file(
            AccountKey::primary(),
            "Test agent".into(),
            Ask::Contacts,
            "why".into(),
        )
        .unwrap_or_else(|| panic!("filed"));
        assert_eq!(current().map(|p| p.id), Some(id));
        answer(id, Decision::Allowed);
        assert_eq!(
            wait(id, &rx, Duration::from_millis(10)),
            Some(Decision::Allowed)
        );
        assert!(current().is_none_or(|p| p.id != id));

        let (id, rx) = file(
            AccountKey::primary(),
            "Test agent".into(),
            Ask::Contacts,
            "why".into(),
        )
        .unwrap_or_else(|| panic!("filed"));
        assert_eq!(wait(id, &rx, Duration::from_millis(10)), None);
        assert!(current().is_none_or(|p| p.id != id), "withdrawn");
    }
}
