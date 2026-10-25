//! Agent access in the UI: which account Settings edits, the pending
//! "all chats" consent, and the task that re-renders when grants, requests
//! or the switch change (`docs/decisions/agent-access.md`). Grants and
//! requests live in `quill::agent`; the UI reads them on every render and
//! caches nothing.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::activity;
use quill::agent::grants::{self, AccountGrants, ChatGrant, Scope};
use quill::agent::prompts::{self, Ask, Prompt};
use quill::ids::AccountKey;
use std::time::Duration;

/// "All other chats" waiting for the user to say yes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AllChatsConsent {
    pub(super) account: AccountKey,
    pub(super) scope: Scope,
}

/// What a grant row changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GrantTarget {
    Saved,
    Bots,
    OwnChannels,
    AllChats,
    Chat(i64),
}

pub(crate) struct AgentUi {
    /// The account Settings edits; `None` follows the active account.
    pub(super) picked: Option<AccountKey>,
    /// Set while the consent dialog for "All other chats" is open.
    pub(super) consent: Option<AllChatsConsent>,
    /// How many activity rows Settings shows.
    pub(super) activity_shown: usize,
}

/// Activity rows shown at first, and added by each "Show more".
pub(super) const ACTIVITY_PAGE: usize = 50;

impl AgentUi {
    /// Starts the task that wakes the UI when agent state changes and
    /// raises the window for a new request.
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        QuillApp::spawn_main_in(window, cx, async move |app, cx| {
            let mut announced = 0u64;
            let mut seen = quill::wake::AGENT.epoch();
            loop {
                super::wake_wait::sleep_or_wake(
                    cx.background_executor(),
                    &quill::wake::AGENT,
                    seen,
                    Duration::from_secs(300),
                )
                .await;
                seen = quill::wake::AGENT.epoch();
                let alive = app
                    .update_in(cx, |_, window, cx| {
                        // The user is usually in another app: a request
                        // they have not seen yet brings Quill forward.
                        let fresh = prompts::current_for(&active_account())
                            .map(|prompt| prompt.id)
                            .filter(|id| *id != announced);
                        if let Some(id) = fresh {
                            announced = id;
                            super::link_inbox::raise_main_window(window, cx);
                        }
                        cx.notify();
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        Self {
            picked: None,
            consent: None,
            activity_shown: ACTIVITY_PAGE,
        }
    }
}

/// The account agents act on.
pub(super) fn active_account() -> AccountKey {
    quill::settings::safe_app_root()
        .map(|root| quill::settings::active_account(&root))
        .unwrap_or_else(AccountKey::primary)
}

/// Whether TDLib's chat id layout puts `chat_id` in the secret chats' range.
pub(super) fn is_secret_chat_id(chat_id: i64) -> bool {
    // TDLib: secret chat ids are -2_000_000_000_000 plus the 32-bit secret
    // chat id (td/telegram/DialogId.h).
    const MIN: i64 = -2_000_000_000_000 + i32::MIN as i64 + 1;
    const MAX: i64 = -2_000_000_000_000 + i32::MAX as i64;
    (MIN..=MAX).contains(&chat_id)
}

/// Moving to `next` gives agents more than `current` does.
pub(super) fn widens(current: Option<Scope>, next: Option<Scope>) -> bool {
    next > current
}

/// Set one grant row.
pub(super) fn apply_scope(grants: &mut AccountGrants, target: GrantTarget, scope: Option<Scope>) {
    match target {
        GrantTarget::Saved => grants.saved = scope,
        GrantTarget::Bots => grants.bots = scope,
        GrantTarget::OwnChannels => grants.own_channels = scope,
        GrantTarget::AllChats => grants.all_chats = scope,
        GrantTarget::Chat(id) => {
            if let Some(grant) = grants.chats.get_mut(&id) {
                grant.scope = scope;
            }
        }
    }
}

/// Apply what the user allowed in a request.
pub(super) fn apply_ask(grants: &mut AccountGrants, ask: &Ask) {
    match ask {
        Ask::Chat {
            chat_id,
            title,
            scope,
            ..
        } => {
            grants.chats.insert(
                *chat_id,
                ChatGrant {
                    scope: Some(*scope),
                    title: title.clone(),
                },
            );
        }
        Ask::AllChats { scope } => grants.all_chats = Some(*scope),
        Ask::Contacts => grants.contacts = true,
        // A confirmation changes no grant: it covers this one write.
        Ask::Write { .. } | Ask::Raw { .. } => {}
    }
}

/// The scope a grant row shows.
pub(super) fn scope_of(grants: &AccountGrants, target: GrantTarget) -> Option<Scope> {
    match target {
        GrantTarget::Saved => grants.saved,
        GrantTarget::Bots => grants.bots,
        GrantTarget::OwnChannels => grants.own_channels,
        GrantTarget::AllChats => grants.all_chats,
        GrantTarget::Chat(id) => grants.chats.get(&id).and_then(|grant| grant.scope),
    }
}

impl QuillApp {
    /// The account Settings edits: the picked one while it is still signed
    /// in, else the active one.
    pub(super) fn agent_settings_account(&self) -> AccountKey {
        let accounts = self.agent_account_list();
        self.agent_ui
            .picked
            .clone()
            .filter(|picked| accounts.iter().any(|record| &record.key == picked))
            .unwrap_or_else(active_account)
    }

    /// The signed-in accounts, in the registry's order.
    pub(super) fn agent_account_list(&self) -> Vec<quill::settings::AccountRecord> {
        quill::settings::safe_app_root()
            .map(|root| quill::settings::list_accounts(&root))
            .unwrap_or_default()
    }

    /// The request the dialog shows: the oldest one for the active account.
    pub(super) fn agent_request(&self) -> Option<Prompt> {
        if !sees_prompts() {
            return None;
        }
        prompts::current_for(&active_account())
    }

    /// The user picked `scope` for a row. "All other chats" asks first when
    /// the change gives agents more than they have.
    pub(super) fn choose_agent_scope(
        &mut self,
        target: GrantTarget,
        scope: Option<Scope>,
        cx: &mut Context<Self>,
    ) {
        let account = self.agent_settings_account();
        if target == GrantTarget::AllChats
            && let Some(next) = scope
            && widens(grants::for_account(&account).all_chats, scope)
        {
            self.agent_ui.consent = Some(AllChatsConsent {
                account,
                scope: next,
            });
        } else {
            grants::update(&account, |g| apply_scope(g, target, scope));
        }
        cx.notify();
    }

    /// The user said yes in the consent dialog.
    pub(super) fn confirm_all_chats(&mut self, cx: &mut Context<Self>) {
        if let Some(consent) = self.agent_ui.consent.take() {
            grants::update(&consent.account, |g| g.all_chats = Some(consent.scope));
        }
        cx.notify();
    }

    pub(super) fn cancel_all_chats(&mut self, cx: &mut Context<Self>) {
        self.agent_ui.consent = None;
        cx.notify();
    }

    pub(super) fn set_agent_contacts(&mut self, on: bool, cx: &mut Context<Self>) {
        let account = self.agent_settings_account();
        grants::update(&account, |g| g.contacts = on);
        cx.notify();
    }

    pub(super) fn remove_agent_chat(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let account = self.agent_settings_account();
        grants::update(&account, |g| {
            g.chats.remove(&chat_id);
        });
        cx.notify();
    }

    /// Answer the request `prompt`: on Allow the grant is applied first, so
    /// the agent's next call already sees it. Nothing happens when the
    /// request is no longer the one waiting (the agent gave up).
    pub(super) fn answer_agent_request(
        &mut self,
        prompt: &Prompt,
        allow: bool,
        cx: &mut Context<Self>,
    ) {
        let waiting =
            prompts::current_for(&prompt.account).is_some_and(|current| current.id == prompt.id);
        if waiting && allow {
            grants::update(&prompt.account, |g| apply_ask(g, &prompt.ask));
        }
        let decision = if waiting && allow {
            prompts::Decision::Allowed
        } else {
            prompts::Decision::Denied
        };
        prompts::answer(prompt.id, decision);
        cx.notify();
    }
}

/// The grants, prompts and switch are process-wide: UI tests that touch
/// them take this lock and reset them.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(super) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// The prompt queue is process-wide and UI tests run in parallel, one
// thread each: only a test that set agent state up sees its prompts.
#[cfg(test)]
thread_local! {
    static SEES_PROMPTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn sees_prompts() -> bool {
    SEES_PROMPTS.with(std::cell::Cell::get)
}

#[cfg(not(test))]
fn sees_prompts() -> bool {
    true
}

/// The agents connected now.
pub(super) fn visible_connected() -> Vec<activity::Connected> {
    if sees_prompts() {
        activity::connected()
    } else {
        Vec::new()
    }
}

/// What agents did for `account`, newest first.
pub(super) fn visible_activity(account: &AccountKey) -> Vec<activity::Action> {
    if sees_prompts() {
        activity::recent(account)
    } else {
        Vec::new()
    }
}

/// Back to a known state: access on, confirmation off, default grants, no
/// waiting requests, an empty activity log.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(super) fn reset_for_test(account: &AccountKey) {
    SEES_PROMPTS.with(|sees| sees.set(true));
    quill::fast_buttons::set_persistence(false);
    prompts::deny_all();
    activity::clear();
    quill::agent::access::set_confirm_sends(false);
    grants::update(account, |g| *g = AccountGrants::default());
    quill::agent::access::set_enabled(true);
}

#[cfg(test)]
mod tests {
    use super::{GrantTarget, apply_ask, apply_scope, is_secret_chat_id, scope_of, widens};
    use quill::agent::grants::{AccountGrants, ChatGrant, Scope};
    use quill::agent::prompts::Ask;

    #[test]
    fn only_a_wider_scope_needs_consent() {
        assert!(widens(None, Some(Scope::Read)));
        assert!(widens(Some(Scope::Read), Some(Scope::ReadWrite)));
        assert!(!widens(Some(Scope::ReadWrite), Some(Scope::Read)));
        assert!(!widens(Some(Scope::Read), None));
        assert!(!widens(None, None));
    }

    #[test]
    fn secret_chat_ids_follow_tdlibs_layout() {
        assert!(is_secret_chat_id(-2_000_000_000_001));
        assert!(is_secret_chat_id(-1_998_000_000_000));
        assert!(!is_secret_chat_id(-1_001_234_567_890));
        assert!(!is_secret_chat_id(123_456_789));
    }

    #[test]
    fn rows_and_requests_edit_the_grants() {
        let mut grants = AccountGrants::default();
        apply_scope(&mut grants, GrantTarget::Saved, None);
        assert_eq!(scope_of(&grants, GrantTarget::Saved), None);
        apply_scope(&mut grants, GrantTarget::AllChats, Some(Scope::Read));
        assert_eq!(grants.all_chats, Some(Scope::Read));

        apply_ask(
            &mut grants,
            &Ask::Chat {
                chat_id: 7,
                title: "Ann".into(),
                secret: false,
                scope: Scope::Read,
            },
        );
        assert_eq!(scope_of(&grants, GrantTarget::Chat(7)), Some(Scope::Read));
        apply_scope(&mut grants, GrantTarget::Chat(7), None);
        assert_eq!(
            grants.chats.get(&7),
            Some(&ChatGrant {
                scope: None,
                title: "Ann".into()
            }),
            "Off keeps the entry so the chat stays blocked"
        );
        apply_ask(&mut grants, &Ask::Contacts);
        assert!(grants.contacts);
        apply_ask(
            &mut grants,
            &Ask::AllChats {
                scope: Scope::ReadWrite,
            },
        );
        assert_eq!(grants.all_chats, Some(Scope::ReadWrite));
    }
}
