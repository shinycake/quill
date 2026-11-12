//! Users: contacts, profiles, profile photos, secret chats and the info panel: the `users` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::fetch::Fetch;
use crate::state::*;

/// The verdict of a `checkChatUsername`: the checked text and what the
/// server said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileUsernameVerdict {
    pub text: String,
    pub result: UsernameCheckResult,
}

/// Where the edit-profile dialog's username check stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProfileUsernameCheck {
    /// Nothing asked since the dialog opened.
    #[default]
    Idle,
    /// A check of `text` is out as `request`; `last` is the verdict before
    /// it (it still stands for its own text).
    Checking {
        request: RequestId,
        text: String,
        last: Option<ProfileUsernameVerdict>,
    },
    /// The latest check answered.
    Checked(ProfileUsernameVerdict),
}

impl ProfileUsernameCheck {
    /// The newest verdict held.
    pub fn verdict(&self) -> Option<&ProfileUsernameVerdict> {
        match self {
            Self::Idle => None,
            Self::Checking { last, .. } => last.as_ref(),
            Self::Checked(verdict) => Some(verdict),
        }
    }

    /// The verdict for exactly `text`.
    pub fn verdict_for(&self, text: &str) -> Option<UsernameCheckResult> {
        self.verdict()
            .filter(|verdict| verdict.text == text)
            .map(|verdict| verdict.result)
    }

    /// Whether a check of exactly `text` is out.
    pub fn is_checking(&self, text: &str) -> bool {
        matches!(self, Self::Checking { text: asked, .. } if asked == text)
    }

    /// The request in flight.
    pub fn pending(&self) -> Option<RequestId> {
        match self {
            Self::Checking { request, .. } => Some(*request),
            _ => None,
        }
    }

    /// A check of `text` goes out as `request`.
    pub fn begin(&mut self, request: RequestId, text: &str) {
        let last = std::mem::take(self).into_verdict();
        *self = Self::Checking {
            request,
            text: text.to_string(),
            last,
        };
    }

    /// The check that never left, or failed: back to the verdict held
    /// before it. Only the request in flight counts (`request`); whether it
    /// was.
    pub fn end(&mut self, request: RequestId) -> bool {
        if self.pending() != Some(request) {
            return false;
        }
        *self = match std::mem::take(self).into_verdict() {
            Some(verdict) => Self::Checked(verdict),
            None => Self::Idle,
        };
        true
    }

    /// The answer to `request`: the text it asked and the verdict, when it
    /// is the check in flight.
    pub fn answer(&mut self, request: RequestId, result: UsernameCheckResult) -> bool {
        let Self::Checking {
            request: current,
            text,
            ..
        } = self
        else {
            return false;
        };
        if *current != request {
            return false;
        }
        *self = Self::Checked(ProfileUsernameVerdict {
            text: std::mem::take(text),
            result,
        });
        true
    }

    fn into_verdict(self) -> Option<ProfileUsernameVerdict> {
        match self {
            Self::Idle => None,
            Self::Checking { last, .. } => last,
            Self::Checked(verdict) => Some(verdict),
        }
    }
}

