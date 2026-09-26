//! Parity slice: chat folder create/edit model and folder-filter matching.
//!
//! Pure logic — no GPUI, no TDLib I/O. The folder editor dialog (create /
//! edit) freezes into a `ChatFolderSpec` for `createChatFolder` /
//! `editChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13358` / `:13361`).
//! `matches_folder_filters` decides whether a chat would still belong to a
//! folder after being dropped from its explicit include lists — the
//! remove-from-folder chain needs it because there is no
//! `removeChatFromList` in TDLib 1.8.67.

use crate::state::ChatSummary;
use crate::telegram::envelope::{ChatFolderSpec, ChatKind, ParsedUser};
use std::collections::HashSet;

/// Editor model for the create/edit folder dialog (tdesktop-like): a name,
/// include-type flags, always-included / never-included chat sets, and the
/// exclude-muted/read/archived flags. Pinned chats are not managed here
/// (Quill has no per-folder pin UI yet) — edits preserve the existing
/// `pinned_chat_ids`.
#[derive(Debug, Clone, Default)]
pub struct FolderEditor {
    pub name: String,
    pub include_contacts: bool,
    pub include_non_contacts: bool,
    pub include_bots: bool,
    pub include_groups: bool,
    pub include_channels: bool,
    pub exclude_muted: bool,
    pub exclude_read: bool,
    pub exclude_archived: bool,
    pub included: HashSet<i64>,
    pub excluded: HashSet<i64>,
    /// `pinned_chat_ids` carried over from the loaded spec on edit (create
    /// starts empty). The dialog never edits these.
    pub pinned: Vec<i64>,
}

impl FolderEditor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prefill from a `getChatFolder` spec (edit path).
    pub fn from_spec(spec: &ChatFolderSpec) -> Self {
        Self {
            name: spec.name.clone(),
            include_contacts: spec.include_contacts,
            include_non_contacts: spec.include_non_contacts,
            include_bots: spec.include_bots,
            include_groups: spec.include_groups,
            include_channels: spec.include_channels,
            exclude_muted: spec.exclude_muted,
            exclude_read: spec.exclude_read,
            exclude_archived: spec.exclude_archived,
            included: spec.included_chat_ids.iter().copied().collect(),
            excluded: spec.excluded_chat_ids.iter().copied().collect(),
            pinned: spec.pinned_chat_ids.clone(),
        }
    }

    /// Freeze the dialog state into the `chatFolder` spec for
    /// `createChatFolder` / `editChatFolder`. Ids are sorted for
    /// deterministic request JSON.
    pub fn to_spec(&self) -> ChatFolderSpec {
        let mut included: Vec<i64> = self.included.iter().copied().collect();
        included.sort_unstable();
        let mut excluded: Vec<i64> = self.excluded.iter().copied().collect();
        excluded.sort_unstable();
        ChatFolderSpec {
            name: self.name.trim().to_string(),
            pinned_chat_ids: self.pinned.clone(),
            included_chat_ids: included,
            excluded_chat_ids: excluded,
            exclude_muted: self.exclude_muted,
            exclude_read: self.exclude_read,
            exclude_archived: self.exclude_archived,
            include_contacts: self.include_contacts,
            include_non_contacts: self.include_non_contacts,
            include_bots: self.include_bots,
            include_groups: self.include_groups,
            include_channels: self.include_channels,
        }
    }

    /// Schema doc on `chatFolderName` (`schema/td_api.tl:3458`): "1-12
    /// characters without line feeds". Returns the validation error, if any.
    pub fn validate(&self) -> Option<&'static str> {
        let name = self.name.trim();
        if name.is_empty() {
            return Some("Enter a folder name.");
        }
        if name.chars().count() > 12 {
            return Some("Folder names are at most 12 characters.");
        }
        if name.contains(['\n', '\r']) {
            return Some("Folder names can't contain line breaks.");
        }
        None
    }

    pub fn toggle_included(&mut self, chat_id: i64) {
        if !self.included.remove(&chat_id) {
            self.included.insert(chat_id);
            // A chat can't be both always-included and never-included.
            self.excluded.remove(&chat_id);
        }
    }

    pub fn toggle_excluded(&mut self, chat_id: i64) {
        if !self.excluded.remove(&chat_id) {
            self.excluded.insert(chat_id);
            self.included.remove(&chat_id);
        }
    }
}

/// Would `chat` belong to a folder described by `spec` through its *filter*
/// flags alone (ignoring the explicit pinned/included lists)? Used by the
/// remove-from-folder chain: dropping a chat from `included_chat_ids` is
/// not enough when it still matches `include_*` — then it must also land in
/// `excluded_chat_ids`.
///
/// `user` is the `ParsedUser` for private/secret chats (contact/bot
/// status); `None` for chats whose user is unknown (treated as
/// non-contact, non-bot).
pub fn matches_folder_filters(
    chat: &ChatSummary,
    spec: &ChatFolderSpec,
    user: Option<&ParsedUser>,
) -> bool {
    if spec.excluded_chat_ids.contains(&chat.id.0) {
        return false;
    }
    let type_matches = match &chat.kind {
        ChatKind::Private { .. } | ChatKind::Secret { .. } => {
            let is_bot = user.is_some_and(|u| u.is_bot);
            let is_contact = user.is_some_and(|u| u.is_contact);
            if is_bot {
                spec.include_bots
            } else if is_contact {
                spec.include_contacts
            } else {
                spec.include_non_contacts
            }
        }
        ChatKind::BasicGroup { .. } => spec.include_groups,
        ChatKind::Supergroup { is_channel, .. } => {
            if *is_channel {
                spec.include_channels
            } else {
                spec.include_groups
            }
        }
        ChatKind::Unknown => false,
    };
    if !type_matches {
        return false;
    }
    if spec.exclude_muted && chat.is_muted() {
        return false;
    }
    if spec.exclude_read && chat.unread_count == 0 {
        return false;
    }
    if spec.exclude_archived && chat.in_archive {
        return false;
    }
    true
}

