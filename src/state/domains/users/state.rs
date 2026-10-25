//! Users: contacts, profiles, profile photos, secret chats and the info panel: the `users` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct UsersState {
    /// B10: chat-id lists behind the profile panels, keyed by
    /// `(kind, user/chat id)`: groups in common, similar channels and
    /// the suitable personal channels. Chat objects themselves arrive via
    /// `updateNewChat` before the `chats` answer.
    pub profile_chat_lists: HashMap<(ProfileChatsKind, i64), ProfileChatsFetch>,
    /// Paging of the groups in common list, by user id.
    pub common_groups_paging: HashMap<i64, CommonGroupsPaging>,
    /// `chats.total_count` of a `getChatSimilarChats` answer, by channel
    /// chat id: more similar channels exist than the list shows (the
    /// non-Premium list is capped), which the strip's "More Channels" tile
    /// counts.
    pub similar_chat_totals: HashMap<i64, i32>,
    /// B10: profile photo galleries (`getUserProfilePhotos`) by user id.
    pub user_profile_photos: HashMap<i64, ProfilePhotosFetch>,
    /// The current photo of a group or channel (`supergroupFullInfo.photo`
    /// / `basicGroupFullInfo.photo`), by chat id; `None` = it has none.
    /// Dropped on `updateChatPhoto`.
    pub chat_current_photos: HashMap<i64, Option<ProfilePhoto>>,
    /// A group or channel's photo history, by chat id. Dropped on
    /// `updateChatPhoto`.
    pub chat_photo_history: HashMap<i64, ChatPhotosFetch>,
    /// Phase 6: `getContacts` result — user ids, in server order. `None`
    /// until the first `users` response; `contacts_error` records a failed
    /// fetch so the UI can offer a retry.
    pub contacts: Option<Vec<i64>>,
    pub contacts_error: bool,
    /// Slice A6: outcome line for contacts mutations (`removeContacts`,
    /// `importContacts`, `clearImportedContacts`) — set on ok and on
    /// error, cleared by the next mutation; shown in the contacts
    /// settings section.
    pub contacts_notice: Option<String>,
    /// The open "Block {name}" box for a reply in the Replies chat.
    pub replies_block: Option<crate::message_menu_origin::RepliesBlock>,
    /// The option `replies_bot_chat_id`: the Replies chat (it differs on
    /// the test environment).
    pub replies_bot_chat_id: Option<i64>,
    /// Phase 6: cached `getUserFullInfo` bios, keyed by user id. Presence
    /// records "fetched" so the driver never refetches.
    pub user_full_infos: HashMap<i64, UserFullInfoData>,
    /// `(user id, photo id)` whose full info was asked for to learn the
    /// clips of a video userpic (`Session::take_userpic_full_info_ask`).
    pub userpic_full_info_asked: std::collections::HashSet<(i64, i64)>,
    /// A5: latest `checkChatUsername` verdict for the edit-profile
    /// dialog: (checked username text, result). Written by the driver
    /// before `apply` takes the pending request; the dialog only shows it
    /// when it matches the current input text (stale verdicts ignored).
    pub username_check: Option<(String, UsernameCheckResult)>,
    /// A5: username text of the in-flight `checkChatUsername`.
    pub username_check_pending: Option<String>,
    /// `bio_length_max` option (TDLib default 70, Premium raises it):
    /// the edit-profile bio counter.
    pub bio_length_max: usize,
    /// A5: last profile-edit request failure
    /// (`setName`/`setBio`/`setUsername`/`checkChatUsername`/
    /// `reorderActiveUsernames`/`toggleUsernameIsActive`/
    /// `setProfilePhoto`/`deleteProfilePhoto`), shown in the
    /// edit-profile dialog. Cleared when the dialog opens.
    pub profile_edit_error: Option<String>,
    /// `getDefaultBackgroundCustomEmojiStickers` answer: the custom emoji
    /// the color editor offers as reply and profile icons.
    pub background_emoji_icons: Vec<StickerItem>,
    /// The icon request failed; the editor shows a retry hint.
    pub background_emoji_icons_failed: bool,
    /// `getDefaultProfilePhotoCustomEmojiStickers` answer: the emoji the
    /// avatar builder offers.
    pub profile_photo_emoji_icons: Vec<StickerItem>,
    /// The builder request failed.
    pub profile_photo_emoji_icons_failed: bool,
    /// `getDefaultChatPhotoCustomEmojiStickers` answer: the emoji the
    /// builder offers for a group or channel photo.
    pub chat_photo_emoji_icons: Vec<StickerItem>,
    pub chat_photo_emoji_icons_failed: bool,
    /// Gifts with wearable colors (`getReceivedGifts` with
    /// `exclude_without_colors`), for the color editor.
    pub wearable_gifts: Vec<crate::premium_hub::ReceivedGift>,
    /// The wearable gifts request answered or failed.
    pub wearable_gifts_loaded: bool,
    /// Phase B1: secret-chat records keyed by `secret_chat_id`
    /// (`updateSecretChat` / `getSecretChat` answers). Kept at the
    /// session level because `updateSecretChat` is guaranteed to arrive
    /// *before* the chat identifier is returned (schema 1.8.67, line
    /// 10740), so a secret chat's chat summary may not exist yet when
    /// its state does. The full record (including `key_hash`) is kept
    /// for the B2 key-verification UI.
    pub secret_chat_states: HashMap<i32, ParsedSecretChat>,
    /// Phase B1: secret_chat_ids whose state still needs a `getSecretChat`
    /// fetch (e.g. a secret chat loaded from the local DB with no state
    /// seen yet). Drained by the driver's `maybe_fetch_secret_chat_states`.
    pub secret_chat_fetch_queue: Vec<i32>,
    /// A user whose private chat the driver opens on the next ingest:
    /// the `getSupportUser` answer (Settings > Ask a Question) or a
    /// contact just created by phone (New Contact).
    pub user_chat_to_open: Option<i64>,
    /// The answer to the New Contact box's `importContacts`, taken by the
    /// box: tdesktop's `AddContactBox::save` opens the chat when the phone
    /// belongs to a Telegram user and otherwise shows "not joined".
    pub new_contact_outcome: Option<NewContactOutcome>,
    /// Phase 6: the open user / supergroup info panel, if any.
    pub open_info_panel: Option<InfoPanelTarget>,
    /// The QR box's backdrops, contact link and free-code answers.
    pub peer_qr: crate::peer_qr::session::PeerQrData,
}

