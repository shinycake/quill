//! Parity slice: chat folder create/edit model and folder-filter matching.
//!
//! Pure logic — no GPUI, no TDLib I/O. The folder editor dialog (create /
//! edit) freezes into a `ChatFolderSpec` for `createChatFolder` /
//! `editChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13358` / `:13361`).
//! `matches_folder_filters` decides whether a chat would still belong to a
//! folder after being dropped from its explicit include lists — the
//! remove-from-folder chain needs it because there is no
//! `removeChatFromList` in TDLib 1.8.67.
#![deny(clippy::indexing_slicing)]

use crate::state::ChatSummary;
use crate::telegram::envelope::{ChatFolderSpec, ChatKind, ParsedUser};
use std::collections::{BTreeSet, HashSet};

/// Editor model for the create/edit folder dialog (tdesktop-like): a name,
/// include-type flags, always-included / never-included chat sets, and the
/// exclude-muted/read/archived flags. Pinned chats are not managed here
/// (Quill has no per-folder pin UI yet) — edits preserve the existing
/// `pinned_chat_ids`.
#[derive(Debug, Clone)]
pub struct FolderEditor {
    pub name: String,
    /// Chosen `chatFolderIcon` name; `None` = the default icon.
    pub icon_name: Option<String>,
    /// Tag color carried over from the loaded spec (not edited here).
    pub color_id: i32,
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

impl Default for FolderEditor {
    fn default() -> Self {
        Self {
            name: String::new(),
            icon_name: None,
            color_id: -1,
            include_contacts: false,
            include_non_contacts: false,
            include_bots: false,
            include_groups: false,
            include_channels: false,
            exclude_muted: false,
            exclude_read: false,
            exclude_archived: false,
            included: HashSet::new(),
            excluded: HashSet::new(),
            pinned: Vec::new(),
        }
    }
}

impl FolderEditor {
    pub fn new() -> Self {
        Self::default()
    }

    /// The icon the folder will show: the chosen one, or the default its
    /// rules produce (tdesktop `ComputeDefaultFilterIcon`).
    pub fn icon_or_default(&self) -> String {
        match &self.icon_name {
            Some(name) => name.clone(),
            None => crate::folder_icons::default_icon_name(&self.to_spec()).to_string(),
        }
    }

