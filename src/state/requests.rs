//! Request plumbing: errors, registry, forwarding and payment flights.
use super::*;
use std::time::{Duration, Instant};

/// How long a sweepable request may stay unanswered before the periodic
/// sweep drops it (R5). TDLib answers every request (even offline ones
/// wait for the network), so a minute with a ready connection means the
/// answer is lost.
pub const PENDING_REQUEST_TTL: Duration = Duration::from_secs(60);

/// Slice A2: which 2FA management request a `PasswordStateOp` purpose
/// carries. Used for honest per-action error lines and for the driver's
/// single-in-flight gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordOp {
    /// `getPasswordState` — initial / refresh fetch.
    Fetch,
    /// `setPassword` with a new password (empty old = first-time enable).
    SetPassword,
    /// `setPassword` with an empty new password (disable).
    DisablePassword,
    /// `setRecoveryEmailAddress`.
    SetRecoveryEmail,
    /// `resendRecoveryEmailAddressCode`.
    ResendCode,
    /// `cancelRecoveryEmailAddressVerification`.
    AbortEmailSetup,
    /// Batch 6: `checkRecoveryEmailAddressCode` (confirm the pending
    /// recovery email with the emailed code).
    CheckEmailCode,
    /// Batch 6: `requestPasswordRecovery` ("Forgot password?"). Answers
    /// `emailAddressAuthenticationCodeInfo`.
    RequestRecoveryCode,
    /// Batch 6: `recoverPassword` with the emailed recovery code.
    RecoverPassword,
    /// Batch 6: `resetPassword` (7-day wait). Answers
    /// `resetPasswordResult*`.
    ResetPassword,
    /// Batch 6: `cancelPasswordReset`. Answers `ok`.
    CancelPasswordReset,
    /// Batch 6: `setLoginEmailAddress` / `resendLoginEmailAddressCode`.
    /// Answer `emailAddressAuthenticationCodeInfo`.
    SetLoginEmail,
    /// Batch 6: `checkLoginEmailAddressCode`. Answers `ok`.
    CheckLoginEmailCode,
}

impl PasswordOp {
    /// Past-tense action label for honest error lines, e.g.
    /// "Could not change the two-step password (error 400)".
    pub(crate) fn action_label(self) -> &'static str {
        match self {
            PasswordOp::Fetch => crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_FETCH),
            PasswordOp::SetPassword => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_SET_PASSWORD)
            }
            PasswordOp::DisablePassword => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_DISABLE_PASSWORD)
            }
            PasswordOp::SetRecoveryEmail => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_SET_RECOVERY_EMAIL)
            }
            PasswordOp::ResendCode => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_RESEND_CODE)
            }
            PasswordOp::AbortEmailSetup => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_ABORT_EMAIL_SETUP)
            }
            PasswordOp::CheckEmailCode => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_CHECK_EMAIL_CODE)
            }
            PasswordOp::RequestRecoveryCode => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_REQUEST_RECOVERY_CODE)
            }
            PasswordOp::RecoverPassword => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_RECOVER_PASSWORD)
            }
            PasswordOp::ResetPassword => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_RESET_PASSWORD)
            }
            PasswordOp::CancelPasswordReset => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_CANCEL_PASSWORD_RESET)
            }
            PasswordOp::SetLoginEmail => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_SET_LOGIN_EMAIL)
            }
            PasswordOp::CheckLoginEmailCode => {
                crate::lang::tr(crate::lang::keys::Q_OPS_ERR_ACTION_CHECK_LOGIN_EMAIL_CODE)
            }
        }
    }
}

impl RequestPurpose {
    /// Mutations that only guard local UI state and are dropped by the sweep
    /// when TDLib never answers. Unlike [`Self::is_sweepable`] lookups they
    /// are user actions, so their errors still surface. Your own group-call
    /// mute toggle is the one: while it is pending, own-row updates are held
    /// back from `is_muted_self`, which would otherwise stay frozen forever.
    pub fn is_swept_without_error(self) -> bool {
        matches!(
            self,
            RequestPurpose::Calls(CallsPurpose::ToggleGroupCallParticipantMute { .. })
        )
    }

