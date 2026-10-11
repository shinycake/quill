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
    /// B10: profile photo galleries (`getUserProfilePhotos`) by user id.
    pub user_profile_photos: HashMap<i64, ProfilePhotosFetch>,
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
    /// Phase 6: cached `getUserFullInfo` bios, keyed by user id. Presence
    /// records "fetched" so the driver never refetches.
    pub user_full_infos: HashMap<i64, UserFullInfoData>,
    /// A5: latest `checkChatUsername` verdict for the edit-profile
    /// dialog: (checked username text, result). Written by the driver
    /// before `apply` takes the pending request; the dialog only shows it
    /// when it matches the current input text (stale verdicts ignored).
    pub username_check: Option<(String, UsernameCheckResult)>,
    /// A5: username text of the in-flight `checkChatUsername`.
    pub username_check_pending: Option<String>,
    /// A5: last profile-edit request failure
    /// (`setName`/`setBio`/`setUsername`/`checkChatUsername`/
    /// `reorderActiveUsernames`/`toggleUsernameIsActive`/
    /// `setProfilePhoto`/`deleteProfilePhoto`), shown in the
    /// edit-profile dialog. Cleared when the dialog opens.
    pub profile_edit_error: Option<String>,
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
    /// `getSupportUser` answer waiting for the driver to open the chat
    /// (Settings > Ask a Question).
    pub support_user_ready: Option<i64>,
    /// Phase 6: the open user / supergroup info panel, if any.
    pub open_info_panel: Option<InfoPanelTarget>,
}

impl UsersState {
    pub(crate) fn new() -> Self {
        Self {
            profile_chat_lists: HashMap::new(),
            user_profile_photos: HashMap::new(),
            contacts: None,
            contacts_error: false,
            contacts_notice: None,
            user_full_infos: HashMap::new(),
            username_check: None,
            username_check_pending: None,
            profile_edit_error: None,
            secret_chat_states: HashMap::new(),
            secret_chat_fetch_queue: Vec::new(),
            support_user_ready: None,
            open_info_panel: None,
        }
    }
}
