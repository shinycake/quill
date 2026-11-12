//! Linux desktop notifications over the session bus
//! (`org.freedesktop.Notifications`), the way Telegram Desktop shows them
//! (`platform/linux/notifications_manager_linux.cpp`): `Notify` with the
//! action list and hints from `quill::notify::dbus`, `CloseNotification` to
//! withdraw one, and the daemon's `ActionInvoked`, `NotificationReplied`
//! and `NotificationClosed` signals for clicks, inline replies and dismissals.
//!
//! One connection per process, made on the first notification's worker
//! thread (never on the UI thread). A listener thread turns signals into
//! clicks that name the account the notification was shown for, queued for
//! `drain_account_clicks` like the other platforms' (which runs them on the
//! account on screen only when it is still theirs); the typed reply waits in
//! [`take_reply_text`]. No session bus means [`notifier`] is `None` and the
//! caller falls back to `notify-send`.

use crate::ui::background_accounts::AccountClicks;
use quill::ids::{AccountKey, ChatId};
use quill::lang::{keys, tr};
use quill::notify::NotificationAction;
use quill::notify::dbus::{
    APP_NAME, BUS_NAME, Capabilities, INTERFACE, OBJECT_PATH, Signal, app_icon, interpret_signal,
    notify_actions, notify_hints,
};
use quill::notify_accounts::ChatClick;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::Value;

/// Whose notification: the account it was shown for and the chat.
pub(super) type NotifyKey = (AccountKey, ChatId);

/// Which chat each live notification belongs to, and the other way round so
/// a newer message replaces the chat's toast and "read elsewhere" can close
/// it.
#[derive(Default)]
struct Shown {
    chat_by_id: HashMap<u32, NotifyKey>,
    id_by_chat: HashMap<NotifyKey, u32>,
    /// Toasts whose `Notify` round trip is out: their id is not known yet.
    /// One entry per call, so two calls for one chat each keep their own
    /// withdrawal mark.
    sending: HashMap<Ticket, (NotifyKey, Sending)>,
    next_ticket: u64,
}

/// One `Notify` call on the bus.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Ticket(u64);

/// A `Notify` still on the bus: whether the chat was read (withdrawn)
/// before its id came back, so the toast is closed the moment it lands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sending {
    Open,
    Withdrawn,
}

impl Shown {
    fn begin(&mut self, key: NotifyKey) -> Ticket {
        let ticket = Ticket(self.next_ticket);
        self.next_ticket = self.next_ticket.wrapping_add(1);
        self.sending.insert(ticket, (key, Sending::Open));
        ticket
    }

    /// The `Notify` failed: nothing was shown.
    fn abandon(&mut self, ticket: Ticket) {
        self.sending.remove(&ticket);
    }

    /// Withdraw every toast of the chat that is still being sent. Returns
    /// whether there is one.
    fn withdraw_sending(&mut self, key: &NotifyKey) -> bool {
        let mut any = false;
        for (owner, state) in self.sending.values_mut() {
            if owner == key {
                *state = Sending::Withdrawn;
                any = true;
            }
        }
        any
    }

    /// The `Notify` of `ticket` answered with `id`. Returns whether the chat
    /// was withdrawn meanwhile (the caller closes the toast).
    fn land(&mut self, id: u32, ticket: Ticket, key: NotifyKey) -> bool {
        let withdrawn = matches!(self.sending.remove(&ticket), Some((_, Sending::Withdrawn)));
        self.insert(id, key);
        withdrawn
    }

    fn insert(&mut self, id: u32, key: NotifyKey) {
        if let Some(previous) = self.id_by_chat.insert(key.clone(), id) {
            self.chat_by_id.remove(&previous);
        }
        self.chat_by_id.insert(id, key);
    }

    fn close(&mut self, id: u32) -> Option<NotifyKey> {
        let key = self.chat_by_id.remove(&id)?;
        if self.id_by_chat.get(&key) == Some(&id) {
            self.id_by_chat.remove(&key);
        }
        Some(key)
    }
}