    /// R5: whether an unanswered request of this purpose may be dropped by
    /// the periodic sweep. Only dedupe-guarded lookups (history pages,
    /// searches, shared media, per-entity info) qualify: dropping them
    /// frees the guard so the next attempt can go out. Downloads, uploads,
    /// calls, auth and every mutation answer on their own schedule (or are
    /// meaningful whenever they land), so they are never swept.
    pub fn is_sweepable(self) -> bool {
        matches!(
            self,
            RequestPurpose::GetHistory
                | RequestPurpose::GetHistoryAround
                | RequestPurpose::GetHistoryNewer
                | RequestPurpose::GetTopicHistory
                | RequestPurpose::GetMessageThreadHistory { .. }
                | RequestPurpose::SearchMessages
                | RequestPurpose::SearchChatMessages
                | RequestPurpose::SearchChatMessagesMore
                | RequestPurpose::SearchChats
                | RequestPurpose::SearchPublicChats
                | RequestPurpose::GetSharedMedia { .. }
                | RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { .. })
                | RequestPurpose::GetPinnedMessages
                | RequestPurpose::GetUserFullInfo
                | RequestPurpose::GetSupergroupFullInfo
                | RequestPurpose::GetBasicGroupFullInfo
                | RequestPurpose::GetChatMember
        )
    }
}

/// The code of the error a lost request settles with: HTTP's "Request
/// Timeout". It never came from TDLib; the domain handlers only read the
/// class (`Other`), so every purpose settles as an ordinary failed lookup.
const LOST_REQUEST_ERROR_CODE: i32 = 408;

impl Session {
    /// Settle a request that will never be answered (swept after its TTL,
    /// dropped because the flood-retry re-send failed, or abandoned with its
    /// account) by feeding it through the same per-domain failure handling a
    /// TDLib error uses. Every purpose settles in this one place: spinners
    /// and "loading more" flags clear, the history window offers Retry.
    /// The request must already be out of the registry.
    pub(crate) fn settle_lost_request(&mut self, pending: PendingRequest) {
        let extra = pending.id;
        let seq = self.last_seq;
        let error = TdError::from_code(LOST_REQUEST_ERROR_CODE);
        self.apply_payload_in_asking_view(
            EnvelopePayload::Error(error),
            Some(&pending),
            Some(extra),
            seq,
        );
    }

    /// The account is going away (logout, close): no answer can land any
    /// more, so settle every pending lookup the way a failure would and
    /// forget the stale-window marks. User actions (sends, edits, ...) are
    /// dropped without a failure notice about a session the user left.
    pub(crate) fn settle_account_requests(&mut self) {
        for pending in self.requests.drain_all() {
            if !super::session_apply_error::is_user_action(pending.purpose) {
                self.settle_lost_request(pending);
            }
        }
        self.stale_history_requests.clear();
    }
}

pub fn is_auth_submit(purpose: RequestPurpose) -> bool {
    matches!(
        purpose,
        RequestPurpose::SetPhoneNumber
            | RequestPurpose::RegisterUser
            | RequestPurpose::SetAuthenticationEmail
            | RequestPurpose::CheckAuthenticationEmailCode
            | RequestPurpose::CheckAuthenticationCode
            | RequestPurpose::CheckAuthenticationPassword
            | RequestPurpose::ResendAuthenticationCode
            | RequestPurpose::RequestAuthenticationPasswordRecovery
            | RequestPurpose::RecoverAuthenticationPassword
            | RequestPurpose::RequestQrCodeAuthentication
            | RequestPurpose::ResetAuthenticationEmail
    )
}

/// Classified TDLib error for an auth submit. Never includes the native message
/// (codes, passwords, and phone numbers live there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthRequestError {
    pub purpose: RequestPurpose,
    pub class: ErrorClass,
    /// Slice parity:platform-flood-errors — retry-after seconds from
    /// `TdError::flood_wait_secs`, so auth flood errors get the same
    /// countdown as other surfaces.
    pub flood_wait_secs: Option<u64>,
}

impl AuthRequestError {
    /// Slice parity:platform-flood-errors — the user-facing line. Flood
    /// errors with a known wait show the real countdown ("… try again in
    /// N seconds"); everything else is the static classified line.
    pub fn user_message(self) -> String {
        let base = self.base_message();
        match (self.class, self.flood_wait_secs) {
            (ErrorClass::Flood, Some(secs)) => {
                let base = base.strip_suffix(" — wait and try again").unwrap_or(base);
                format!("{base} — try again in {secs} seconds")
            }
            _ => base.to_string(),
        }
    }