/// How the New Contact box's import ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewContactOutcome {
    /// The phone belongs to this Telegram user; the chat opens.
    Joined(i64),
    /// No Telegram account has the phone.
    NotJoined,
    /// The request failed (TDLib error code).
    Failed(i32),
}

impl UsersState {
    /// Fold the options this domain reads (`bio_length_max`).
    pub(crate) fn apply_option(
        &mut self,
        name: &str,
        value: &crate::telegram::envelope::OptionValue,
    ) {
        if name == "bio_length_max"
            && let crate::telegram::envelope::OptionValue::Integer(limit) = value
            && let Ok(limit) = usize::try_from(*limit)
            && limit > 0
        {
            self.bio_length_max = limit;
        }
        if name == "replies_bot_chat_id"
            && let crate::telegram::envelope::OptionValue::Integer(id) = value
            && *id > 0
        {
            self.replies_bot_chat_id = Some(*id);
        }
    }

    pub(crate) fn new() -> Self {
        Self {
            profile_chat_lists: HashMap::new(),
            common_groups_paging: HashMap::new(),
            similar_chat_totals: HashMap::new(),
            user_profile_photos: HashMap::new(),
            chat_current_photos: HashMap::new(),
            chat_photo_history: HashMap::new(),
            contacts: None,
            contacts_error: false,
            contacts_notice: None,
            replies_block: None,
            replies_bot_chat_id: None,
            user_full_infos: HashMap::new(),
            userpic_full_info_asked: Default::default(),
            username_check: None,
            username_check_pending: None,
            bio_length_max: 70,
            profile_edit_error: None,
            background_emoji_icons: Vec::new(),
            background_emoji_icons_failed: false,
            profile_photo_emoji_icons: Vec::new(),
            profile_photo_emoji_icons_failed: false,
            chat_photo_emoji_icons: Vec::new(),
            chat_photo_emoji_icons_failed: false,
            wearable_gifts: Vec::new(),
            wearable_gifts_loaded: false,
            secret_chat_states: HashMap::new(),
            secret_chat_fetch_queue: Vec::new(),
            user_chat_to_open: None,
            new_contact_outcome: None,
            open_info_panel: None,
            peer_qr: crate::peer_qr::session::PeerQrData::default(),
        }
    }
}

#[cfg(test)]
mod bio_limit_tests {
    use super::UsersState;
    use crate::telegram::envelope::OptionValue;

    #[test]
    fn bio_limit_follows_the_option() {
        let mut users = UsersState::new();
        assert_eq!(users.bio_length_max, 70);
        users.apply_option("bio_length_max", &OptionValue::Integer(140));
        assert_eq!(users.bio_length_max, 140);
        users.apply_option("bio_length_max", &OptionValue::Integer(0));
        users.apply_option("other", &OptionValue::Integer(5));
        assert_eq!(users.bio_length_max, 140);
    }
}