pub struct UsersState {
    /// The requests the profile boxes wait on before they close.
    pub saves: SaveWatch,
    /// B10: chat-id lists behind the profile panels, keyed by
    /// `(kind, user/chat id)`: groups in common, similar channels and
    /// the suitable personal channels. Chat objects themselves arrive via
    /// `updateNewChat` before the `chats` answer.
    pub profile_chat_lists: HashMap<(ProfileChatsKind, i64), ProfileChatsFetch>,
    /// Paging of the groups in common list, by user id.
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
    /// Phase 6: `getContacts` result — user ids, in server order. Idle
    /// until asked (and again after a contact change invalidates it); a
    /// failed fetch is `Failed` so the UI can offer a retry.
    pub contacts: crate::state::fetch::Fetch<Vec<i64>>,
    /// Slice A6: outcome line for contacts mutations (`removeContacts`,
    /// `importContacts`, `clearImportedContacts`) — set on ok and on
    /// error, cleared by the next mutation; shown in the contacts
    /// settings section.
    pub contacts_notice: Option<String>,
    /// The open "Block {name}" box for a reply in the Replies chat.
    pub replies_block: Option<crate::message_menu_origin::RepliesBlock>,
    /// Short names of senders the server blocked from the Replies chat,
    /// waiting for the screen's "blocked" note (drained once).
    pub replies_blocked: Vec<String>,
    /// The option `replies_bot_chat_id`: the Replies chat (it differs on
    /// the test environment).
    pub replies_bot_chat_id: Option<i64>,
    /// Phase 6: cached `getUserFullInfo` bios, one `Fetch` per user.
    /// Presence records "fetched" so the driver never refetches; a failed
    /// ask is kept so `ensure_user_full_info` (the ask a render makes) does
    /// not repeat it; an explicit `fetch_user_full_info` does.
    pub user_full_infos: UserFullInfos,
    /// Where each user's ask for the clips of their video userpic stands
    /// (`Session::userpic_full_info_due`): one entry per user, replaced
    /// when their photo changes, so it is as large as the users known.
    pub userpic_full_info: std::collections::HashMap<i64, super::userpic_video::UserpicInfoAsk>,
    /// A5: the `checkChatUsername` of the edit-profile dialog: the
    /// request in flight with the text it checks, and the latest verdict.
    /// The dialog shows a verdict only when it matches the current input
    /// text (stale verdicts ignored), and an answer or error only counts
    /// for the request in flight, not for an earlier check's reply.
    pub username_check: ProfileUsernameCheck,
    /// The `getMe` fallback for the own user id (the `my_id` option normally
    /// brings it first): asked once per session, never again after a failure (it does not
    /// matter to the id itself: the `my_id` option may still set it).
    pub my_id_probe: Fetch<()>,
    /// `bio_length_max` option (TDLib default 70, Premium raises it):
    /// the edit-profile bio counter.
    pub bio_length_max: usize,
    /// `getDefaultBackgroundCustomEmojiStickers` answer: the custom emoji
    /// the color editor offers as reply and profile icons.
    pub background_emoji_icons: crate::state::fetch::Fetch<Vec<StickerItem>>,
    /// `getDefaultProfilePhotoCustomEmojiStickers` answer: the emoji the
    /// avatar builder offers.
    pub profile_photo_emoji_icons: crate::state::fetch::Fetch<Vec<StickerItem>>,
    /// `getDefaultChatPhotoCustomEmojiStickers` answer: the emoji the
    /// builder offers for a group or channel photo.
    pub chat_photo_emoji_icons: crate::state::fetch::Fetch<Vec<StickerItem>>,
    /// Gifts with wearable colors (`getReceivedGifts` with
    /// `exclude_without_colors`), for the color editor.
    pub wearable_gifts: crate::state::fetch::Fetch<Vec<crate::premium_hub::ReceivedGift>>,
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
    /// The New Contact box's `importContacts`: the request it waits for,
    /// then its answer (taken by the box), keyed by the import it answers
    /// so only the box that sent it applies it. Closing the box forgets
    /// it: the answer then opens no chat, and the next box's Create is sent
    /// instead of waiting for it.
    pub new_contact: NewContact,
    /// Phase 6: the open user / supergroup info panel, if any.
    pub open_info_panel: Option<InfoPanelTarget>,
    /// The QR box's backdrops, contact link and free-code answers.
    pub peer_qr: crate::peer_qr::session::PeerQrData,
}

/// The New Contact box's import, from Create to its taken answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NewContact {
    /// No box waits for an import.
    #[default]
    Closed,
    /// `importContacts` `RequestId` is out.
    Sending(RequestId),
    /// The phone belongs to this Telegram user: the driver opens the chat,
    /// then the box is told (`Done` with `Joined`). Closing the box ends it.
    Opening(RequestId, i64),
    /// The import answered; the box takes it. tdesktop's
    /// `AddContactBox::save` opens the chat when the phone belongs to a
    /// Telegram user and otherwise shows "not joined".
    Done(RequestId, NewContactOutcome),
}

impl NewContact {
    /// The chat to open for the joined phone, taken (the stage moves to
    /// `Done` with the outcome).
    pub fn take_opening(&mut self) -> Option<i64> {
        let Self::Opening(request, user) = *self else {
            return None;
        };
        *self = Self::Done(request, NewContactOutcome::Joined(user));
        Some(user)
    }