    fn base_message(self) -> &'static str {
        match (self.purpose, self.class) {
            // tdesktop `phoneSubmitFail` / `lng_bad_phone`,
            // `lng_error_phone_flood`; the banned number gets its own box.
            (_, ErrorClass::PhoneInvalid) => "Invalid phone number. Please try again.",
            (_, ErrorClass::PhoneBanned) => "This phone number is banned.",
            (_, ErrorClass::PhoneFlood) => {
                "You have deleted and re-created your account too many times recently. Please wait for a few days before signing up again."
            }
            (RequestPurpose::ResetAuthenticationEmail, ErrorClass::TaskAlreadyExists) => {
                "an email reset is already pending"
            }
            (RequestPurpose::ResetAuthenticationEmail, _) => "couldn't start the email reset",
            (RequestPurpose::RegisterUser, ErrorClass::Invalid) => {
                "registration not accepted — check your name"
            }
            (RequestPurpose::RegisterUser, ErrorClass::Flood) => {
                "too many registration attempts — wait and try again"
            }
            (RequestPurpose::SetPhoneNumber, ErrorClass::Invalid) => "phone not accepted",
            (RequestPurpose::SetPhoneNumber, ErrorClass::Flood) => {
                "too many phone attempts — wait and try again"
            }
            (RequestPurpose::SetAuthenticationEmail, ErrorClass::Invalid) => {
                "email address not accepted"
            }
            (RequestPurpose::SetAuthenticationEmail, ErrorClass::Flood) => {
                "too many email attempts — wait and try again"
            }
            (
                RequestPurpose::CheckAuthenticationCode
                | RequestPurpose::CheckAuthenticationEmailCode,
                ErrorClass::Invalid,
            ) => "code not accepted",
            (
                RequestPurpose::CheckAuthenticationCode
                | RequestPurpose::CheckAuthenticationEmailCode,
                ErrorClass::Flood,
            ) => "too many code attempts — wait and try again",
            (RequestPurpose::CheckAuthenticationPassword, ErrorClass::Invalid) => {
                "password not accepted"
            }
            (RequestPurpose::CheckAuthenticationPassword, ErrorClass::Flood) => {
                "too many password attempts — wait and try again"
            }
            (RequestPurpose::ResendAuthenticationCode, ErrorClass::Invalid) => {
                "couldn't resend the code"
            }
            // No invented local cooldown: a too-early resend fails
            // server-side (429), and this is the honest surface for it.
            (RequestPurpose::ResendAuthenticationCode, ErrorClass::Flood) => {
                "too many resends — wait and try again"
            }
            (RequestPurpose::RequestAuthenticationPasswordRecovery, ErrorClass::Invalid) => {
                "couldn't send the recovery code"
            }
            (RequestPurpose::RequestAuthenticationPasswordRecovery, ErrorClass::Flood) => {
                "too many recovery requests — wait and try again"
            }
            (RequestPurpose::RecoverAuthenticationPassword, ErrorClass::Invalid) => {
                "recovery code not accepted"
            }
            (RequestPurpose::RecoverAuthenticationPassword, ErrorClass::Flood) => {
                "too many recovery attempts — wait and try again"
            }
            (_, ErrorClass::Unauthorized) => "session is no longer authorized",
            _ => "Telegram rejected the request",
        }
    }
}

/// Source + dest frozen when `forwardMessages` is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardFlight {
    pub extra: RequestId,
    pub dest_chat_id: ChatId,
    pub from_chat_id: ChatId,
    pub requested: usize,
}

/// Result of `forwardMessages` (`messages` or `error`). Dest title is resolved
/// from the loaded chat list (tdesktop ShareBox success names the peer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardResult {
    pub dest_chat_id: ChatId,
    pub dest_title: String,
    pub from_chat_id: ChatId,
    pub requested: usize,
    pub forwarded_ids: Vec<MessageId>,
}