pub(super) struct Notifier {
    connection: Connection,
    capabilities: Capabilities,
    shown: Mutex<Shown>,
    clicks: AccountClicks,
    reply_texts: Mutex<HashMap<NotifyKey, String>>,
}

/// The connection once made. A failed attempt is not cached here: the
/// daemon may start after Quill (mako or dunst launched by the session
/// after an autostarted Quill), so [`CONNECT_RETRY`] gates the next try.
/// The listener thread clears it when its bus connection ends, so the next
/// notification connects again (a bus restart).
static NOTIFIER: Mutex<Option<Arc<Notifier>>> = Mutex::new(None);

fn current_notifier() -> Option<Arc<Notifier>> {
    lock(&NOTIFIER).clone()
}

/// When the last connect failed, so a missing daemon is asked for again
/// every [`REPROBE`] rather than on every notification or never.
static CONNECT_RETRY: Mutex<RetryGate> = Mutex::new(RetryGate { failed_at: None });

/// The last failed connect, and whether the next may run.
#[derive(Debug)]
struct RetryGate {
    failed_at: Option<std::time::Instant>,
}

impl RetryGate {
    fn due(&self, now: std::time::Instant) -> bool {
        self.failed_at
            .is_none_or(|at| now.saturating_duration_since(at) >= REPROBE)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    quill::locks::lock_or_recover(mutex)
}

/// The notifier if a worker already connected it; never connects. The UI
/// thread's way in (closing a toast): a connect blocks for the bus round
/// trips, up to the D-Bus timeout with a hanging daemon, and waits on
/// [`CONNECT_RETRY`] while a worker connects. With no connection no toast
/// went out over D-Bus, so there is nothing to close.
pub(super) fn connected_notifier() -> Option<Arc<Notifier>> {
    current_notifier()
}

/// The process-wide notifier, connected on first use. `None` while the
/// session bus or the daemon cannot be reached; asked again after
/// [`REPROBE`], as the capability probe does. Blocks while it connects:
/// worker threads only (the UI thread uses [`connected_notifier`]).
pub(super) fn notifier_on_worker(clicks: &AccountClicks) -> Option<Arc<Notifier>> {
    if let Some(notifier) = current_notifier() {
        return Some(notifier);
    }
    let mut gate = lock(&CONNECT_RETRY);
    // Another worker may have connected while this one waited.
    if let Some(notifier) = current_notifier() {
        return Some(notifier);
    }
    let now = std::time::Instant::now();
    if !gate.due(now) {
        return None;
    }
    match Notifier::connect(clicks.clone()) {
        Ok(notifier) => {
            gate.failed_at = None;
            let notifier = Arc::new(notifier);
            *lock(&NOTIFIER) = Some(notifier.clone());
            Some(notifier)
        }
        Err(error) => {
            eprintln!("desktop notifications over D-Bus unavailable: {error}");
            gate.failed_at = Some(now);
            None
        }
    }
}

/// How often a missing notification daemon is looked for again.
const REPROBE: std::time::Duration = std::time::Duration::from_secs(30);

/// The capability probe. It runs when somebody asks (the Notifications page
/// or a notification deciding where to show), never on a clock: a daemon
/// found missing is asked for again by the first question after
/// [`REPROBE`], as a daemon may start after Quill.
enum Probe {
    /// Nobody has asked yet.
    Idle,
    /// The probe thread is out; `last` is the earlier answer, still what
    /// is reported until the new one lands.
    Asking {
        _thread: std::thread::JoinHandle<Option<()>>,
        last: quill::notify::custom::NativeService,
    },
    Answered {
        service: quill::notify::custom::NativeService,
        at: std::time::Instant,
    },
}

impl Probe {
    fn reported(&self) -> quill::notify::custom::NativeService {
        use quill::notify::custom::NativeService;
        match self {
            Self::Idle => NativeService::Unknown,
            Self::Asking { last, .. } => *last,
            Self::Answered { service, .. } => *service,
        }
    }

