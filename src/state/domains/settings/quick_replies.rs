//! Quick reply shortcuts and their messages (see `crate::quick_replies` for
//! what Telegram Desktop does).
use crate::quick_replies::{
    MESSAGES_MAX, QuickRepliesUpdate, QuickReplyMessage, QuickReplyShortcut, SHORTCUTS_MAX,
    count_label,
};
use crate::state::fetch::Fetch;
use crate::state::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickRepliesState {
    /// Shortcut ids in the order TDLib sent (`updateQuickReplyShortcuts`).
    pub order: Vec<i32>,
    pub shortcuts: HashMap<i32, QuickReplyShortcut>,
    /// The messages of each shortcut asked for: in flight, in, or failed.
    /// A failed load stays (so nothing asks again on its own) until the
    /// dialog opens again (`reopen`); a deleted shortcut takes its entry.
    pub messages: HashMap<i32, Fetch<Vec<QuickReplyMessage>>>,
    /// `loadQuickReplyShortcuts`: once per session. A failed load waits for
    /// the dialog to open again (`reopen`) instead of being asked on every
    /// poll.
    pub shortcuts_load: Fetch<()>,
    /// The last refusal, in words, for the open dialog.
    pub error: Option<String>,
    /// TDLib options `quick_reply_shortcut_count_max` and
    /// `quick_reply_shortcut_message_count_max`, once reported.
    pub shortcut_count_max: Option<usize>,
    pub message_count_max: Option<usize>,
    /// Counter for `sendQuickReplyShortcutMessages.sending_id`.
    pub next_sending_id: i32,
    /// The changes and sends on their way, by request: each leaves with
    /// its own answer or error (`settled`), so a double tap sends once.
    pub in_flight: HashMap<RequestId, QuickReplyWork>,
}

/// One quick reply request on its way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickReplyWork {
    /// A change (add, edit, rename, delete), as the request it sent without
    /// its id.
    Change(String),
    /// A shortcut being sent to a chat.
    Send(ChatId, i32),
}

impl QuickRepliesState {
    /// Shortcuts in list order. A shortcut missing from the order (an
    /// update raced ahead of it) follows, sorted by id.
    pub fn list(&self) -> Vec<&QuickReplyShortcut> {
        let mut out: Vec<&QuickReplyShortcut> = self
            .order
            .iter()
            .filter_map(|id| self.shortcuts.get(id))
            .collect();
        let mut rest: Vec<&QuickReplyShortcut> = self
            .shortcuts
            .values()
            .filter(|s| !self.order.contains(&s.id))
            .collect();
        rest.sort_by_key(|s| s.id);
        out.extend(rest);
        out
    }

    /// How many shortcuts the account may have.
    pub fn shortcut_limit(&self) -> usize {
        self.shortcut_count_max.unwrap_or(SHORTCUTS_MAX)
    }

    /// How many messages one shortcut may have.
    pub fn message_limit(&self) -> usize {
        self.message_count_max.unwrap_or(MESSAGES_MAX)
    }

    /// The shortcut named `name`, compared without case.
    pub fn by_name(&self, name: &str) -> Option<&QuickReplyShortcut> {
        self.shortcuts
            .values()
            .find(|s| s.name.eq_ignore_ascii_case(name) || s.name == name)
    }

    /// This change is already on its way.
    pub fn is_changing(&self, probe: &str) -> bool {
        self.in_flight
            .values()
            .any(|work| matches!(work, QuickReplyWork::Change(sent) if sent == probe))
    }

    /// This shortcut is already on its way to this chat.
    pub fn is_sending(&self, chat_id: ChatId, shortcut_id: i32) -> bool {
        self.in_flight
            .values()
            .any(|work| *work == QuickReplyWork::Send(chat_id, shortcut_id))
    }

    /// `request` was answered, failed or was lost: its work is over.
    pub fn settled(&mut self, request: RequestId) {
        self.in_flight.remove(&request);
    }

    /// The shortcut's messages are in, or their load failed: nothing is
    /// left to wait for.
    pub fn messages_settled(&self, shortcut_id: i32) -> bool {
        self.messages
            .get(&shortcut_id)
            .is_some_and(|fetch| !fetch.is_loading())
    }

    /// The dialog opened: the last refusal's line goes, and a shortcut
    /// whose load failed may be asked for again.
    pub fn reopen(&mut self) {
        self.error = None;
        self.messages
            .retain(|_, fetch| !matches!(fetch, Fetch::Failed { .. }));
        if matches!(self.shortcuts_load, Fetch::Failed { .. }) {
            self.shortcuts_load = Fetch::Idle;
        }
    }