/// B1: an incoming message whose markup demands a reply
/// (`replyMarkupForceReply`, or `force_reply` on an inline / show-keyboard
/// markup). Drained by the UI into a composer reply-to + focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceReplyTarget {
    pub chat_id: ChatId,
    pub message_id: MessageId,
}

/// B1: context of an in-flight login-URL button press. Kept while
/// `getLoginUrlInfo` (and then `getLoginUrl`, after consent) is in flight,
/// so an error degrades the button to a plain URL-button press (schema
/// 1.8.67 doc on `getLoginUrl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginUrlRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub button_id: i64,
    /// The button's raw URL — the plain-URL fallback.
    pub raw_url: String,
    /// Pressed by a step the agent UI driver injected: what the answer
    /// opens runs as part of that step (`guard::with_injecting`).
    pub from_agent: bool,
}

/// Slice P1: context of an in-flight payment request (`getPaymentForm`,
/// `validateOrderInfo`, `sendPaymentForm`, `getPaymentReceipt`), kept so
/// the answers / errors correlate to the right invoice message. Set when a
/// payment request is sent (connect.rs), cleared when the checkout dialog
/// closes — the answer reducers never clear it, because `validateOrderInfo`
/// and `sendPaymentForm` need it after the form answer is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRequest {
    /// What is being paid: an invoice message, an invoice link or a
    /// payment toward Telegram. The same value goes into every request.
    pub invoice: crate::payment_invoice::InputInvoice,
}

/// B1: pick the custom keyboard to show for a chat — the newest message
/// carrying `replyMarkupShowKeyboard`, invalidated by a newer
/// `replyMarkupRemoveKeyboard` or by a one-time keyboard already tapped.
/// Pure logic: unit-tested.
pub fn active_custom_keyboard(
    messages: &std::collections::BTreeMap<i64, HistoryMessage>,
    dismissed: &std::collections::HashSet<(i64, i64)>,
) -> Option<(ChatId, MessageId, ReplyKeyboard)> {
    let mut active: Option<(ChatId, MessageId, ReplyKeyboard)> = None;
    for message in messages.values() {
        match &message.reply_markup {
            Some(ReplyMarkup::ShowKeyboard(keyboard)) => {
                active = Some((message.chat_id, message.id, keyboard.clone()));
            }
            Some(ReplyMarkup::RemoveKeyboard) => active = None,
            _ => {}
        }
    }
    active.filter(|(chat_id, message_id, _)| !dismissed.contains(&(chat_id.0, message_id.0)))
}

impl ForwardResult {
    /// The toast text: tdesktop's `lng_share_message(s)_to_chat` without its
    /// bold markers, or a failure line when nothing was forwarded.
    pub fn success_label(&self) -> String {
        use crate::lang::{keys, tr_with};
        let tags = [("chat", self.dest_title.as_ref())];
        let key = match self.forwarded_ids.len() {
            0 => return tr_with(keys::Q_FORWARD_FAILED, &tags),
            1 => keys::SHARE_MESSAGE_TO_CHAT,
            _ => keys::SHARE_MESSAGES_TO_CHAT,
        };
        tr_with(key, &tags).replace("**", "")
    }
}

/// Sponsored messages for one chat (`getChatSponsoredMessages` response).
/// Rows render with a **Sponsored** / **Recommended** label.
#[derive(Debug, Clone, Default)]
pub struct ChatSponsoredMessages {
    pub messages: Vec<SponsoredMessage>,
    /// Schema `messages_between`: minimum number of ordinary messages between
    /// shown sponsored rows (0 = after all ordinary messages).
    pub messages_between: i32,
    /// Ids already reported to TDLib as viewed (`viewMessages`): an ad is
    /// counted once per fetched list, however often it scrolls into view.
    pub viewed: std::collections::HashSet<i64>,
    /// Ids the user reported or hid; never shown again for this list.
    pub dismissed: std::collections::HashSet<i64>,
    /// When the response arrived (tdesktop `lastReceived`): refetches inside
    /// `SPONSORED_REFETCH_AFTER` are skipped.
    pub fetched_at: Option<std::time::Instant>,
}

