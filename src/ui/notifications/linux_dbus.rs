//! Linux desktop notifications over the session bus
//! (`org.freedesktop.Notifications`), the way Telegram Desktop shows them
//! (`platform/linux/notifications_manager_linux.cpp`): `Notify` with the
//! action list and hints from `quill::notify::dbus`, `CloseNotification` to
//! withdraw one, and the daemon's `ActionInvoked`, `NotificationReplied`
//! and `NotificationClosed` signals for clicks, inline replies and dismissals.
//!
//! One connection per process, made on the first notification's worker
//! thread (never on the UI thread). A listener thread turns signals into the
//! same `(chat, action)` clicks the other platforms queue (or, for an
//! account that is not on screen, `(account, chat, action)`); the typed
//! reply waits in [`take_reply_text`]. No session bus means [`notifier`] is
//! `None` and the caller falls back to `notify-send`.

use crate::ui::background_accounts::AccountClicks;
use quill::ids::{AccountKey, ChatId};
use quill::notify::NotificationAction;
use quill::notify::dbus::{
    APP_NAME, BUS_NAME, Capabilities, INTERFACE, OBJECT_PATH, Signal, app_icon, interpret_signal,
    notify_actions, notify_hints,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::Value;

type Clicks = Arc<Mutex<Vec<(ChatId, NotificationAction)>>>;

/// Whose notification: the account it belongs to (`None` for the account
/// on screen when it was shown) and the chat.
pub(super) type NotifyKey = (Option<AccountKey>, ChatId);

/// Which chat each live notification belongs to, and the other way round so
/// a newer message replaces the chat's toast and "read elsewhere" can close
/// it.
#[derive(Default)]
struct Shown {
    chat_by_id: HashMap<u32, NotifyKey>,
    id_by_chat: HashMap<NotifyKey, u32>,
}

impl Shown {
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
    clicks: Clicks,
    account_clicks: AccountClicks,
    reply_texts: Mutex<HashMap<NotifyKey, String>>,
}

static NOTIFIER: OnceLock<Option<Notifier>> = OnceLock::new();

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The process-wide notifier, connected on first use. `None`, for good,
/// when the session bus or the daemon cannot be reached.
pub(super) fn notifier(
    clicks: &Clicks,
    account_clicks: &AccountClicks,
) -> Option<&'static Notifier> {
    NOTIFIER
        .get_or_init(
            || match Notifier::connect(clicks.clone(), account_clicks.clone()) {
                Ok(notifier) => Some(notifier),
                Err(error) => {
                    eprintln!("desktop notifications over D-Bus unavailable: {error}");
                    None
                }
            },
        )
        .as_ref()
}

/// The text typed into the chat's notification, if it was answered inline.
/// Taking it clears it. Only the account on screen offers the field.
pub(super) fn take_reply_text(chat_id: ChatId) -> Option<String> {
    let notifier = NOTIFIER.get()?.as_ref()?;
    lock(&notifier.reply_texts).remove(&(None, chat_id))
}

impl Notifier {
    fn connect(clicks: Clicks, account_clicks: AccountClicks) -> zbus::Result<Self> {
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
            account_clicks,
            reply_texts: Mutex::new(HashMap::new()),
        };
        std::thread::Builder::new()
            .name("quill-notify-signals".to_string())
            .spawn(move || listen(signals))
            .map_err(|error| zbus::Error::Failure(error.to_string()))?;
        Ok(notifier)
    }

    fn proxy(connection: &Connection) -> zbus::Result<Proxy<'static>> {
        Proxy::new(connection, BUS_NAME, OBJECT_PATH, INTERFACE)
    }

    /// Show (or, for a chat with a toast still up, replace) a notification.
    /// `reply_field`: offer the daemon's inline reply where it has one.
    /// Blocks for the bus round trip: call from a worker thread.
    pub(super) fn show(
        &self,
        key: NotifyKey,
        title: &str,
        body: &str,
        avatar: Option<&Path>,
        locked: bool,
        reply_field: bool,
    ) -> zbus::Result<()> {
        let replaces = lock(&self.shown).id_by_chat.get(&key).copied();
        let capabilities = Capabilities {
            inline_reply: self.capabilities.inline_reply && reply_field,
            ..self.capabilities
        };
        let actions = notify_actions(capabilities, locked);
        let hints: HashMap<&str, Value<'_>> = notify_hints()
            .into_iter()
            .map(|(key, value)| (key, Value::from(value)))
            .collect();
        let id: u32 = Self::proxy(&self.connection)?.call(
            "Notify",
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
        )?;
        lock(&self.shown).insert(id, key);
        Ok(())
    }

    /// Withdraw the chat's notification, if one is up. Blocks for the bus
    /// round trip: call from a worker thread.
    pub(super) fn close(&self, key: &NotifyKey) -> zbus::Result<()> {
        let Some(id) = lock(&self.shown).id_by_chat.get(key).copied() else {
            return Ok(());
        };
        Self::proxy(&self.connection)?.call::<_, _, ()>("CloseNotification", &(id,))
    }

    /// Queue a click for the account it belongs to.
    fn push_click(&self, key: NotifyKey, action: NotificationAction) {
        match key {
            (None, chat_id) => lock(&self.clicks).push((chat_id, action)),
            (Some(account), chat_id) => lock(&self.account_clicks).push((account, chat_id, action)),
        }
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
        if let Some(notifier) = NOTIFIER.get().and_then(Option::as_ref) {
            notifier.handle(signal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_toast_for_the_chat_replaces_the_old_id() {
        let mut shown = Shown::default();
        let key = (None, ChatId(7));
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

    #[test]
    fn the_same_chat_on_two_accounts_keeps_two_toasts() {
        let mut shown = Shown::default();
        let active = (None, ChatId(7));
        let other = (AccountKey::new("account-1"), ChatId(7));
        shown.insert(1, active.clone());
        shown.insert(2, other.clone());
        assert_eq!(shown.id_by_chat.get(&active), Some(&1));
        assert_eq!(shown.id_by_chat.get(&other), Some(&2));
        assert_eq!(shown.close(2), Some(other));
        assert_eq!(shown.id_by_chat.get(&active), Some(&1));
    }
}
