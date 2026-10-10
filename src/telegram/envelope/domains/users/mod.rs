//! TDLib updates and answers for users, contacts, profiles and secret chats.
mod parse;

use crate::ids::UserId;
use crate::telegram::envelope::*;
pub(crate) use parse::parse_users_payload;

/// Payloads for users, contacts, profiles and secret chats; wrapped as
/// [`EnvelopePayload::Users`].
#[derive(Debug, Clone, PartialEq)]
pub enum UsersPayload {
    /// `updateUser` — Phase 6 keeps the full parsed user (contacts list,
    /// user info panel) in `Session::users`; the bot bit still drives the
    /// private-chat draft gate.
    UpdateUser { user_id: UserId, user: ParsedUser },
    /// `updateUserStatus` (schema 1.8.67, line 10729) — online / last-seen
    /// for a known user; refreshes the contacts list row.
    UpdateUserStatus {
        user_id: UserId,
        status: UserStatusKind,
    },
    /// `users` — `getContacts` response (schema 1.8.67, line 2471). Only
    /// the ids are authoritative here; the user objects themselves arrive
    /// via `updateUser`. `total_count` is dropped.
    Users { user_ids: Vec<i64> },
    /// Phase B1: `updateSecretChat` (schema 1.8.67, line 10741) — the
    /// secret chat's state changed. Guaranteed to arrive before the
    /// chat identifier is returned (i.e. before `updateNewChat`).
    UpdateSecretChat { secret_chat: ParsedSecretChat },
    /// Phase B1: `secretChat` (schema 1.8.67, line 2816) — the
    /// `getSecretChat` answer.
    SecretChat { secret_chat: ParsedSecretChat },
    /// Slice A6: `importedContacts` (schema 1.8.67, line 14517) — the
    /// `importContacts` answer. Only the user ids are kept; the
    /// reducer treats it like the `ok` of the other contact
    /// mutations (invalidate + notice).
    ImportedContacts { user_ids: Vec<i64> },
    /// A5: `checkChatUsername` answer (schema 1.8.67, lines 8583–8598).
    /// Correlated by `@extra` in `ConnectDriver` before `apply` takes the
    /// pending request (the response carries no chat id).
    CheckChatUsernameResult(UsernameCheckResult),
    /// `user` — `getMe` response; only the id is kept.
    Me { user_id: i64 },
    /// `userFullInfo` — `getUserFullInfo` response (schema 1.8.67,
    /// `getUserFullInfo user_id:int53 = UserFullInfo`, line 11501). The
    /// response carries no user id; it is resolved from the pending
    /// request in `Session::apply`, so only `bot_info` (from
    /// `userFullInfo.bot_info:botInfo`, line 2468), `bio` (from
    /// `userFullInfo.bio:formattedText`) and the preferred profile-photo
    /// file (from `userFullInfo.photo:chatPhoto` sizes) are kept.
    UserFullInfo {
        /// `userFullInfo.birthdate` and `group_in_common_count` (schema
        /// 1.8.67, line 2468) for the profile panel.
        extras: UserProfileExtras,
        bot_info: Option<BotInfo>,
        bio: String,
        /// Preferred size's `photo:file` from `chatPhoto.sizes`
        /// (schema 1.8.67, line 1030); `None` when the user has no photo.
        photo: Option<ParsedFile>,
        /// A5: `chatPhoto.id` (schema 1.8.67, line 1030) — the
        /// `profile_photo_id` for `deleteProfilePhoto`.
        photo_id: Option<i64>,
        /// Slice A6: `userFullInfo.block_list:BlockList` (schema 1.8.67,
        /// line 2468) — true when it is `blockListMain` (line 9692),
        /// parsed with the same `is_block_list_main` as `chat.block_list`.
        blocked: bool,
    },
    /// `updateUserFullInfo` — full info changed (schema 1.8.67, line 10744);
    /// the user id is explicit here.
    UpdateUserFullInfo {
        user_id: UserId,
        extras: UserProfileExtras,
        bot_info: Option<BotInfo>,
        bio: String,
        photo: Option<ParsedFile>,
        /// A5: `chatPhoto.id`, as above.
        photo_id: Option<i64>,
        /// Slice A6: `block_list` (schema 1.8.67, line 2468), same as
        /// `UserFullInfo::blocked`.
        blocked: bool,
    },
}