/// tdesktop `kRequestTimeLimit`: a chat's sponsored messages are not
/// requested again for five minutes.
pub const SPONSORED_REFETCH_AFTER: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl ChatSponsoredMessages {
    /// Rows in TDLib's response order. The schema does not promise an order,
    /// so the vector order is preserved verbatim.
    pub fn ordered(&self) -> Vec<&SponsoredMessage> {
        self.messages.iter().collect()
    }
}

/// `reportChatSponsoredMessage` waiting on the user's option choice
/// (`reportSponsoredResultOptionRequired`).
#[derive(Debug, Clone)]
pub struct SponsoredReportFlight {
    pub extra: RequestId,
    pub chat_id: ChatId,
    /// int53 sponsored message id.
    pub message_id: i64,
    pub title: String,
    pub options: Vec<ReportOption>,
}

/// Last `reportChatSponsoredMessage` outcome (no TDLib text is echoed).
#[derive(Debug, Clone)]
pub struct SponsoredReportOutcome {
    pub chat_id: ChatId,
    /// int53 sponsored message id.
    pub message_id: i64,
    pub result: ReportSponsoredResult,
}

/// Slice B2: fetch state of `getBotSimilarBots` for a bot profile. The
/// `users` payload carries only ids (names resolve through
/// `Session::users`); presence records "fetched" so the driver never
/// retries, and `Loading` dedupes the request.
#[derive(Debug, Clone, Default)]
pub enum SimilarBotsFetch {
    #[default]
    Loading,
    Loaded(Vec<i64>),
}

impl SponsoredReportOutcome {
    pub fn user_message(&self) -> &'static str {
        self.result.user_message()
    }
}

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub id: RequestId,
    pub account_generation: AccountGeneration,
    pub purpose: RequestPurpose,
    pub chat_id: Option<ChatId>,
    pub view_generation: Option<ViewGeneration>,
    pub file_id: Option<i32>,
    pub search_generation: Option<u64>,
    pub around_message_id: Option<MessageId>,
    /// Phase 5.1: `forum_topic_id` for `GetTopicHistory` requests so the
    /// `foundChatMessages` response lands in the right topic history.
    pub forum_topic_id: Option<i32>,
    /// Phase 6: `user_id` for `GetUserFullInfo` (panel opened from the
    /// contacts list, where there is no chat) and `AddContact` requests so
    /// id-less responses (`userFullInfo`) land on the right user.
    pub user_id: Option<i64>,
    /// Phase 6: `supergroup_id` for `GetSupergroupFullInfo` requests so the
    /// id-less `supergroupFullInfo` response lands on the right group.
    pub supergroup_id: Option<i64>,
    /// Slice (communities backend core): `community_id` for
    /// `GetCommunityFullInfo` / `SetCommunityName` correlation and
    /// per-community in-flight dedupe.
    pub community_id: Option<i64>,
    /// Phase 9.1: `story_id` for `GetStory` requests so in-flight
    /// per-story dedupe distinguishes stories of the same chat.
    pub story_id: Option<i32>,
    /// Phase 9.7: `story_ids` for `SetChatPinnedStories` (the sent pinned
    /// story ids) and `ReorderStoryAlbums` (the sent album id order) so
    /// the id-less `ok` response applies to the right list.
    pub story_ids: Option<Vec<i32>>,
    /// Phase 9.7: `story_album_id` for `DeleteStoryAlbum` so the `ok`
    /// response drops the right album from `Session::story_albums`.
    pub story_album_id: Option<i32>,
    /// Parity slice: `folder_id` for folder-scoped requests
    /// (`GetChatFolder`, `EditChatFolder`, `DeleteChatFolder`,
    /// `LoadFolderChats`) so responses correlate to the folder.
    pub folder_id: Option<i32>,
    /// Parity slice: `scope` for `GetScopeNotificationSettings` /
    /// `SetScopeNotificationSettings` /
    /// `GetChatNotificationSettingsExceptions` so the id-less
    /// `scopeNotificationSettings` / `chats` response lands on the right
    /// scope.
    pub scope: Option<NotificationSettingsScope>,
    /// Phase B1: `secret_chat_id` for `GetSecretChat` /
    /// `CloseSecretChat` so the id-less `secretChat` response and
    /// purpose-gating correlate to the right secret chat.
    pub secret_chat_id: Option<i32>,
    /// Slice G1: rollback for requests the driver applies optimistically
    /// at send time (`setChatPermissions`,
    /// `toggleSupergroupJoinByRequest`, `setSupergroupUsername`). On a
    /// TDLib error the pre-request value is restored so the UI never
    /// keeps showing a change the server rejected.
    pub rollback: Option<RequestRollback>,
    /// The chat window whose view was in the session's place when the
    /// request was registered (`None`: the main window's). An answer that
    /// lands in a view applies to that window's
    /// (`Session::apply_payload_in_asking_view`).
    pub window: Option<ChatId>,
    /// When the request was registered. The periodic sweep
    /// ([`RequestRegistry::sweep_stale`]) drops dedupe-guard entries whose
    /// answer never came, so a lost response cannot block a chat forever.
    pub created_at: Instant,
}