    /// Whether a question now starts a probe.
    fn due(&self, now: std::time::Instant) -> bool {
        use quill::notify::custom::NativeService;
        match self {
            Self::Idle => true,
            Self::Asking { .. } => false,
            Self::Answered { service, at } => {
                *service == NativeService::Missing && now.saturating_duration_since(*at) >= REPROBE
            }
        }
    }
}

fn ask_daemon() -> quill::notify::custom::NativeService {
    use quill::notify::custom::NativeService;
    Connection::session()
        .and_then(|connection| {
            Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE)?
                .call::<_, _, Vec<String>>("GetCapabilities", &())
        })
        .map(|list| NativeService::Present {
            full: quill::notify::dbus::full_parity(&list),
        })
        .unwrap_or(NativeService::Missing)
}

/// What the notification daemon can do (tdesktop `Supported` and
/// `ByDefault`), asked on a thread of its own: `Unknown` until it
/// answers, `Missing` without a session bus or daemon.
pub(in crate::ui) fn linux_service_capabilities() -> quill::notify::custom::NativeService {
    use quill::notify::custom::NativeService;
    static PROBE: Mutex<Probe> = Mutex::new(Probe::Idle);
    let mut probe = lock(&PROBE);
    let now = std::time::Instant::now();
    if probe.due(now) {
        let last = probe.reported();
        // The thread's answer waits for this lock, so it lands after the
        // stage below is stored, never before.
        let spawned = quill::thread_guard::spawn_guarded("quill-notify-caps", || {
            let service = ask_daemon();
            *lock(&PROBE) = Probe::Answered {
                service,
                at: std::time::Instant::now(),
            };
            // The page and the delivery path read it on their next frame.
            quill::wake::UPDATES.poke();
        });
        *probe = match spawned {
            Ok(thread) => Probe::Asking {
                _thread: thread,
                last,
            },
            Err(_) => Probe::Answered {
                service: NativeService::Missing,
                at: now,
            },
        };
    }
    probe.reported()
}

/// The text typed into `account`'s notification for the chat, if it was
/// answered inline. Taking it clears it. Only the account on screen offers
/// the field.
pub(super) fn take_reply_text(account: &AccountKey, chat_id: ChatId) -> Option<String> {
    let notifier = current_notifier()?;
    lock(&notifier.reply_texts).remove(&(account.clone(), chat_id))
}

impl Notifier {
    fn connect(clicks: AccountClicks) -> zbus::Result<Self> {
        let connection = Connection::session()?;
        // Subscribe before the first `Notify` so no signal is missed.
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(INTERFACE)?
            .build();
        let signals = MessageIterator::for_match_rule(rule, &connection, None)?;
        let capabilities = Capabilities::from_list(
            &Self::proxy(&connection)?.call::<_, _, Vec<String>>("GetCapabilities", &())?,
        );
        let notifier = Self {
            connection,
            capabilities,
            shown: Mutex::new(Shown::default()),
            clicks,
            reply_texts: Mutex::new(HashMap::new()),
        };
        quill::thread_guard::spawn_guarded("quill-notify-signals", move || listen(signals))
            .map_err(|error| zbus::Error::Failure(error.to_string()))?;
        Ok(notifier)
    }