    /// Prefill from a `getChatFolder` spec (edit path).
    pub fn from_spec(spec: &ChatFolderSpec) -> Self {
        Self {
            name: spec.name.clone(),
            icon_name: spec.icon_name.clone(),
            color_id: spec.color_id,
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
            icon_name: self.icon_name.clone(),
            color_id: self.color_id,
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

    /// What Save sends given the folder as it is now (`fresh`, `None` while
    /// there is no current copy: the caller fetches it first): the form
    /// as it is when the folder is as the form started from it, else the
    /// edits merged onto the newer folder.
    pub fn spec_onto_current(
        &self,
        base: &ChatFolderSpec,
        fresh: Option<&ChatFolderSpec>,
    ) -> Option<ChatFolderSpec> {
        let fresh = fresh?;
        Some(if fresh == base {
            self.to_spec()
        } else {
            self.merged_onto(base, fresh)
        })
    }

    /// What Save sends when the folder changed after the editor opened
    /// (`base` is the spec the form started from, `fresh` the folder now):
    /// the fields the person touched go onto the fresh folder, and what
    /// changed elsewhere (a pin, a chat added or removed) stays. Pinned
    /// chats are not edited here, so they are the fresh ones.
    pub fn merged_onto(&self, base: &ChatFolderSpec, fresh: &ChatFolderSpec) -> ChatFolderSpec {
        let mine = self.to_spec();
        fn pick<T: PartialEq + Clone>(mine: &T, base: &T, fresh: &T) -> T {
            if mine != base {
                mine.clone()
            } else {
                fresh.clone()
            }
        }
        fn merge_ids(mine: &[i64], base: &[i64], fresh: &[i64]) -> BTreeSet<i64> {
            let mut out: BTreeSet<i64> = fresh.iter().copied().collect();
            out.extend(mine.iter().filter(|id| !base.contains(id)));
            for id in base.iter().filter(|id| !mine.contains(id)) {
                out.remove(id);
            }
            out
        }
        let mut included = merge_ids(
            &mine.included_chat_ids,
            &base.included_chat_ids,
            &fresh.included_chat_ids,
        );
        let mut excluded = merge_ids(
            &mine.excluded_chat_ids,
            &base.excluded_chat_ids,
            &fresh.excluded_chat_ids,
        );
        // A chat is in one list: the person's choice wins, else the
        // newer one.
        for id in included
            .intersection(&excluded)
            .copied()
            .collect::<Vec<_>>()
        {
            if mine.included_chat_ids.contains(&id) && !base.included_chat_ids.contains(&id) {
                excluded.remove(&id);
            } else {
                included.remove(&id);
            }
        }
        // A pinned chat is not also in the always-included list.
        included.retain(|id| !fresh.pinned_chat_ids.contains(id));
        ChatFolderSpec {
            name: pick(&mine.name, &base.name, &fresh.name),
            icon_name: pick(&mine.icon_name, &base.icon_name, &fresh.icon_name),
            color_id: pick(&mine.color_id, &base.color_id, &fresh.color_id),
            pinned_chat_ids: fresh.pinned_chat_ids.clone(),
            included_chat_ids: included.into_iter().collect(),
            excluded_chat_ids: excluded.into_iter().collect(),
            exclude_muted: pick(
                &mine.exclude_muted,
                &base.exclude_muted,
                &fresh.exclude_muted,
            ),
            exclude_read: pick(&mine.exclude_read, &base.exclude_read, &fresh.exclude_read),
            exclude_archived: pick(
                &mine.exclude_archived,
                &base.exclude_archived,
                &fresh.exclude_archived,
            ),
            include_contacts: pick(
                &mine.include_contacts,
                &base.include_contacts,
                &fresh.include_contacts,
            ),
            include_non_contacts: pick(
                &mine.include_non_contacts,
                &base.include_non_contacts,
                &fresh.include_non_contacts,
            ),
            include_bots: pick(&mine.include_bots, &base.include_bots, &fresh.include_bots),
            include_groups: pick(
                &mine.include_groups,
                &base.include_groups,
                &fresh.include_groups,
            ),
            include_channels: pick(
                &mine.include_channels,
                &base.include_channels,
                &fresh.include_channels,
            ),
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

/// The confirmation toast after adding or removing a chat from a folder
/// (`lng_filters_toast_add` / `lng_filters_toast_remove`).
pub fn folder_membership_toast(chat: &str, folder: &str, added: bool) -> String {
    let verb = if added { "added to" } else { "removed from" };
    format!("{chat} {verb} {folder} folder")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_toast_texts() {
        assert_eq!(
            folder_membership_toast("Maya", "Work", true),
            "Maya added to Work folder"
        );
        assert_eq!(
            folder_membership_toast("Maya", "Work", false),
            "Maya removed from Work folder"
        );
    }
    use crate::chatlist_style::ChatPreviewStyle;
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
            last_preview_style: ChatPreviewStyle::default(),
            last_preview_sender: String::new(),
            last_preview_thumb: None,
            last_message: None,
            sender_actions: Vec::new(),
            draft: None,
            my_member_status: None,
            my_admin_can_post_messages: None,
            my_admin_can_invite_users: None,
            my_admin_can_change_info: None,
            my_admin_can_send_welcome_messages: None,
            my_admin_can_promote_members: None,
            my_admin_can_restrict_members: None,
            my_admin_can_pin_messages: None,
            my_restriction: None,
            my_rights_fetched: false,
            is_forum: None,
            photo_file_id: None,
            can_send_basic_messages: true,
            permissions: None,
            can_be_deleted_for_all_users: false,
            can_be_deleted_only_for_self: false,
            is_marked_as_unread: false,
            unread_mention_count: 0,
            unread_reaction_count: 0,
            unread_poll_vote_count: 0,
            can_be_reported: false,
            blocked: false,
            secret_state: None,
            message_auto_delete_time: 0,
            video_chat: None,
        }
    }

    fn test_user(id: i64, is_contact: bool, is_bot: bool) -> ParsedUser {
        ParsedUser {
            id,
            first_name: "Test".into(),
            last_name: String::new(),
            username: String::new(),
            active_usernames: Vec::new(),
            disabled_usernames: Vec::new(),
            editable_username: String::new(),
            collectible_usernames: Vec::new(),
            phone_number: String::new(),
            is_contact,
            is_bot,
            is_deleted: false,
            is_inline: false,
            inline_query_placeholder: String::new(),
            has_topics: false,
            allows_users_to_create_topics: false,
            can_join_groups: false,
            has_main_web_app: false,
            can_be_added_to_attachment_menu: false,
            can_be_edited: false,
            can_manage_bots: false,
            can_connect_to_business: false,
            status: UserStatusKind::Empty,
            status_hidden_by_me: false,
            is_support: false,
            bot_active_users: 0,
            photo_small_file_id: 0,
            photo_small: None,
            photo_id: 0,
            photo_has_animation: false,
            accent_color_id: 0,
            background_custom_emoji_id: 0,
            profile_accent_color_id: -1,
            profile_background_custom_emoji_id: 0,
            upgraded_gift_colors_id: 0,
            is_premium: false,
            verification: Default::default(),
            emoji_status_id: 0,
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

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn spec(included: &[i64], pinned: &[i64]) -> ChatFolderSpec {
        ChatFolderSpec {
            name: "Work".into(),
            included_chat_ids: included.to_vec(),
            pinned_chat_ids: pinned.to_vec(),
            ..ChatFolderSpec::default()
        }
    }

    #[test]
    fn a_save_keeps_what_changed_elsewhere_and_what_the_person_touched() {
        let base = spec(&[1, 2], &[]);
        let mut editor = FolderEditor::from_spec(&base);
        // The person removes chat 2 and adds chat 5, and renames the folder.
        editor.toggle_included(2);
        editor.toggle_included(5);
        editor.name = "Jobs".into();
        // Meanwhile elsewhere: chat 1 was pinned (so it left the included
        // list), chat 7 was added.
        let fresh = spec(&[2, 7], &[1]);
        let saved = editor.merged_onto(&base, &fresh);
        assert_eq!(saved.name, "Jobs", "the rename is the person's");
        assert_eq!(saved.pinned_chat_ids, vec![1], "the new pin stays");
        assert_eq!(saved.included_chat_ids, vec![5, 7], "2 removed, 5 and 7 in");
    }

    #[test]
    fn save_goes_onto_a_folder_refetched_since_the_form_was_filled() {
        let base = spec(&[1, 2], &[]);
        let mut editor = FolderEditor::from_spec(&base);
        editor.toggle_included(5);
        // Another consumer refetched the folder after a pin elsewhere, so
        // a fresh copy exists and differs from what the form started from.
        let fresh = spec(&[2, 7], &[1]);
        let saved = editor.spec_onto_current(&base, Some(&fresh));
        assert_eq!(
            saved.as_ref().map(|s| s.pinned_chat_ids.clone()),
            Some(vec![1]),
            "the newer pin is not overwritten by the old copy"
        );
        assert_eq!(saved.map(|s| s.included_chat_ids), Some(vec![2, 5, 7]));
        // Nothing changed elsewhere: the form goes as it is.
        let same = editor.spec_onto_current(&base, Some(&base));
        assert_eq!(same, Some(editor.to_spec()));
        // No current copy: the folder is fetched first.
        assert_eq!(editor.spec_onto_current(&base, None), None);
    }

    #[test]
    fn an_untouched_form_saves_the_fresh_folder() {
        let base = spec(&[1], &[]);
        let editor = FolderEditor::from_spec(&base);
        let mut fresh = spec(&[1, 3], &[9]);
        fresh.name = "Renamed elsewhere".into();
        assert_eq!(editor.merged_onto(&base, &fresh), fresh);
    }
}