#[derive(Debug, Default)]
pub struct RequestRegistry {
    pub(crate) next: u64,
    pub(crate) pending: HashMap<u64, PendingRequest>,
    /// The chat window whose view is in the session's place (`None`: the
    /// main window's). `Session::enter_view` keeps it; every request
    /// registered meanwhile carries it (`PendingRequest::window`).
    pub(crate) window: Option<ChatId>,
}

impl RequestRegistry {
    /// Phase B1: whether a request with the given purpose is already in
    /// flight for this secret chat id. Used to avoid duplicate offline
    /// `getSecretChat` fetches while the state resolves.
    pub fn has_pending_for_secret_chat(
        &self,
        purpose: RequestPurpose,
        secret_chat_id: i32,
    ) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.secret_chat_id == Some(secret_chat_id))
    }

    pub fn register(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        view_generation: Option<ViewGeneration>,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id,
                view_generation,
                file_id: None,
                search_generation: None,
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                community_id: None,
                story_id: None,
                story_ids: None,
                story_album_id: None,
                folder_id: None,
                scope: None,
                secret_chat_id: None,
                rollback: None,
                window: self.window,
                created_at: Instant::now(),
            },
        );
        id
    }

    pub fn register_search(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        search_generation: u64,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id: None,
                view_generation: None,
                file_id: None,
                search_generation: Some(search_generation),
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                community_id: None,
                story_id: None,
                story_ids: None,
                story_album_id: None,
                folder_id: None,
                scope: None,
                secret_chat_id: None,
                rollback: None,
                window: self.window,
                created_at: Instant::now(),
            },
        );
        id
    }

    pub fn register_chat_search(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        chat_id: ChatId,
        search_generation: u64,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id: Some(chat_id),
                view_generation: None,
                file_id: None,
                search_generation: Some(search_generation),
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                community_id: None,
                story_id: None,
                story_ids: None,
                story_album_id: None,
                folder_id: None,
                scope: None,
                secret_chat_id: None,
                rollback: None,
                window: self.window,
                created_at: Instant::now(),
            },
        );
        id
    }

    pub fn register_around(
        &mut self,
        account_generation: AccountGeneration,
        chat_id: ChatId,
        view_generation: ViewGeneration,
        around_message_id: MessageId,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose: RequestPurpose::GetHistoryAround,
                chat_id: Some(chat_id),
                view_generation: Some(view_generation),
                file_id: None,
                search_generation: None,
                around_message_id: Some(around_message_id),
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                community_id: None,
                story_id: None,
                story_ids: None,
                story_album_id: None,
                folder_id: None,
                scope: None,
                secret_chat_id: None,
                rollback: None,
                window: self.window,
                created_at: Instant::now(),
            },
        );
        id
    }

    pub fn register_download(
        &mut self,
        account_generation: AccountGeneration,
        file_id: FileId,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose: RequestPurpose::DownloadFile,
                chat_id: None,
                view_generation: None,
                file_id: Some(file_id.0),
                search_generation: None,
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                community_id: None,
                story_id: None,
                story_ids: None,
                story_album_id: None,
                folder_id: None,
                scope: None,
                secret_chat_id: None,
                rollback: None,
                window: self.window,
                created_at: Instant::now(),
            },
        );
        id
    }

    pub fn take(&mut self, id: RequestId) -> Option<PendingRequest> {
        self.pending.remove(&id.0)
    }

    /// Drop the in-flight request with the given purpose, if any (Phase
    /// S2: storage-stats Refresh must drop the stale in-flight purpose,
    /// or the immediate refetch no-ops and the overlay shows "No storage
    /// data yet." until the old answer lands).
    pub fn take_purpose(&mut self, purpose: RequestPurpose) -> Option<PendingRequest> {
        let key = self
            .pending
            .iter()
            .find(|(_, req)| req.purpose == purpose)
            .map(|(key, _)| *key)?;
        self.pending.remove(&key)
    }

    pub(crate) fn invalidate_auth(&mut self) {
        self.pending
            .retain(|_, request| !is_auth_submit(request.purpose));
    }

    /// R5: drop sweepable requests (see [`RequestPurpose::is_sweepable`])
    /// registered at least `ttl` before `now` that `keep` does not exempt
    /// (the driver keeps requests waiting for a flood retry). Returns the
    /// dropped entries so the reducer can settle their UI state. Long
    /// running operations (downloads, uploads, calls, mutations) are never
    /// touched.
    pub fn sweep_stale(
        &mut self,
        now: Instant,
        ttl: Duration,
        keep: impl Fn(RequestId) -> bool,
    ) -> Vec<PendingRequest> {
        let stale: Vec<u64> = self
            .pending
            .values()
            .filter(|p| {
                (p.purpose.is_sweepable() || p.purpose.is_swept_without_error())
                    && now.saturating_duration_since(p.created_at) >= ttl
                    && !keep(p.id)
            })
            .map(|p| p.id.0)
            .collect();
        stale
            .into_iter()
            .filter_map(|key| self.pending.remove(&key))
            .collect()
    }

    /// Restart the TTL clock of a request that was re-sent (flood retry).
    pub fn touch(&mut self, id: RequestId, now: Instant) {
        if let Some(pending) = self.pending.get_mut(&id.0) {
            pending.created_at = now;
        }
    }

    /// Take every pending request, oldest first, so the reducer can settle
    /// what each one guarded ([`Session::settle_account_requests`]).
    pub(crate) fn drain_all(&mut self) -> Vec<PendingRequest> {
        let mut all: Vec<PendingRequest> = self.pending.drain().map(|(_, p)| p).collect();
        all.sort_by_key(|p| p.id.0);
        all
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn purpose(&self, id: RequestId) -> Option<RequestPurpose> {
        self.pending.get(&id.0).map(|p| p.purpose)
    }

    /// Slice G2: non-destructive view of a pending request, so the
    /// driver can capture mutation context before `apply` takes it.
    pub fn get(&self, id: RequestId) -> Option<&PendingRequest> {
        self.pending.get(&id.0)
    }

    pub fn has_auth_submit(&self) -> bool {
        self.pending.values().any(|p| is_auth_submit(p.purpose))
    }

    pub fn has_purpose(&self, purpose: RequestPurpose) -> bool {
        self.pending.values().any(|p| p.purpose == purpose)
    }

    /// Ids of in-flight requests for `chat_id` with any of `purposes`.
    pub fn ids_for_chat(&self, purposes: &[RequestPurpose], chat_id: ChatId) -> Vec<RequestId> {
        self.ids_for_chat_where(chat_id, |purpose| purposes.contains(&purpose))
    }

    /// Ids of in-flight requests for `chat_id` whose purpose `wanted` picks.
    pub fn ids_for_chat_where(
        &self,
        chat_id: ChatId,
        wanted: impl Fn(RequestPurpose) -> bool,
    ) -> Vec<RequestId> {
        self.pending
            .values()
            .filter(|p| p.chat_id == Some(chat_id) && wanted(p.purpose))
            .map(|p| p.id)
            .collect()
    }

    pub fn has_purpose_for_chat(&self, purpose: RequestPurpose, chat_id: ChatId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.chat_id == Some(chat_id))
    }

    /// Phase D3c: whether any `getChatEventLog` request is in flight for
    /// the chat, whatever its paging cursor. `has_purpose_for_chat`
    /// compares the full purpose (including `from_event_id`), so it
    /// cannot dedup across pages.
    pub fn has_event_log_in_flight(&self, chat_id: ChatId) -> bool {
        self.pending.values().any(|p| {
            matches!(
                p.purpose,
                RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { .. })
            ) && p.chat_id == Some(chat_id)
        })
    }

    /// Bots slice: whether a `getInlineQueryResults` request is in flight
    /// for the (chat, bot) pair, whatever the page. `has_purpose_for_chat`
    /// compares the full purpose (including `first_page`), so it cannot
    /// dedup across pages.
    pub fn has_inline_query_in_flight(&self, chat_id: ChatId, bot_user_id: i64) -> bool {
        self.pending.values().any(|p| {
            matches!(
                p.purpose,
                RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
                    chat_id: c,
                    bot_user_id: b,
                    ..
                }) if c == chat_id && b == bot_user_id
            )
        })
    }

    /// Mutable access to a pending request (parity slice: stamping
    /// `PendingRequest::scope` after `register`).
    pub fn pending_mut(&mut self, id: RequestId) -> Option<&mut PendingRequest> {
        self.pending.get_mut(&id.0)
    }

    /// Parity slice: per-scope in-flight check for
    /// `GetScopeNotificationSettings` (the purpose alone is shared by all
    /// three scopes).
    pub fn has_purpose_for_scope(
        &self,
        purpose: RequestPurpose,
        scope: NotificationSettingsScope,
    ) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.scope == Some(scope))
    }

    /// Phase 6: an in-flight request for a purpose/user pair (user-scoped
    /// `GetUserFullInfo` / `AddContact`).
    pub fn has_purpose_for_user(&self, purpose: RequestPurpose, user_id: i64) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.user_id == Some(user_id))
    }

    /// Phase 6: an in-flight request for a purpose/supergroup pair
    /// (`GetSupergroupFullInfo`).
    pub fn has_purpose_for_supergroup(&self, purpose: RequestPurpose, supergroup_id: i64) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.supergroup_id == Some(supergroup_id))
    }

    /// Slice (communities backend core): an in-flight request for a
    /// purpose/community pair (`GetCommunityFullInfo` /
    /// `SetCommunityName`).
    pub fn has_purpose_for_community(&self, purpose: RequestPurpose, community_id: i64) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.community_id == Some(community_id))
    }

    /// Phase 9.1: an in-flight request for a purpose/chat/story triple
    /// (`GetStory` prefetch of a chat's stories).
    pub fn has_purpose_for_story(
        &self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        story_id: i32,
    ) -> bool {
        self.pending.values().any(|p| {
            p.purpose == purpose && p.chat_id == Some(chat_id) && p.story_id == Some(story_id)
        })
    }

    /// Phase 9.7: an in-flight request for a purpose/chat/album triple
    /// (`GetStoryAlbumStories` per album).
    pub fn has_purpose_for_story_album(
        &self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        story_album_id: i32,
    ) -> bool {
        self.pending.values().any(|p| {
            p.purpose == purpose
                && p.chat_id == Some(chat_id)
                && p.story_album_id == Some(story_album_id)
        })
    }
    /// Parity slice: an in-flight request for a purpose/folder pair
    /// (`GetChatFolder` / `LoadFolderChats` / folder mutations).
    pub fn has_purpose_for_folder(&self, purpose: RequestPurpose, folder_id: i32) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.folder_id == Some(folder_id))
    }

    /// Parity slice: the folder id stamped on an in-flight request, if any.
    pub fn folder_id_for(&self, id: RequestId) -> Option<i32> {
        self.pending.get(&id.0).and_then(|p| p.folder_id)
    }

    /// The in-flight request id for a purpose/chat pair (test hook; the live
    /// path matches responses by `@extra`).
    pub fn pending_extra_for(
        &self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
    ) -> Option<RequestId> {
        self.pending
            .values()
            .find(|p| p.purpose == purpose && p.chat_id == chat_id)
            .map(|p| p.id)
    }

    /// Parity slice: the in-flight request id for a purpose/folder pair
    /// (test hook; the live path matches responses by `@extra`).
    pub fn pending_extra_for_folder(
        &self,
        purpose: RequestPurpose,
        folder_id: i32,
    ) -> Option<RequestId> {
        self.pending
            .values()
            .find(|p| p.purpose == purpose && p.folder_id == Some(folder_id))
            .map(|p| p.id)
    }

    pub fn has_download(&self, file_id: FileId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == RequestPurpose::DownloadFile && p.file_id == Some(file_id.0))
    }
}