/// Build the edited spec for removing `chat_id` from a folder: drop it from
/// the explicit pinned/included lists, and — if it would still match the
/// folder's filter flags — add it to `excluded_chat_ids`.
pub fn spec_without_chat(
    spec: &ChatFolderSpec,
    chat: &ChatSummary,
    user: Option<&ParsedUser>,
) -> ChatFolderSpec {
    let mut edited = spec.clone();
    edited.pinned_chat_ids.retain(|id| *id != chat.id.0);
    edited.included_chat_ids.retain(|id| *id != chat.id.0);
    if !edited.excluded_chat_ids.contains(&chat.id.0) && matches_folder_filters(chat, spec, user) {
        edited.excluded_chat_ids.push(chat.id.0);
        edited.excluded_chat_ids.sort_unstable();
    }
    edited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ChatId, MessageId, UserId};
    use crate::state::ChatSummary;
    use crate::telegram::envelope::{ChatNotificationSettings, UserStatusKind};
    use std::collections::BTreeMap;

    fn private_chat(id: i64) -> ChatSummary {
        ChatSummary {
            id: ChatId(id),
            title: format!("chat {id}"),
            kind: ChatKind::Private {
                user_id: UserId(1000 + id),
            },
            unread_count: 0,
            last_read_inbox_message_id: MessageId(0),
            last_read_outbox_message_id: MessageId(0),
            order: 0,
            is_pinned: false,
            in_main_list: true,
            in_archive: false,
            archive_order: 0,
            archive_is_pinned: false,
            folder_positions: BTreeMap::new(),
            notification_settings: ChatNotificationSettings::default(),
            last_preview: String::new(),
            typing_senders: Vec::new(),
            draft: None,
            my_member_status: None,
            my_admin_can_post_messages: None,
            is_forum: None,
            photo_file_id: None,
            can_send_basic_messages: true,
        }
    }

    fn test_user(id: i64, is_contact: bool, is_bot: bool) -> ParsedUser {
        ParsedUser {
            id,
            first_name: "Test".into(),
            last_name: String::new(),
            username: String::new(),
            phone_number: String::new(),
            is_contact,
            is_bot,
            status: UserStatusKind::Empty,
            photo_small_file_id: 0,
        }
    }

    #[test]
    fn editor_round_trips_spec() {
        let spec = ChatFolderSpec {
            name: "Work".into(),
            pinned_chat_ids: vec![11],
            included_chat_ids: vec![12, 13],
            excluded_chat_ids: vec![14],
            exclude_muted: true,
            include_contacts: true,
            ..ChatFolderSpec::default()
        };
        let editor = FolderEditor::from_spec(&spec);
        assert_eq!(editor.name, "Work");
        assert!(editor.included.contains(&12));
        assert!(editor.excluded.contains(&14));
        // Pinned ids are preserved through the editor (not editable).
        let back = editor.to_spec();
        assert_eq!(back.pinned_chat_ids, vec![11]);
        assert_eq!(back.included_chat_ids, vec![12, 13]);
        assert!(back.exclude_muted);
    }

    #[test]
    fn editor_toggles_are_mutually_exclusive() {
        let mut editor = FolderEditor::new();
        editor.toggle_included(12);
        assert!(editor.included.contains(&12));
        editor.toggle_excluded(12);
        assert!(!editor.included.contains(&12));
        assert!(editor.excluded.contains(&12));
    }

    #[test]
    fn editor_validates_name() {
        let mut editor = FolderEditor::new();
        assert_eq!(editor.validate(), Some("Enter a folder name."));
        editor.name = "This name is way too long".into();
        assert!(editor.validate().is_some());
        editor.name = "Work".into();
        assert_eq!(editor.validate(), None);
    }

    #[test]
    fn filter_match_contact_and_exclude_muted() {
        let chat = private_chat(12);
        let user = test_user(1012, true, false);
        let mut spec = ChatFolderSpec::default();
        assert!(!matches_folder_filters(&chat, &spec, Some(&user)));
        spec.include_contacts = true;
        assert!(matches_folder_filters(&chat, &spec, Some(&user)));
        spec.exclude_muted = true;
        // Not muted → still matches; muted → excluded.
        assert!(matches_folder_filters(&chat, &spec, Some(&user)));
        let mut muted = chat.clone();
        muted.notification_settings = ChatNotificationSettings {
            use_default_mute_for: false,
            mute_for: i32::MAX,
            ..ChatNotificationSettings::default()
        };
        assert!(!matches_folder_filters(&muted, &spec, Some(&user)));
    }

    #[test]
    fn spec_without_chat_drops_explicit_and_excludes_filter_match() {
        let chat = private_chat(12);
        let user = test_user(1012, true, false);
        // Explicitly included: removal just drops it from the list.
        let spec = ChatFolderSpec {
            included_chat_ids: vec![12],
            ..ChatFolderSpec::default()
        };
        let edited = spec_without_chat(&spec, &chat, Some(&user));
        assert!(edited.included_chat_ids.is_empty());
        assert!(edited.excluded_chat_ids.is_empty());
        // Filter-matched: removal must also exclude it.
        let spec = ChatFolderSpec {
            include_contacts: true,
            ..ChatFolderSpec::default()
        };
        let edited = spec_without_chat(&spec, &chat, Some(&user));
        assert_eq!(edited.excluded_chat_ids, vec![12]);
    }
}