    fn proxy(connection: &Connection) -> zbus::Result<Proxy<'static>> {
        Proxy::new(connection, BUS_NAME, OBJECT_PATH, INTERFACE)
    }

    /// Show (or, for a chat with a toast still up, replace) a notification.
    /// `reply_field`: offer the daemon's inline reply where it has one;
    /// `photo` is the message's photo, the notification's image.
    /// Blocks for the bus round trip: call from a worker thread.
    pub(super) fn show(
        &self,
        key: NotifyKey,
        title: &str,
        body: &str,
        avatar: Option<&Path>,
        photo: Option<&Path>,
        no_actions: bool,
        reply_field: bool,
    ) -> zbus::Result<()> {
        let replaces = lock(&self.shown).id_by_chat.get(&key).copied();
        let capabilities = Capabilities {
            inline_reply: self.capabilities.inline_reply && reply_field,
            ..self.capabilities
        };
        let actions = notify_actions(capabilities, no_actions);
        let hints: HashMap<&str, Value<'_>> = notify_hints(photo)
            .into_iter()
            .map(|(key, value)| (key, Value::from(value)))
            .collect();
        let proxy = Self::proxy(&self.connection)?;
        let ticket = lock(&self.shown).begin(key.clone());
        let sent: zbus::Result<u32> = proxy.call(
            tr(keys::Q_PREFS_NOTIFY),
            &(
                APP_NAME,
                replaces.unwrap_or(0),
                app_icon(avatar),
                title,
                body,
                actions,
                hints,
                -1_i32,
            ),
        );
        let id = match sent {
            Ok(id) => id,
            Err(error) => {
                lock(&self.shown).abandon(ticket);
                return Err(error);
            }
        };
        // Read elsewhere while `Notify` was out: the toast goes down now.
        let withdrawn = lock(&self.shown).land(id, ticket, key);
        if withdrawn {
            Self::proxy(&self.connection)?.call::<_, _, ()>("CloseNotification", &(id,))?;
        }
        Ok(())
    }

    /// Withdraw the chat's notification, if one is up. Blocks for the bus
    /// round trip: call from a worker thread.
    pub(super) fn close(&self, key: &NotifyKey) -> zbus::Result<()> {
        let id = {
            let mut shown = lock(&self.shown);
            // A toast still on its way closes when it lands (`show`).
            shown.withdraw_sending(key);
            shown.id_by_chat.get(key).copied()
        };
        let Some(id) = id else {
            return Ok(());
        };
        Self::proxy(&self.connection)?.call::<_, _, ()>("CloseNotification", &(id,))
    }

    /// Queue a click with the account its notification was shown for.
    fn push_click(&self, key: NotifyKey, action: NotificationAction) {
        let (account, chat_id) = key;
        lock(&self.clicks).push(ChatClick {
            account,
            chat_id,
            action,
        });
        // The poll loop sleeps while idle; wake it for the click.
        quill::wake::UPDATES.poke();
    }

    fn handle(&self, signal: Signal) {
        match signal {
            Signal::Action { id, action } => {
                let key = lock(&self.shown).chat_by_id.get(&id).cloned();
                if let Some(key) = key {
                    self.push_click(key, action);
                }
            }
            Signal::Replied { id, text } => {
                let key = lock(&self.shown).chat_by_id.get(&id).cloned();
                if let Some(key) = key {
                    lock(&self.reply_texts).insert(key.clone(), text);
                    self.push_click(key, NotificationAction::Reply);
                }
            }
            Signal::Closed { id } => {
                lock(&self.shown).close(id);
            }
        }
    }
}

