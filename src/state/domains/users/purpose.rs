//! Request purposes for users, contacts, profiles and secret chats.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for users, contacts, profiles and secret chats; wrapped as
/// [`RequestPurpose::Users`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsersPurpose {
    /// The info panel's media counts: `getChatMessageCount` with the
    /// filter at index `filter` of `MEDIA_COUNT_FILTERS`.
    GetChatMessageCount { filter: u8 },
    /// "Save to... Profile" on a song: `addProfileAudio`.
    AddProfileAudio,
    /// `getMe`. Response is `user`; only the id is kept.
    GetMe,
    /// `getUserFullInfo` for a bot user. Response is `userFullInfo`; the
    /// user id is resolved from the request's chat (`ChatKind::Private`).
    GetUserFullInfo,
    /// Phase 6: `getContacts`. Response is `users`; the user ids land in
    /// `Session::contacts`, the user objects via `updateUser`.
    GetContacts,
    /// Phase 6: `addContact`. Response is `ok`; the contact row refreshes
    /// via `updateUser` (and the contacts list is invalidated for refetch).
    AddContact,
    /// A5: `setName` (schema 1.8.67, line 14823). Response is `ok`; the
    /// new name arrives via `updateUser`.
    SetName,
    /// A5: `setBio` (schema 1.8.67, line 14826). Response is `ok`; the
    /// new bio arrives via `updateUserFullInfo`.
    SetBio,
    /// A5: `setUsername` (schema 1.8.67, line 14830). Response is `ok`;
    /// the new usernames arrive via `updateUser`.
    SetUsername,
    /// A5: `checkChatUsername` with the private chat with self (schema
    /// 1.8.67, line 11677) — the documented availability check for the
    /// current user's own username (TGX `EditUsernameController` sends it
    /// with `tdlib.selfChatId()`). Response is `checkChatUsernameResult*`.
    CheckUsername,
    /// A5: `reorderActiveUsernames` (schema 1.8.67, line 14838).
    /// Response is `ok`; the new order arrives via `updateUser`.
    ReorderActiveUsernames,
    /// A5: `toggleUsernameIsActive` (schema 1.8.67, line 14835).
    /// Response is `ok`; the new lists arrive via `updateUser`.
    ToggleUsernameIsActive,
    /// A5: `setProfilePhoto` (schema 1.8.67, line 14803). Response is
    /// `ok`; the new photo arrives via `updateUserFullInfo`.
    SetProfilePhoto,
    /// A5: `deleteProfilePhoto` (schema 1.8.67, line 14806). Response is
    /// `ok`; the removal arrives via `updateUserFullInfo`.
    DeleteProfilePhoto,
    /// Slice A12: `setProfileAccentColor` (schema 1.8.67, line 14820).
    /// Response is `ok`; the new color arrives via `updateUser` on our
    /// own user (`profile_accent_color_id`).
    SetProfileAccentColor,
    /// Slice A6: `removeContacts`. Response is `ok`; the contacts list is
    /// invalidated for refetch (same as `AddContact`).
    RemoveContact,
    /// Slice A6: `importContacts`. Response is `importedContacts`; the
    /// contacts list is invalidated for refetch.
    ImportContacts,
    /// Slice A6: `clearImportedContacts`. Response is `ok`; the contacts
    /// list is invalidated for refetch.
    ClearImportedContacts,
    /// Batch 8: `sharePhoneNumber` (the bar's "Share my phone number").
    /// Response is `ok`; TDLib then clears the bar.
    SharePhoneNumber,
    /// Slice CL3: `setMessageSenderBlockList` (schema 1.8.67, line
    /// 14492). Response is `ok`; the new state arrives via
    /// `updateChatBlockList`. Slice A6: carries the requested `block`
    /// value so the `ok` arm can update the cached
    /// `UserFullInfoData.blocked` authoritatively (the `ok` response
    /// itself carries no state).
    SetMessageSenderBlockList { block: bool },
    /// B10: `setBirthdate` (schema 1.8.67, line 14841). Response is
    /// `ok`; the new value arrives via `updateUserFullInfo`.
    SetBirthdate,
    /// B10: `setPersonalChat` (line 14847). Response is `ok`.
    SetPersonalChat,
    /// B10: `setUserNote` (line 14553). Response is `ok`.
    SetUserNote,
    /// `setUserPersonalProfilePhoto` / `suggestUserProfilePhoto`
    /// (lines 14942, 14952). Response is `ok`; pending `user_id`.
    SetUserPersonalPhoto,
    /// `reportChatPhoto` (line 16107). Response is `ok`.
    ReportChatPhoto,
    /// B10: `getUserProfilePhotos` (line 14591) for the profile photo
    /// gallery; pending `user_id`. Response is `chatPhotos`.
    GetUserProfilePhotos,
    /// Phase B1: `createNewSecretChat`. Response is `chat` (the new
    /// secret chat); the canonical state arrives as `updateNewChat` /
    /// `updateSecretChat`.
    CreateNewSecretChat,
    /// Phase B1: `getSecretChat`. Response is `secretChat`; resolves the
    /// initial state of a secret chat whose `updateSecretChat` was never
    /// seen (e.g. loaded from the local DB). Correlated via
    /// `PendingRequest::secret_chat_id`.
    GetSecretChat,
    /// Phase B1: `closeSecretChat`. Response is `ok`; the state change to
    /// `secretChatStateClosed` arrives as `updateSecretChat`.
    CloseSecretChat,
}

flat_purposes!(Users(UsersPurpose) {
    AddProfileAudio,
    GetMe,
    GetUserFullInfo,
    GetContacts,
    AddContact,
    SetName,
    SetBio,
    SetUsername,
    CheckUsername,
    ReorderActiveUsernames,
    ToggleUsernameIsActive,
    SetProfilePhoto,
    DeleteProfilePhoto,
    SetProfileAccentColor,
    RemoveContact,
    ImportContacts,
    ClearImportedContacts,
    SharePhoneNumber,
    SetBirthdate,
    SetPersonalChat,
    SetUserNote,
    SetUserPersonalPhoto,
    ReportChatPhoto,
    GetUserProfilePhotos,
    CreateNewSecretChat,
    GetSecretChat,
    CloseSecretChat,
});