    /// The import in flight.
    pub fn sending(&self) -> Option<RequestId> {
        match self {
            Self::Sending(request) => Some(*request),
            _ => None,
        }
    }

    /// The answer waiting to be taken.
    pub fn outcome(&self) -> Option<&NewContactOutcome> {
        match self {
            Self::Done(_, outcome) => Some(outcome),
            _ => None,
        }
    }

    /// Take the answer; the stage is closed.
    pub fn take_outcome(&mut self) -> Option<(RequestId, NewContactOutcome)> {
        match std::mem::take(self) {
            Self::Done(request, outcome) => Some((request, outcome)),
            other => {
                *self = other;
                None
            }
        }
    }
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
    /// The reply and profile icons the color editor offers.
    pub fn background_icons(&self) -> &[StickerItem] {
        self.background_emoji_icons
            .data()
            .map_or(&[], Vec::as_slice)
    }

    /// The emoji the avatar builder offers.
    pub fn profile_photo_icons(&self) -> &[StickerItem] {
        self.profile_photo_emoji_icons
            .data()
            .map_or(&[], Vec::as_slice)
    }

    /// The emoji the group or channel photo builder offers.
    pub fn chat_photo_icons(&self) -> &[StickerItem] {
        self.chat_photo_emoji_icons
            .data()
            .map_or(&[], Vec::as_slice)
    }

    /// The gifts whose colors can be worn.
    pub fn wearable_gift_list(&self) -> &[crate::premium_hub::ReceivedGift] {
        self.wearable_gifts.data().map_or(&[], Vec::as_slice)
    }

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
            saves: SaveWatch::default(),
            profile_chat_lists: HashMap::new(),
            similar_chat_totals: HashMap::new(),
            user_profile_photos: HashMap::new(),
            chat_current_photos: HashMap::new(),
            chat_photo_history: HashMap::new(),
            contacts: crate::state::fetch::Fetch::Idle,
            contacts_notice: None,
            replies_block: None,
            replies_blocked: Vec::new(),
            replies_bot_chat_id: None,
            user_full_infos: UserFullInfos::default(),
            userpic_full_info: Default::default(),
            username_check: ProfileUsernameCheck::Idle,
            my_id_probe: Fetch::Idle,
            bio_length_max: 70,
            background_emoji_icons: Default::default(),
            profile_photo_emoji_icons: Default::default(),
            chat_photo_emoji_icons: Default::default(),
            wearable_gifts: Default::default(),
            secret_chat_states: HashMap::new(),
            secret_chat_fetch_queue: Vec::new(),
            new_contact: NewContact::Closed,
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

#[cfg(test)]
mod username_check_tests {
    use super::*;

    #[test]
    fn only_the_check_in_flight_is_answered_or_ended() {
        let mut check = ProfileUsernameCheck::Idle;
        check.begin(RequestId(1), "old");
        check.begin(RequestId(2), "new");
        assert!(!check.answer(RequestId(1), UsernameCheckResult::Occupied));
        assert!(!check.end(RequestId(1)));
        assert!(check.is_checking("new"));
        assert!(check.answer(RequestId(2), UsernameCheckResult::Available));
        assert_eq!(
            check.verdict_for("new"),
            Some(UsernameCheckResult::Available)
        );
        assert_eq!(check.verdict_for("old"), None);
    }

    #[test]
    fn a_failed_check_gives_back_the_verdict_held_before_it() {
        let mut check = ProfileUsernameCheck::Idle;
        check.begin(RequestId(1), "a");
        check.answer(RequestId(1), UsernameCheckResult::Occupied);
        check.begin(RequestId(2), "b");
        assert_eq!(
            check.verdict_for("a"),
            Some(UsernameCheckResult::Occupied),
            "the earlier text keeps its verdict while another is checked"
        );
        assert!(check.end(RequestId(2)));
        assert_eq!(check.pending(), None);
        assert_eq!(check.verdict_for("a"), Some(UsernameCheckResult::Occupied));
        let mut first = ProfileUsernameCheck::Idle;
        first.begin(RequestId(3), "c");
        assert!(first.end(RequestId(3)));
        assert_eq!(first, ProfileUsernameCheck::Idle);
    }
}