    pub fn messages_of(&self, shortcut_id: i32) -> &[QuickReplyMessage] {
        self.messages
            .get(&shortcut_id)
            .and_then(Fetch::data)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Composer rows: `(id, name, "N messages")` for shortcuts that have
    /// messages and start with `prefix` (tdesktop `FieldAutocomplete`).
    pub fn menu_rows(&self, prefix: &str) -> Vec<(i32, String, String)> {
        let prefix = prefix.to_lowercase();
        self.list()
            .into_iter()
            .filter(|s| s.message_count >= 1 && s.name.to_lowercase().starts_with(&prefix))
            .map(|s| (s.id, s.name.clone(), count_label(s.message_count)))
            .collect()
    }
}

impl Session {
    /// `/` menu rows for a private chat with a person (not a bot): the quick reply shortcuts that have messages.
    pub fn quick_reply_menu_items(&self, chat_id: ChatId) -> Vec<CommandMenuItem> {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return Vec::new();
        };
        let ChatKind::Private { user_id } = chat.kind else {
            return Vec::new();
        };
        // Saved Messages is included, as in tdesktop.
        if self.bots.bot_user_ids.contains(&user_id.0) {
            return Vec::new();
        }
        self.settings
            .quick_replies
            .menu_rows("")
            .into_iter()
            .map(|(id, name, description)| CommandMenuItem {
                command: name,
                description,
                global: false,
                is_ephemeral: false,
                quick_reply: Some(id),
                bot: None,
            })
            .collect()
    }

    pub fn apply_quick_replies(&mut self, update: QuickRepliesUpdate) {
        let state = &mut self.settings.quick_replies;
        match update {
            QuickRepliesUpdate::Shortcut(shortcut) => {
                state.shortcuts.insert(shortcut.id, shortcut);
            }
            QuickRepliesUpdate::Deleted { shortcut_id } => {
                state.shortcuts.remove(&shortcut_id);
                state.messages.remove(&shortcut_id);
                state.order.retain(|id| *id != shortcut_id);
            }
            QuickRepliesUpdate::Order { shortcut_ids } => state.order = shortcut_ids,
            QuickRepliesUpdate::Messages {
                shortcut_id,
                messages,
            } => {
                if let Some(shortcut) = state.shortcuts.get_mut(&shortcut_id) {
                    shortcut.message_count = messages.len() as i32;
                }
                state.messages.insert(shortcut_id, Fetch::Ready(messages));
            }
        }
    }

    /// `updateOption`: the quick reply limits.
    pub(crate) fn apply_quick_replies_option(&mut self, name: &str, value: &OptionValue) {
        let OptionValue::Integer(limit) = value else {
            return;
        };
        let limit = usize::try_from(*limit).ok().filter(|l| *l > 0);
        match name {
            "quick_reply_shortcut_count_max" => {
                self.settings.quick_replies.shortcut_count_max = limit;
            }
            "quick_reply_shortcut_message_count_max" => {
                self.settings.quick_replies.message_count_max = limit;
            }
            _ => {}
        }
    }

    /// `ok` of a quick reply request: the last refusal's line goes.
    pub(crate) fn apply_quick_replies_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Settings(
            SettingsPurpose::LoadQuickReplyShortcuts
            | SettingsPurpose::LoadQuickReplyMessages { .. }
            | SettingsPurpose::MutateQuickReplies,
        )) = pending.map(|p| p.purpose)
        {
            self.settings.quick_replies.error = None;
        }
        if let Some(RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyShortcuts)) =
            pending.map(|p| p.purpose)
        {
            self.settings.quick_replies.shortcuts_load.succeed(());
        }
        // TDLib reports the messages as an update before this answer: a load
        // still waiting here had nothing to report.
        if let Some(pending) = pending
            && let RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyMessages { shortcut_id }) =
                pending.purpose
            && let Some(fetch) = self.settings.quick_replies.messages.get_mut(&shortcut_id)
        {
            let held = fetch.data().cloned().unwrap_or_default();
            fetch.answered(Some(pending.id), held);
        }
    }

    /// A failed quick reply request; `true` when it was one.
    pub(crate) fn apply_quick_replies_error(
        &mut self,
        err: &TdError,
        purpose: Option<RequestPurpose>,
        request: Option<RequestId>,
    ) -> bool {
        let state = &mut self.settings.quick_replies;
        match purpose {
            Some(RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyShortcuts)) => {
                state.shortcuts_load.fail(error_reason(err));
            }
            Some(RequestPurpose::Settings(SettingsPurpose::LoadQuickReplyMessages {
                shortcut_id,
            })) => {
                // Marked failed, not un-requested: a poll waiting for the
                // messages stops instead of asking again on every tick.
                // Only the load that is waited for fails.
                if let Some(fetch) = state.messages.get_mut(&shortcut_id) {
                    fetch.failed(request, error_reason(err), false);
                }
            }
            Some(RequestPurpose::Settings(SettingsPurpose::MutateQuickReplies)) => {}
            _ => return false,
        }
        state.error = Some(format!("Quick replies: {}", error_reason(err)));
        true
    }
}