/// The listener thread: every signal of the interface, for the life of the
/// process.
fn listen(signals: MessageIterator) {
    for message in signals.flatten() {
        let header = message.header();
        let Some(member) = header.member() else {
            continue;
        };
        let body = message.body();
        // `ActionInvoked` and `NotificationReplied` carry `(u32, String)`,
        // `NotificationClosed` `(u32, u32)`.
        let (id, text) = match body.deserialize::<(u32, String)>() {
            Ok((id, text)) => (id, Some(text)),
            Err(_) => match body.deserialize::<(u32, u32)>() {
                Ok((id, _reason)) => (id, None),
                Err(_) => continue,
            },
        };
        let Some(signal) = interpret_signal(member.as_str(), id, text.as_deref()) else {
            continue;
        };
        if let Some(notifier) = current_notifier() {
            notifier.handle(signal);
        }
    }
    // The bus connection ended: its toasts and their ids went with it. The
    // next notification connects again.
    *lock(&NOTIFIER) = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_toast_for_the_chat_replaces_the_old_id() {
        let mut shown = Shown::default();
        let key = (AccountKey::primary(), ChatId(7));
        shown.insert(1, key.clone());
        shown.insert(2, key.clone());
        assert_eq!(shown.id_by_chat.get(&key), Some(&2));
        assert_eq!(shown.chat_by_id.get(&1), None);
        // Closing the stale id leaves the live one.
        assert_eq!(shown.close(1), None);
        assert_eq!(shown.id_by_chat.get(&key), Some(&2));
        assert_eq!(shown.close(2), Some(key));
        assert!(shown.id_by_chat.is_empty());
    }

    /// Round 17 (area B, finding 10): a daemon missing at the first
    /// notification is asked for again after the re-probe interval.
    #[test]
    fn a_failed_connect_is_retried_after_the_reprobe_interval() {
        let start = std::time::Instant::now();
        let mut gate = RetryGate { failed_at: None };
        assert!(gate.due(start), "the first connect runs");
        gate.failed_at = Some(start);
        assert!(!gate.due(start + REPROBE / 2));
        assert!(gate.due(start + REPROBE));
    }

    #[test]
    fn a_missing_daemon_is_asked_again_only_by_a_question_after_the_interval() {
        use quill::notify::custom::NativeService;
        let start = std::time::Instant::now();
        assert!(Probe::Idle.due(start), "the first question probes");
        let missing = Probe::Answered {
            service: NativeService::Missing,
            at: start,
        };
        assert!(!missing.due(start + REPROBE / 2));
        assert!(missing.due(start + REPROBE));
        let present = Probe::Answered {
            service: NativeService::Present { full: true },
            at: start,
        };
        assert!(!present.due(start + REPROBE * 10), "a found daemon is kept");
    }

    #[test]
    fn a_chat_read_while_its_toast_is_on_the_bus_is_closed_when_it_lands() {
        let mut shown = Shown::default();
        let key = (AccountKey::primary(), ChatId(7));
        let first = shown.begin(key.clone());
        assert!(shown.withdraw_sending(&key));
        assert!(
            shown.land(5, first, key.clone()),
            "closed as soon as it has an id"
        );
        // A newer message after that shows as usual.
        let second = shown.begin(key.clone());
        assert!(!shown.land(6, second, key.clone()));
        assert!(!shown.withdraw_sending(&key), "nothing is on the bus now");
    }

    #[test]
    fn two_toasts_on_the_bus_for_one_chat_are_both_closed_when_it_is_read() {
        let mut shown = Shown::default();
        let key = (AccountKey::primary(), ChatId(7));
        let first = shown.begin(key.clone());
        let second = shown.begin(key.clone());
        assert!(shown.withdraw_sending(&key));
        assert!(shown.land(1, first, key.clone()));
        assert!(
            shown.land(2, second, key.clone()),
            "the second keeps its mark"
        );
    }

    #[test]
    fn a_failed_notify_leaves_nothing_to_withdraw() {
        let mut shown = Shown::default();
        let key = (AccountKey::primary(), ChatId(7));
        let ticket = shown.begin(key.clone());
        shown.abandon(ticket);
        assert!(!shown.withdraw_sending(&key));
    }

    #[test]
    fn the_same_chat_on_two_accounts_keeps_two_toasts() {
        let mut shown = Shown::default();
        let active = (AccountKey::primary(), ChatId(7));
        let other = (AccountKey::new("account-1").unwrap(), ChatId(7));
        shown.insert(1, active.clone());
        shown.insert(2, other.clone());
        assert_eq!(shown.id_by_chat.get(&active), Some(&1));
        assert_eq!(shown.id_by_chat.get(&other), Some(&2));
        assert_eq!(shown.close(2), Some(other));
        assert_eq!(shown.id_by_chat.get(&active), Some(&1));
    }
}
