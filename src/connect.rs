//! Live TDLib connect gate: credentials + tdjson → setTdlibParameters → auth updates.
//! Never logs api_hash, phone numbers, or codes.

use crate::composer::{
    AttachmentKind, ComposerEdit, ComposerEditKind, ComposerSnapshot, DeleteConfirm,
    DraftSaveClock, DraftSaveStep, ForwardDraft, draft_text_to_store, schedule_draft_save,
};
use crate::credentials::TelegramCredentials;
use crate::diagnostics::DiagnosticSink;
use crate::ids::{AccountKey, ChatId, FileId, MessageId, RequestId};
use crate::lifecycle::{RestoreBlocker, plan_restore};
use crate::platform::{DatabaseKey, KeyDecision, SecretStore, load_or_create_key};
use crate::poll::{PollDraft, poll_answer_for_tap};
use crate::settings::{AccountPaths, default_app_root};
use crate::state::{
    ChatSearchJumpNeed, ForwardFlight, RequestPurpose, SearchStatus, Session, ShutdownPhase,
};
use crate::telegram::client::{LiveTdJson, OwnedEnvelope, ReceiveBridge};
use crate::telegram::envelope::{
    AuthorizationState, ChatDraft, ChatKind, ChatNotificationSettings, EnvelopePayload,
    MUTE_FOREVER, MessageContent,
};
use crate::telegram::ffi::{LibraryOrigin, TdJsonError, resolve_tdjson_path};
use crate::telegram::requests::{
    AnimationSend, PollSend, SetTdlibParameters, StickerSend, VideoNoteSend,
    VideoNoteThumbnailSend, VideoSend, add_chat_to_list, add_message_reaction,
    add_recently_found_chat, check_authentication_code, check_authentication_password,
    click_chat_sponsored_message, close_chat, close_request, delete_messages,
    download_file as download_file_request, edit_message_caption, edit_message_text,
    forward_messages, get_authorization_state, get_callback_query_answer, get_chat_history,
    get_chat_member, get_chat_sponsored_messages, get_commands, get_installed_sticker_sets, get_me,
    get_saved_animations, get_sticker_set, get_user_full_info, input_message_photo,
    input_message_video, join_chat, leave_chat, load_chats, open_chat, open_message_content,
    pin_chat_message, remove_message_reaction, report_chat_sponsored_message, search_chat_messages,
    search_chats, search_messages, search_recently_found_chats, send_animation, send_chat_action,
    send_chat_action_kind, send_document, send_message_album, send_photo, send_poll, send_sticker,
    send_text, send_video, send_video_note, send_voice_note, set_authentication_phone_number,
    set_chat_draft_message, set_chat_notification_settings, set_poll_answer, unpin_chat_message,
    view_messages, view_sponsored_chat,
};
use crate::voice::VoiceDraft;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Unigram `OutputChatActionManager` default delay and `ChatTextBox` keystroke
/// gap before another `SendChatAction` (`chatActionTyping`). tdesktop resends
/// every 5s (`kSendMyTypingInterval`); Quill follows the TDLib client's 4s.
pub const OUTGOING_TYPING_INTERVAL_MS: u64 = 4_000;

/// How many chats to ask TDLib to load per `loadChats` page.
pub const MAIN_CHAT_LOAD_LIMIT: i32 = 100;
/// Page size for `getChatHistory`.
pub const HISTORY_PAGE_SIZE: i32 = 50;
/// `downloadFile.priority` for automatic photo thumbs (schema: 1–32).
pub const THUMB_DOWNLOAD_PRIORITY: i32 = 1;
/// `downloadFile.priority` when the user opens media.
pub const USER_DOWNLOAD_PRIORITY: i32 = 32;
/// `searchChats.limit` / `searchMessages.limit` (Unigram messages page is 20).
pub const SEARCH_LIMIT: i32 = 20;
/// `searchRecentlyFoundChats.limit` — schema/Unigram cap is 50.
pub const RECENT_SEARCH_LIMIT: i32 = 50;
/// tdesktop `kSearchRequestDelay` / `AutoSearchTimeout` (config.h): 900 ms.
/// ComposeSearch `requestSearchDelayed` uses the same `AutoSearchTimeout`.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(900);
/// tdesktop `kSaveDraftTimeout` — quiet time before `setChatDraftMessage`.
pub const DRAFT_SAVE_DEBOUNCE: Duration = Duration::from_millis(1_000);
/// tdesktop `kSearchPerPage` (`api_messages_search.cpp`).
pub const CHAT_SEARCH_LIMIT: i32 = 50;
/// Unigram `LoadMessageSliceImpl`: `GetChatHistory(chatId, maxId, -25, 50)`.
pub const HISTORY_AROUND_OFFSET: i32 = -25;
/// Unigram `LoadMessageSliceImpl` page size around the jump target.
pub const HISTORY_AROUND_LIMIT: i32 = 50;

/// In-flight global search extras (official empty = recents; typed = chats + messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFlight {
    Recents(RequestId),
    Query(RequestId, RequestId),
}

/// Empty/open recents send immediately; typed queries wait for [`SEARCH_DEBOUNCE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchQueryOutcome {
    Sent(SearchFlight),
    Debounced { token: u64 },
    Unchanged,
}

/// In-chat search extras (`searchChatMessages`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchFlight {
    Query(RequestId),
}

/// Empty in-chat query clears immediately; typed queries wait for [`SEARCH_DEBOUNCE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchQueryOutcome {
    Sent(ChatSearchFlight),
    Debounced { token: u64 },
    Unchanged,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectBlocker {
    MissingCredentials,
    MissingTdjson,
    MissingKeyAgainstExistingDb,
    LockedStore,
    StoreError,
    TdjsonLoad,
}

impl ConnectBlocker {
    pub fn user_message(&self) -> &'static str {
        match self {
            ConnectBlocker::MissingCredentials => {
                "set TELEGRAM_API_ID / TELEGRAM_API_HASH (or local .env)"
            }
            ConnectBlocker::MissingTdjson => {
                "tdjson not found — set QUILL_TDJSON_PATH to libtdjson, or bundle it next to the executable (see docs/native-bundle.md). Homebrew paths are never searched."
            }
            ConnectBlocker::MissingKeyAgainstExistingDb => {
                "database encryption key missing for an existing TDLib database — restore the key or remove the local database"
            }
            ConnectBlocker::LockedStore => "secret store is locked or unavailable",
            ConnectBlocker::StoreError => "secret store error",
            ConnectBlocker::TdjsonLoad => {
                "tdjson library found but failed to load (missing symbols or wrong arch)"
            }
        }
    }

    /// One-token label for `--connect-smoke` (no secrets).
    pub fn slug(&self) -> &'static str {
        match self {
            ConnectBlocker::MissingCredentials => "missing-credentials",
            ConnectBlocker::MissingTdjson => "missing-tdjson",
            ConnectBlocker::MissingKeyAgainstExistingDb => "missing-key",
            ConnectBlocker::LockedStore => "locked-store",
            ConnectBlocker::StoreError => "store-error",
            ConnectBlocker::TdjsonLoad => "tdjson-load",
        }
    }
}

impl From<RestoreBlocker> for ConnectBlocker {
    fn from(value: RestoreBlocker) -> Self {
        match value {
            RestoreBlocker::MissingCredentials => ConnectBlocker::MissingCredentials,
            RestoreBlocker::MissingKeyAgainstExistingDb => {
                ConnectBlocker::MissingKeyAgainstExistingDb
            }
            RestoreBlocker::LockedStore => ConnectBlocker::LockedStore,
            RestoreBlocker::StoreError => ConnectBlocker::StoreError,
        }
    }
}

/// Outcome of the pre-flight gate (no native load yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectGate {
    Blocked(ConnectBlocker),
    Ready { tdjson: LibraryOrigin },
}

/// Classify credentials + tdjson availability. Does not open a client.
pub fn evaluate_gate(credentials_present: bool) -> ConnectGate {
    if !credentials_present {
        return ConnectGate::Blocked(ConnectBlocker::MissingCredentials);
    }
    match resolve_tdjson_path() {
        Some(tdjson) => ConnectGate::Ready { tdjson },
        None => ConnectGate::Blocked(ConnectBlocker::MissingTdjson),
    }
}

/// Local paths + database key ready for `setTdlibParameters`.
#[derive(Debug)]
pub struct PreparedConnect {
    pub account: AccountKey,
    pub paths: AccountPaths,
    pub database_key: DatabaseKey,
    pub database_exists: bool,
}

/// Resolve account paths and DB key. Credentials must already be validated.
pub fn prepare_connect<S: SecretStore + ?Sized>(
    app_root: &Path,
    account: AccountKey,
    store: &S,
    credentials: &TelegramCredentials,
) -> Result<PreparedConnect, ConnectBlocker> {
    let plan = plan_restore(
        app_root,
        account.clone(),
        store,
        Some(credentials.api_id),
        Some(credentials.api_hash.as_str()),
    )
    .map_err(ConnectBlocker::from)?;
    let key =
        load_or_create_key(store, &plan.account, plan.database_exists).map_err(|e| match e {
            KeyDecision::MissingAgainstExistingDb => ConnectBlocker::MissingKeyAgainstExistingDb,
            KeyDecision::Locked => ConnectBlocker::LockedStore,
            KeyDecision::Store(_) => ConnectBlocker::StoreError,
        })?;
    std::fs::create_dir_all(&plan.paths.tdlib_database).map_err(|_| ConnectBlocker::StoreError)?;
    std::fs::create_dir_all(&plan.paths.tdlib_files).map_err(|_| ConnectBlocker::StoreError)?;
    Ok(PreparedConnect {
        account: plan.account,
        paths: plan.paths,
        database_key: key,
        database_exists: plan.database_exists,
    })
}

/// Build `setTdlibParameters` from loaded credentials + local paths/key.
/// The returned JSON includes `api_hash`; callers must not log it.
pub fn build_set_tdlib_parameters(
    credentials: &TelegramCredentials,
    paths: &AccountPaths,
    database_key: &DatabaseKey,
) -> SetTdlibParameters {
    SetTdlibParameters {
        use_test_dc: std::env::var("QUILL_USE_TEST_DC")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        database_directory: paths.tdlib_database.to_string_lossy().into_owned(),
        files_directory: paths.tdlib_files.to_string_lossy().into_owned(),
        database_encryption_key_b64: database_key.tdlib_base64(),
        api_id: credentials.api_id,
        api_hash: credentials.api_hash.clone(),
        device_model: "Desktop".into(),
        system_version: std::env::consts::OS.into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        system_language_code: "en".into(),
    }
}

/// Outbound JSON (live `td_send` or test recorder). Must not log request bodies.
pub trait JsonSender: Send {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectSendError {
    InvalidRequest,
    Native,
}

/// Records outbound JSON for unit/replay tests (no network).
#[derive(Default)]
pub struct RecordingSender {
    pub sent: Mutex<Vec<String>>,
}

impl RecordingSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.sent.lock().expect("recording sender").clone()
    }
}

impl JsonSender for RecordingSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.sent
            .lock()
            .expect("recording sender")
            .push(request.to_string());
        Ok(())
    }
}

impl JsonSender for Arc<RecordingSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        (**self).send_json(request)
    }
}

/// Live `td_send` wrapper.
pub struct LiveSender {
    api: Arc<crate::telegram::ffi::TdJson>,
    client_id: i32,
}

impl LiveSender {
    pub fn from_live(live: &LiveTdJson) -> Self {
        Self {
            api: live.api.clone(),
            client_id: live.client_id,
        }
    }
}

impl JsonSender for LiveSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        self.api.send(self.client_id, request).map_err(|e| match e {
            TdJsonError::InvalidRequest => ConnectSendError::InvalidRequest,
            _ => ConnectSendError::Native,
        })
    }
}

/// Session + outbound sender that auto-replies to `WaitTdlibParameters`.
pub struct ConnectDriver<S: JsonSender> {
    pub session: Session,
    sender: S,
    credentials: TelegramCredentials,
    paths: AccountPaths,
    database_key: DatabaseKey,
    parameters_sent: bool,
    search_debounce_token: u64,
    pending_typed_search: Option<(u64, String)>,
    chat_search_debounce_token: u64,
    pending_typed_chat_search: Option<(u64, String)>,
    /// Last `chatActionTyping` we sent (Unigram `_lastTypingTime`).
    outgoing_typing: Option<OutgoingTyping>,
    /// Last `chatActionRecordingVoiceNote` (Unigram record button).
    outgoing_voice: Option<OutgoingTyping>,
    /// tdesktop `saveDraft` clock for the open composer.
    draft_clock: DraftSaveClock,
    draft_save_token: u64,
    pending_draft: Option<PendingDraft>,
}

struct OutgoingTyping {
    chat_id: ChatId,
    last_sent_ms: u64,
}

struct PendingDraft {
    token: u64,
    chat_id: ChatId,
    text: String,
    reply_to: Option<MessageId>,
}

/// Result of noting a composer edit. UI arms a timer only for `Debounced`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftSaveOutcome {
    Debounced { token: u64, delay: Duration },
    Sent,
    Skipped,
}

impl<S: JsonSender> ConnectDriver<S> {
    pub fn new(
        session: Session,
        sender: S,
        credentials: TelegramCredentials,
        prepared: PreparedConnect,
    ) -> Self {
        Self {
            session,
            sender,
            credentials,
            paths: prepared.paths,
            database_key: prepared.database_key,
            parameters_sent: false,
            search_debounce_token: 0,
            pending_typed_search: None,
            chat_search_debounce_token: 0,
            pending_typed_chat_search: None,
            outgoing_typing: None,
            outgoing_voice: None,
            draft_clock: DraftSaveClock::idle(),
            draft_save_token: 0,
            pending_draft: None,
        }
    }

    pub fn parameters_sent(&self) -> bool {
        self.parameters_sent
    }

    pub fn tdlib_files(&self) -> &Path {
        &self.paths.tdlib_files
    }

    /// Kick the JSON client so authorization updates start flowing.
    pub fn kickoff(&mut self) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::GetAuthorizationState, None);
        self.sender.send_json(&get_authorization_state(extra))?;
        Ok(extra)
    }

    pub fn ingest(&mut self, owned: OwnedEnvelope) -> Result<(), ConnectSendError> {
        let was_ready = matches!(self.session.auth, AuthorizationState::Ready);
        // Continue paging only when this envelope completes an in-flight loadChats
        // with ok. Unrelated ingest ticks and non-404 errors must not re-issue.
        let load_chats_ok = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && owned.envelope.extra.is_some_and(|id| {
                self.session.requests.purpose(id) == Some(RequestPurpose::LoadChats)
            });
        let view_purpose = owned
            .envelope
            .extra
            .and_then(|id| self.session.requests.purpose(id));
        let view_after = match &owned.envelope.payload {
            EnvelopePayload::Messages(_) | EnvelopePayload::UpdateNewMessage(_) => true,
            EnvelopePayload::Ok | EnvelopePayload::Error(_)
                if view_purpose == Some(RequestPurpose::ViewMessages) =>
            {
                true
            }
            _ => false,
        };
        let thumbs_after = matches!(
            owned.envelope.payload,
            EnvelopePayload::Messages(_)
                | EnvelopePayload::UpdateNewMessage(_)
                | EnvelopePayload::UpdateMessageContent { .. }
                | EnvelopePayload::UpdateFile(_)
                | EnvelopePayload::File(_)
        );
        let chat_search_hits = matches!(
            owned.envelope.payload,
            EnvelopePayload::FoundChatMessages { .. }
        );
        self.session.apply(owned);
        self.maybe_send_parameters()?;
        self.maybe_probe_channel_membership()?;
        let became_ready = !was_ready && matches!(self.session.auth, AuthorizationState::Ready);
        if became_ready || load_chats_ok {
            self.maybe_load_main_chats()?;
        }
        if view_after {
            self.maybe_view_open_messages()?;
        }
        if thumbs_after || self.session.stickers.open || self.session.gifs.open {
            self.maybe_download_open_thumbs()?;
        }
        self.maybe_load_selected_sticker_set()?;
        self.maybe_refresh_saved_animations()?;
        if chat_search_hits {
            // Unigram ChatSearchViewModel: first hit → LoadMessageSliceAsync.
            self.jump_selected_chat_search_hit()?;
        }
        Ok(())
    }

    fn maybe_send_parameters(&mut self) -> Result<(), ConnectSendError> {
        if self.parameters_sent {
            return Ok(());
        }
        if !matches!(self.session.auth, AuthorizationState::WaitTdlibParameters) {
            return Ok(());
        }
        let extra = self.session.request(RequestPurpose::SetParameters, None);
        let params = build_set_tdlib_parameters(&self.credentials, &self.paths, &self.database_key);
        // Contains api_hash — do not log `json`.
        let json = params.to_json(extra);
        self.sender.send_json(&json)?;
        self.parameters_sent = true;
        Ok(())
    }

    fn chats_path_active(&self) -> bool {
        matches!(self.session.auth, AuthorizationState::Ready)
            && matches!(self.session.shutdown, ShutdownPhase::Running)
    }

    /// First page on Ready; further pages only when a `loadChats` request returns ok.
    pub fn maybe_load_main_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        if self.session.chats_exhausted {
            return Ok(None);
        }
        if self.session.requests.has_purpose(RequestPurpose::LoadChats) {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::LoadChats, None);
        self.sender
            .send_json(&load_chats(extra, MAIN_CHAT_LOAD_LIMIT))?;
        Ok(Some(extra))
    }

    /// Select a chat, inform TDLib it is open, and request history.
    /// Returns `None` if history is already complete or a history request
    /// is already in flight.
    pub fn select_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.open_chat == Some(chat_id) {
            self.maybe_probe_channel_membership()?;
            self.maybe_fetch_bot_info()?;
            self.maybe_fetch_bot_commands()?;
            self.maybe_view_open_messages()?;
            self.maybe_download_open_thumbs()?;
            self.fetch_sponsored_messages(chat_id)?;
            return self.fetch_history();
        }
        self.cancel_outgoing_typing()?;
        self.close_open_chat()?;
        self.draft_clock = DraftSaveClock::idle();
        self.pending_draft = None;
        self.session.open_chat(chat_id);
        // Channels are ungated since Phase 2.2: they follow the normal
        // openChat / history path; sponsored rows fetch for every channel.
        self.send_open_chat(chat_id)?;
        self.maybe_probe_channel_membership()?;
        self.maybe_fetch_bot_info()?;
        self.maybe_fetch_bot_commands()?;
        self.maybe_view_open_messages()?;
        self.maybe_download_open_thumbs()?;
        self.fetch_sponsored_messages(chat_id)?;
        self.fetch_history()
    }

    /// Own membership probe for the open broadcast channel: `getMe` once, then
    /// `getChatMember`. Drives the composer gate and the join/leave affordance
    /// (`ChannelMemberStatus`). No-op for non-channels.
    fn maybe_probe_channel_membership(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_channel())
        {
            return Ok(());
        }
        let Some(my_id) = self.session.my_user_id else {
            if self.session.requests.has_purpose(RequestPurpose::GetMe) {
                return Ok(());
            }
            let extra = self.session.request(RequestPurpose::GetMe, None);
            return self
                .sender
                .send_json(&get_me(extra))
                .map(|_| ())
                .inspect_err(|_| {
                    self.session.requests.take(extra);
                });
        };
        if self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.my_member_status.is_some())
        {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatMember, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatMember, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_member(extra, chat_id, my_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 3.1: lazy `getUserFullInfo` for the open bot chat. Fires once
    /// per uncached bot (deduped by cache + in-flight purpose); the
    /// `userFullInfo` response and `updateUserFullInfo` both populate
    /// `Session::bot_info`. No-op for non-bot chats.
    fn maybe_fetch_bot_info(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        let Some(user_id) = self.session.bot_user_id_for_chat(chat_id) else {
            return Ok(());
        };
        if self.session.bot_info.contains_key(&user_id) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetUserFullInfo, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetUserFullInfo, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_user_full_info(extra, user_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 3.3: `getCommands` for the open bot chat's global commands.
    /// A null scope selects the default scope (`botCommandScopeDefault`,
    /// schema 1.8.67 line 10360) with an empty language code. Fires once
    /// per bot (deduped by cache and in-flight purpose) alongside the
    /// `getUserFullInfo` fetch. The schema annotates `getCommands`
    /// "for bots only" (TDLib 1.8.67 line 14953), so a user session gets
    /// an `error` answer — recorded as an empty command set by
    /// `Session::apply`, with no retry on later chat selections; the `/`
    /// menu then shows the `botInfo` commands only.
    fn maybe_fetch_bot_commands(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        let Some(user_id) = self.session.bot_user_id_for_chat(chat_id) else {
            return Ok(());
        };
        if self.session.bot_commands.contains_key(&user_id) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetCommands, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetCommands, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_commands(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// `joinChat` for a public channel. Own membership updates arrive via
    /// `updateChatMember`; the response also flips status optimistically.
    pub fn join_channel(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::JoinChat, Some(chat_id));
        if let Err(err) = self.sender.send_json(&join_chat(extra, chat_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// `leaveChat` for a channel. Own membership updates arrive via
    /// `updateChatMember`; the `ok` response flips status optimistically.
    pub fn leave_channel(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::LeaveChat, Some(chat_id));
        if let Err(err) = self.sender.send_json(&leave_chat(extra, chat_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Composer edit in a private chat. `delayed` follows tdesktop `saveDraft(true)`
    /// (1s quiet, 5s cap). `delayed == false` is Unigram's flush on leaving the chat.
    pub fn note_composer_draft(
        &mut self,
        chat_id: ChatId,
        text: &str,
        reply_to: Option<MessageId>,
        now_ms: u64,
        delayed: bool,
    ) -> Result<DraftSaveOutcome, ConnectSendError> {
        if !self.chats_path_active() || !self.session.accepts_composer_draft(chat_id) {
            return Ok(DraftSaveOutcome::Skipped);
        }
        let stored = draft_text_to_store(text, reply_to.is_some()).map(str::to_string);
        let reply_to = stored.as_ref().and(reply_to);
        if self.draft_matches(chat_id, stored.as_deref(), reply_to) {
            self.pending_draft = None;
            self.draft_clock = DraftSaveClock::idle();
            let existing = self
                .session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| chat.draft.clone());
            self.session.store_composer_draft(chat_id, existing);
            return Ok(DraftSaveOutcome::Skipped);
        }
        self.session.mark_draft_dirty(chat_id);
        match schedule_draft_save(self.draft_clock, now_ms, delayed) {
            DraftSaveStep::Wait {
                delay_ms,
                started_ms,
            } => {
                self.draft_clock = DraftSaveClock {
                    started_ms: Some(started_ms),
                };
                self.draft_save_token = self.draft_save_token.saturating_add(1);
                let token = self.draft_save_token;
                self.pending_draft = Some(PendingDraft {
                    token,
                    chat_id,
                    text: stored.unwrap_or_default(),
                    reply_to,
                });
                Ok(DraftSaveOutcome::Debounced {
                    token,
                    delay: Duration::from_millis(delay_ms),
                })
            }
            DraftSaveStep::Write => {
                self.pending_draft = None;
                self.draft_clock = DraftSaveClock::idle();
                self.send_draft(chat_id, stored.as_deref(), reply_to)?;
                Ok(DraftSaveOutcome::Sent)
            }
        }
    }

    /// Timer fired. Sends only if `token` is still the latest quiet window.
    pub fn commit_debounced_draft(
        &mut self,
        token: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(pending) = self.pending_draft.take() else {
            return Ok(None);
        };
        if pending.token != token {
            self.pending_draft = Some(pending);
            return Ok(None);
        }
        self.draft_clock = DraftSaveClock::idle();
        let text = draft_text_to_store(&pending.text, pending.reply_to.is_some());
        let reply_to = text.and(pending.reply_to);
        if self.draft_matches(pending.chat_id, text, reply_to) {
            self.session.store_composer_draft(
                pending.chat_id,
                self.session
                    .chats
                    .get(&pending.chat_id.0)
                    .and_then(|chat| chat.draft.clone()),
            );
            return Ok(None);
        }
        self.send_draft(pending.chat_id, text, reply_to).map(Some)
    }

    /// Successful send: drop the draft when the composer is still idle.
    pub fn clear_draft_after_send(
        &mut self,
        chat_id: ChatId,
        composer_idle: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !composer_idle || self.session.draft_is_dirty(chat_id) {
            return Ok(None);
        }
        if !self.session.accepts_composer_draft(chat_id) {
            return Ok(None);
        }
        self.pending_draft = None;
        self.draft_clock = DraftSaveClock::idle();
        self.send_draft(chat_id, None, None).map(Some)
    }

    pub fn cancel_pending_draft(&mut self) {
        self.pending_draft = None;
        self.draft_clock = DraftSaveClock::idle();
    }

    fn draft_matches(
        &self,
        chat_id: ChatId,
        text: Option<&str>,
        reply_to: Option<MessageId>,
    ) -> bool {
        let current = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.draft.as_ref());
        match (current, text) {
            (None, None) => true,
            (Some(draft), Some(text)) => {
                draft.text == text && draft.reply_to_message_id == reply_to
            }
            _ => false,
        }
    }

    fn send_draft(
        &mut self,
        chat_id: ChatId,
        text: Option<&str>,
        reply_to: Option<MessageId>,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SetChatDraftMessage, Some(chat_id));
        match self
            .sender
            .send_json(&set_chat_draft_message(extra, chat_id, text, reply_to))
        {
            Ok(()) => {
                let draft = text
                    .filter(|text| !text.trim().is_empty() || reply_to.is_some())
                    .map(|text| ChatDraft {
                        text: text.to_string(),
                        reply_to_message_id: reply_to,
                    });
                self.session.store_composer_draft(chat_id, draft);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    fn close_open_chat(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.session.open_chat else {
            return Ok(());
        };
        if !self
            .session
            .chats
            .get(&prev.0)
            .is_some_and(|chat| chat.supported())
        {
            return Ok(());
        }
        let extra = self.session.request(RequestPurpose::CloseChat, Some(prev));
        self.sender.send_json(&close_chat(extra, prev))?;
        Ok(())
    }

    fn send_open_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::OpenChat, Some(chat_id));
        self.sender.send_json(&open_chat(extra, chat_id))?;
        Ok(extra)
    }

    /// `viewMessages` for loaded history in the open chat (TDLib 1.8.67).
    /// Unread counts change only when `updateChatReadInbox` arrives.
    pub fn maybe_view_open_messages(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(None);
        };
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ViewMessages, chat_id)
        {
            return Ok(None);
        }
        let ids = self.session.message_ids_to_view(chat_id);
        if ids.is_empty() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewMessages, Some(chat_id));
        match self
            .sender
            .send_json(&view_messages(extra, chat_id, &ids, true))
        {
            Ok(()) => {
                self.session.begin_viewing(chat_id, &ids);
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Auto-download photo thumbs in the open chat (`priority` 1). Skips secret/spoiler.
    pub fn maybe_download_open_thumbs(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(Vec::new());
        }
        let ids = self.session.thumb_file_ids_to_download();
        let mut extras = Vec::new();
        for file_id in ids {
            if let Some(extra) = self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY)? {
                extras.push(extra);
            }
        }
        Ok(extras)
    }

    /// Send `downloadFile` (`synchronous: false`). No-op if already local or in flight.
    pub fn download_file(
        &mut self,
        file_id: FileId,
        priority: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.should_download(file_id) {
            return Ok(None);
        }
        let extra = self.session.request_download(file_id);
        self.session.begin_download(file_id);
        match self
            .sender
            .send_json(&download_file_request(extra, file_id, priority))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.abort_download(file_id);
                Err(err)
            }
        }
    }

    /// Load another page of history for the open chat (`from_message_id` = oldest, or 0).
    pub fn fetch_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if self
            .session
            .histories
            .get(&chat_id.0)
            .is_some_and(|h| h.loaded_complete)
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetHistory, chat_id)
        {
            return Ok(None);
        }
        let from = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.oldest_id())
            .unwrap_or(MessageId(0));
        let extra = self
            .session
            .request(RequestPurpose::GetHistory, Some(chat_id));
        self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            from,
            0,
            HISTORY_PAGE_SIZE,
            false,
        ))?;
        Ok(Some(extra))
    }

    /// `getChatSponsoredMessages` for a channel chat (TDLib 1.8.67). Called
    /// when a channel is opened; rows render Sponsored / Recommended.
    /// The fetch already runs so the pipeline is proven with replay
    /// fixtures. Bot chats can also carry sponsored messages per the
    /// schema; they are not fetched yet (Phase 3).
    pub fn fetch_sponsored_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_channel = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
        if !is_channel {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatSponsoredMessages, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatSponsoredMessages, Some(chat_id));
        match self
            .sender
            .send_json(&get_chat_sponsored_messages(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `reportChatSponsoredMessage` (TDLib 1.8.67). Empty `option_id` starts
    /// the flow; TDLib may answer `reportSponsoredResultOptionRequired`.
    /// Returns `None` when the row is missing or `can_be_reported` is false.
    pub fn report_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        option_id: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .begin_sponsored_report(chat_id, message_id)
            .is_none()
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&report_chat_sponsored_message(
            extra, chat_id, message_id, option_id,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.dismiss_sponsored_report();
                Err(err)
            }
        }
    }

    /// `clickChatSponsoredMessage` (TDLib 1.8.67): the user opened a sponsored
    /// message's sponsor link/button (`is_media_click = false`) or its media
    /// (`is_media_click = true`). Fire-and-forget; the `ok` response is ignored.
    pub fn click_chat_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        is_media_click: bool,
        from_fullscreen: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ClickChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&click_chat_sponsored_message(
            extra,
            chat_id,
            message_id,
            is_media_click,
            from_fullscreen,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `viewSponsoredChat` (TDLib 1.8.67): the user fully viewed a sponsored
    /// chat. The unique id comes from `sponsoredChat` search results.
    pub fn view_sponsored_chat(
        &mut self,
        sponsored_chat_unique_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewSponsoredChat, None);
        match self
            .sender
            .send_json(&view_sponsored_chat(extra, sponsored_chat_unique_id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Open the sticker panel and load installed regular sets
    /// (`getInstalledStickerSets` + `stickerTypeRegular`).
    pub fn open_sticker_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.open = true;
        self.session.stickers.failed = false;
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledStickerSets)
        {
            return Ok(None);
        }
        if !self.session.stickers.sets.is_empty() {
            return self.maybe_load_selected_sticker_set();
        }
        self.session.stickers.loading_sets = true;
        let extra = self
            .session
            .request(RequestPurpose::GetInstalledStickerSets, None);
        match self.sender.send_json(&get_installed_sticker_sets(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.loading_sets = false;
                Err(err)
            }
        }
    }

    pub fn close_sticker_panel(&mut self) {
        self.session.stickers.close();
    }

    pub fn select_sticker_set(
        &mut self,
        set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.select_sticker_set(set_id);
        self.maybe_load_selected_sticker_set()
    }

    fn maybe_load_selected_sticker_set(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.stickers.open || !self.chats_path_active() {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetStickerSet)
        {
            return Ok(None);
        }
        let Some(set_id) = self.session.stickers.selected_needs_load() else {
            return Ok(None);
        };
        self.session.mark_sticker_set_loading();
        let extra = self.session.request(RequestPurpose::GetStickerSet, None);
        match self.sender.send_json(&get_sticker_set(extra, set_id)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.loading_set = false;
                Err(err)
            }
        }
    }

    /// Open the GIF panel and load `getSavedAnimations`.
    pub fn open_gif_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.gifs.open = true;
        self.session.gifs.failed = false;
        self.maybe_refresh_saved_animations()
    }

    pub fn close_gif_panel(&mut self) {
        self.session.gifs.close();
    }

    fn maybe_refresh_saved_animations(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.gifs.open || !self.chats_path_active() {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetSavedAnimations)
        {
            return Ok(None);
        }
        let needs = self.session.gifs.animations.is_empty() || self.session.gifs.stale;
        if !needs {
            return Ok(None);
        }
        self.session.gifs.loading = true;
        self.session.gifs.stale = false;
        let extra = self
            .session
            .request(RequestPurpose::GetSavedAnimations, None);
        match self.sender.send_json(&get_saved_animations(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.gifs.loading = false;
                Err(err)
            }
        }
    }

    /// `sendMessage` + `inputMessageAnimation` / `inputAnimation` / `inputFileId`.
    pub fn send_animation(
        &mut self,
        chat_id: ChatId,
        animation: AnimationSend,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || animation.file_id.0 == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let json = send_animation(extra, chat_id, animation);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessage` + `inputMessageSticker` / `inputFileId` (Unigram compose).
    pub fn send_sticker(
        &mut self,
        chat_id: ChatId,
        sticker: StickerSend<'_>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || sticker.file_id.0 == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let json = send_sticker(extra, chat_id, sticker);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `sendMessage` for a snapshot frozen at composer submit.
    /// Caption / path are not logged.
    pub fn send_text_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_snapshot(snapshot)
    }

    /// Send text, photo, document, or local video via `sendMessage` (TDLib 1.8.67).
    pub fn send_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if snapshot.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if snapshot.is_media_album() {
            return self.send_album_snapshot(snapshot);
        }
        let chat_id = snapshot.chat_id();
        // Phase 2.3: channel posting is admin-gated (`can_post` derives the
        // right from own membership); the driver rejects non-admin channel
        // sends the same way the hidden composer does.
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = snapshot.caption();
        if snapshot.attachment.is_none() && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let reply_to = snapshot.send_reply_to();
        // Validate the picked path before allocating `@extra`.
        let media_path = match snapshot.attachment.as_ref() {
            Some(att) => Some(
                att.send_path_str()
                    .ok_or(ConnectSendError::InvalidRequest)?,
            ),
            None => None,
        };
        let video_probe = match snapshot.attachment.as_ref() {
            Some(att) if att.kind == AttachmentKind::Video => Some(
                crate::video::probe_local_video(&att.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?,
            ),
            _ => None,
        };
        let video_note = match snapshot.attachment.as_ref() {
            Some(att) if att.kind == AttachmentKind::VideoNote => {
                let probe = crate::video::probe_local_video_note(&att.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?;
                let thumbnail = crate::video::write_video_note_thumbnail(&att.path).map(|thumb| {
                    VideoNoteThumbnailSend {
                        path: thumb.path.to_string_lossy().into_owned(),
                        width: thumb.width,
                        height: thumb.height,
                    }
                });
                Some(VideoNoteSend {
                    duration: probe.duration,
                    length: probe.length,
                    thumbnail,
                })
            }
            _ => None,
        };
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        // Contains caption / path — do not log `json`.
        let json = match (snapshot.attachment.as_ref(), media_path.as_deref()) {
            (Some(att), Some(path)) => match att.kind {
                AttachmentKind::Photo => send_photo(extra, chat_id, path, caption, reply_to),
                AttachmentKind::Document => send_document(extra, chat_id, path, caption, reply_to),
                AttachmentKind::Video => {
                    let probe = video_probe.ok_or_else(|| {
                        self.session.requests.take(extra);
                        ConnectSendError::InvalidRequest
                    })?;
                    send_video(
                        extra,
                        chat_id,
                        path,
                        &VideoSend {
                            duration: probe.duration,
                            width: probe.width,
                            height: probe.height,
                            supports_streaming: probe.supports_streaming,
                        },
                        caption,
                        reply_to,
                    )
                }
                AttachmentKind::VideoNote => {
                    let note = video_note.ok_or_else(|| {
                        self.session.requests.take(extra);
                        ConnectSendError::InvalidRequest
                    })?;
                    send_video_note(extra, chat_id, path, &note, reply_to)
                }
            },
            (None, None) => send_text(extra, chat_id, caption, reply_to),
            _ => {
                self.session.requests.take(extra);
                return Err(ConnectSendError::InvalidRequest);
            }
        };
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessageAlbum` for 2–10 local photos and/or videos.
    fn send_album_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        let chat_id = snapshot.chat_id();
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || !snapshot.is_media_album() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = snapshot.caption();
        let last = snapshot.album.len() - 1;
        let mut contents = Vec::with_capacity(snapshot.album.len());
        for (index, att) in snapshot.album.iter().enumerate() {
            let path = att
                .send_path_str()
                .ok_or(ConnectSendError::InvalidRequest)?;
            let item_caption = if index == last { caption } else { "" };
            let content = match att.kind {
                AttachmentKind::Photo => input_message_photo(&path, item_caption),
                AttachmentKind::Video => {
                    let probe = crate::video::probe_local_video(&att.path)
                        .map_err(|_| ConnectSendError::InvalidRequest)?;
                    input_message_video(
                        &path,
                        &VideoSend {
                            duration: probe.duration,
                            width: probe.width,
                            height: probe.height,
                            supports_streaming: probe.supports_streaming,
                        },
                        item_caption,
                    )
                }
                AttachmentKind::Document | AttachmentKind::VideoNote => {
                    return Err(ConnectSendError::InvalidRequest);
                }
            };
            contents.push(content);
        }
        let reply_to = snapshot.send_reply_to();
        let extra = self
            .session
            .request(RequestPurpose::SendMessageAlbum, Some(chat_id));
        let json = send_message_album(extra, chat_id, reply_to, contents);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessage` + `inputMessageVoiceNote` for a finished local capture.
    pub fn send_voice_note(
        &mut self,
        draft: &VoiceDraft,
        caption: &str,
        reply_to: Option<MessageId>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let path = crate::local_path::pick_send_path(&draft.path)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = path.to_string_lossy().into_owned();
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let json = send_voice_note(
            extra,
            chat_id,
            &path,
            draft.duration_secs,
            &draft.waveform_b64(),
            caption,
            reply_to,
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                let _ = self.sync_voice_recording(false, 0);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `openMessageContent` when playback of a voice note starts.
    pub fn open_voice_content(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::OpenMessageContent, Some(chat_id));
        let json = open_message_content(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 3.2: send a callback query for an inline keyboard button press
    /// (`getCallbackQueryAnswer`; TDLib answers with `callbackQueryAnswer`).
    /// Only real (non-pending) messages in a supported chat can be answered.
    pub fn send_callback_query(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        data: &[u8],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if message.pending || message.id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetCallbackQueryAnswer, Some(chat_id));
        let json = get_callback_query_answer(extra, chat_id, message_id, data);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// While the voice bar is recording, send `chatActionRecordingVoiceNote`
    /// at most every [`OUTGOING_TYPING_INTERVAL_MS`] (Unigram record action).
    /// Stopping sends `chatActionCancel`.
    pub fn sync_voice_recording(
        &mut self,
        active: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let chat_id = self
            .session
            .open_chat
            .filter(|id| self.typing_chat_allowed(*id));
        let Some(chat_id) = chat_id.filter(|_| active) else {
            return self.cancel_outgoing_voice();
        };
        let _ = self.cancel_outgoing_typing();
        if let Some(prev) = &self.outgoing_voice {
            if prev.chat_id != chat_id {
                self.cancel_outgoing_voice()?;
            } else if now_ms.saturating_sub(prev.last_sent_ms) < OUTGOING_TYPING_INTERVAL_MS {
                return Ok(());
            }
        }
        self.send_voice_action(chat_id, true, now_ms)
    }

    fn cancel_outgoing_voice(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.outgoing_voice.take() else {
            return Ok(());
        };
        if !self.typing_chat_allowed(prev.chat_id) {
            return Ok(());
        }
        self.send_voice_action(prev.chat_id, false, 0)
    }

    fn send_voice_action(
        &mut self,
        chat_id: ChatId,
        recording: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let json = send_chat_action_kind(
            extra,
            chat_id,
            if recording {
                "chatActionRecordingVoiceNote"
            } else {
                "chatActionCancel"
            },
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.outgoing_voice = recording.then_some(OutgoingTyping {
                    chat_id,
                    last_sent_ms: now_ms,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Composer activity → `sendChatAction`.
    ///
    /// Unigram `ChatTextBox.OnTextChanged` calls `SetTyping(ChatActionTyping)`
    /// while the field is non-empty, at most every 4s. tdesktop
    /// `HistoryWidget::fieldChanged` does the same only when the field has
    /// sendable text and the user is not editing. Empty text, edit mode, send,
    /// and leaving the chat send `chatActionCancel` (Unigram `CancelTyping`;
    /// tdesktop stops the action with progress `-1` on chat close and on send).
    pub fn sync_outgoing_typing(
        &mut self,
        text: &str,
        editing: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let composing = !editing && !text.trim().is_empty();
        let chat_id = self
            .session
            .open_chat
            .filter(|id| self.typing_chat_allowed(*id));
        let Some(chat_id) = chat_id.filter(|_| composing) else {
            return self.cancel_outgoing_typing();
        };
        if let Some(prev) = &self.outgoing_typing {
            if prev.chat_id != chat_id {
                self.cancel_outgoing_typing()?;
            } else if now_ms.saturating_sub(prev.last_sent_ms) < OUTGOING_TYPING_INTERVAL_MS {
                return Ok(());
            }
        }
        self.send_outgoing_action(chat_id, true, now_ms)
    }

    fn typing_chat_allowed(&self, chat_id: ChatId) -> bool {
        self.chats_path_active()
            && self
                .session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| chat.supported())
    }

    fn cancel_outgoing_typing(&mut self) -> Result<(), ConnectSendError> {
        let Some(prev) = self.outgoing_typing.take() else {
            return Ok(());
        };
        if !self.typing_chat_allowed(prev.chat_id) {
            return Ok(());
        }
        self.send_outgoing_action(prev.chat_id, false, 0)
    }

    fn send_outgoing_action(
        &mut self,
        chat_id: ChatId,
        typing: bool,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let json = send_chat_action(extra, chat_id, typing);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.outgoing_typing = typing.then_some(OutgoingTyping {
                    chat_id,
                    last_sent_ms: now_ms,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Save an own-message edit via `editMessageText` or `editMessageCaption`.
    pub fn edit_snapshot(
        &mut self,
        edit: &ComposerEdit,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&edit.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&edit.chat_id.0)
            .and_then(|history| history.messages.get(&edit.message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if ComposerEdit::from_own_content(
            message.chat_id,
            message.id,
            message.is_outgoing,
            message.pending,
            &message.content,
        )
        .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = text.trim();
        if matches!(edit.kind, ComposerEditKind::Text) && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::EditMessage, Some(edit.chat_id));
        let json = match edit.kind {
            ComposerEditKind::Text => {
                edit_message_text(extra, edit.chat_id, edit.message_id, caption)
            }
            ComposerEditKind::Caption => {
                edit_message_caption(extra, edit.chat_id, edit.message_id, caption, false)
            }
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// After UI confirm (tdesktop `DeleteMessagesBox`), send `deleteMessages`.
    /// `revoke: true` matches official desktop default for own outgoing.
    pub fn delete_confirmed(
        &mut self,
        confirm: &DeleteConfirm,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&confirm.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&confirm.chat_id.0)
            .and_then(|history| history.messages.get(&confirm.message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if DeleteConfirm::own(
            message.chat_id,
            message.id,
            message.is_outgoing,
            message.pending,
        )
        .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(confirm.chat_id));
        let json = delete_messages(extra, confirm.chat_id, &[confirm.message_id], true);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `forwardMessages` after the dest picker chooses a supported chat.
    /// `send_copy: false` preserves official "Forwarded from" attribution.
    pub fn forward_messages(
        &mut self,
        dest: ChatId,
        draft: &ForwardDraft,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if draft.is_empty() || draft.message_ids.len() > 100 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let dest_ok = self
            .session
            .chats
            .get(&dest.0)
            .is_some_and(|chat| chat.supported());
        let from_ok = self
            .session
            .chats
            .get(&draft.from_chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !dest_ok || !from_ok {
            return Err(ConnectSendError::InvalidRequest);
        }
        for id in &draft.message_ids {
            let Some(message) = self
                .session
                .histories
                .get(&draft.from_chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
            else {
                return Err(ConnectSendError::InvalidRequest);
            };
            if message.pending || id.0 <= 0 {
                return Err(ConnectSendError::InvalidRequest);
            }
        }
        let extra = self
            .session
            .request(RequestPurpose::ForwardMessages, Some(dest));
        self.session.in_flight_forward = Some(ForwardFlight {
            extra,
            dest_chat_id: dest,
            from_chat_id: draft.from_chat_id,
            requested: draft.message_ids.len(),
        });
        let json = forward_messages(extra, dest, draft.from_chat_id, &draft.message_ids);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.in_flight_forward = None;
                Err(err)
            }
        }
    }

    /// Add an emoji reaction (tdesktop InlineList / Unigram ReactionButton).
    /// `is_big: false`, `update_recent_reactions: true` — official picker click.
    pub fn add_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_message_reaction(chat_id, message_id, emoji, false)
    }

    /// Remove the current user's chosen emoji reaction.
    pub fn remove_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_message_reaction(chat_id, message_id, emoji, true)
    }

    /// Chip / picker toggle: chosen → `removeMessageReaction`, else add.
    pub fn toggle_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let chosen = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.chosen_emoji(emoji));
        self.send_message_reaction(chat_id, message_id, emoji, chosen)
    }

    fn send_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
        remove: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if emoji.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !message.can_react() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if remove {
            RequestPurpose::RemoveMessageReaction
        } else {
            RequestPurpose::AddMessageReaction
        };
        let extra = self.session.request(purpose, Some(chat_id));
        let json = if remove {
            remove_message_reaction(extra, chat_id, message_id, emoji)
        } else {
            add_message_reaction(extra, chat_id, message_id, emoji, false, true)
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Pin a history message (tdesktop / Unigram Pin). Official defaults:
    /// `disable_notification` false, `only_for_self` false.
    pub fn pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, false)
    }

    /// Unpin one pinned message (tdesktop PinnedBar cancel / Unpin).
    pub fn unpin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, true)
    }

    /// Toggle pin for an already-sent message.
    pub fn toggle_pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        let pinned = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_pinned);
        self.send_pin_chat_message(chat_id, message_id, pinned)
    }

    fn send_pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        unpin: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !message.can_pin() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if unpin {
            RequestPurpose::UnpinChatMessage
        } else {
            RequestPurpose::PinChatMessage
        };
        let extra = self.session.request(purpose, Some(chat_id));
        let json = if unpin {
            unpin_chat_message(extra, chat_id, message_id)
        } else {
            pin_chat_message(extra, chat_id, message_id, false, false)
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 4.2: `setPollAnswer` for a poll-row tap. Guards mirror the other
    /// send methods: chats path active, supported chat, real non-pending
    /// message whose content is a votable `messagePoll`. The tap resolves to
    /// the full answer set via `poll_answer_for_tap` (tdesktop-style
    /// toggle/retract semantics); `None` there means no-op (closed poll,
    /// unchanged answer) and no request goes out. On send, the chosen marks
    /// flip locally right away; the server's `updatePoll` corrects the
    /// counts/percentages in place.
    pub fn send_poll_answer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let poll = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .filter(|message| !message.pending && message.id.0 > 0)
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            })
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !poll.can_vote() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let option_ids =
            poll_answer_for_tap(&poll, option_index).ok_or(ConnectSendError::InvalidRequest)?;
        // Capture the previous chosen marks: the optimistic flip below is
        // rolled back if the send fails (the server's `updatePoll` corrects
        // counts/percentages in place on success).
        let mut previous: Vec<bool> = Vec::new();
        if let Some(history) = self.session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::Poll(poll_content) = &mut message.content
        {
            let chosen: std::collections::HashSet<i32> = option_ids.iter().copied().collect();
            for (index, option) in poll_content.poll.options.iter_mut().enumerate() {
                previous.push(option.is_chosen);
                option.is_chosen = chosen.contains(&(index as i32));
            }
        }
        let extra = self
            .session
            .request(RequestPurpose::SetPollAnswer, Some(chat_id));
        let json = set_poll_answer(extra, chat_id, message_id, &option_ids);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                // The vote never left the client: restore the previous marks.
                if let Some(history) = self.session.histories.get_mut(&chat_id.0)
                    && let Some(message) = history.messages.get_mut(&message_id.0)
                    && let MessageContent::Poll(poll_content) = &mut message.content
                {
                    for (option, &was_chosen) in
                        poll_content.poll.options.iter_mut().zip(previous.iter())
                    {
                        option.is_chosen = was_chosen;
                    }
                }
                Err(err)
            }
        }
    }

    /// Phase 4.2: create a poll from the composer dialog via `sendMessage` +
    /// `inputMessagePoll` (TDLib 1.8.67). Guards: chats path active, chat can
    /// post (admin-gated channels, same as `send_snapshot`), valid draft
    /// (`PollDraft::validate`). Quiz polls are created as regular
    /// (`inputPollTypeRegular`) — quiz creation stays out of this slice.
    pub fn send_poll_draft(
        &mut self,
        chat_id: ChatId,
        draft: &PollDraft,
        reply_to: Option<MessageId>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if draft.validate().is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        let question = draft.question.trim().to_string();
        let options: Vec<String> = draft
            .usable_options()
            .into_iter()
            .map(str::to_string)
            .collect();
        let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let json = send_poll(
            extra,
            chat_id,
            PollSend {
                question: &question,
                options: &option_refs,
                is_anonymous: draft.is_anonymous,
                allows_multiple_answers: draft.allows_multiple_answers,
                reply_to,
            },
        );
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
    /// notification settings and clears `use_default_mute_for` (Unigram).
    pub fn set_chat_mute_for(
        &mut self,
        chat_id: ChatId,
        mute_for: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let settings = chat.notification_settings.clone().with_mute_for(mute_for);
        self.send_notification_settings(chat_id, &settings)
    }

    /// Unmute (`mute_for` 0, not "use default").
    pub fn unmute_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.set_chat_mute_for(chat_id, 0)
    }

    /// Mute forever (`i32::MAX`, tdesktop `kMuteForeverValue`).
    pub fn mute_chat_forever(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.set_chat_mute_for(chat_id, MUTE_FOREVER)
    }

    fn send_notification_settings(
        &mut self,
        chat_id: ChatId,
        settings: &ChatNotificationSettings,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SetChatNotificationSettings, Some(chat_id));
        let json = set_chat_notification_settings(extra, chat_id, settings);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Move the chat to `chatListArchive` (`addChatToList`).
    pub fn archive_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.send_add_chat_to_list(chat_id, true)
    }

    /// Move the chat back to `chatListMain` (`addChatToList`).
    pub fn unarchive_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.send_add_chat_to_list(chat_id, false)
    }

    fn send_add_chat_to_list(
        &mut self,
        chat_id: ChatId,
        archive: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddChatToList, Some(chat_id));
        let json = add_chat_to_list(extra, chat_id, archive);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `setAuthenticationPhoneNumber` when auth is WaitPhoneNumber.
    /// Phone value is never stored on the session or diagnostics.
    pub fn submit_phone(&mut self, phone: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPhoneNumber) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let phone = phone.trim();
        if phone.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self.session.request(RequestPurpose::SetPhoneNumber, None);
        self.sender
            .send_json(&set_authentication_phone_number(extra, phone))?;
        Ok(extra)
    }

    /// Send `checkAuthenticationCode` when auth is WaitCode.
    /// The code is never stored on the session or diagnostics.
    pub fn submit_code(&mut self, code: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitCode { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let code = code.trim();
        if code.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckAuthenticationCode, None);
        self.sender
            .send_json(&check_authentication_code(extra, code))?;
        Ok(extra)
    }

    /// Send `checkAuthenticationPassword` when auth is WaitPassword.
    /// The password is never stored on the session or diagnostics. Not trimmed
    /// (leading/trailing spaces can be significant).
    pub fn submit_password(&mut self, password: &str) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::WaitPassword { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.last_auth_error = None;
        let extra = self
            .session
            .request(RequestPurpose::CheckAuthenticationPassword, None);
        self.sender
            .send_json(&check_authentication_password(extra, password))?;
        Ok(extra)
    }

    pub fn open_search(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.clear_typed_debounce();
        if self.session.search.open && !self.session.search.query.is_empty() {
            return Ok(None);
        }
        if self.session.search.open
            && self.session.search.recents
            && matches!(
                self.session.search.status,
                SearchStatus::Searching | SearchStatus::Ready | SearchStatus::Idle
            )
        {
            return Ok(None);
        }
        self.request_recents()
    }

    pub fn close_search(&mut self) {
        self.clear_typed_debounce();
        self.session.close_search();
    }

    /// Empty query: `searchRecentlyFoundChats` immediately (official Recent).
    /// Non-empty: debounce, then `searchChats` + `searchMessages` (`chat_list` null).
    pub fn set_search_query(
        &mut self,
        query: &str,
    ) -> Result<SearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.clear_typed_debounce();
            if self.session.search.open
                && self.session.search.query.is_empty()
                && self.session.search.recents
                && matches!(
                    self.session.search.status,
                    SearchStatus::Searching | SearchStatus::Ready | SearchStatus::Idle
                )
            {
                return Ok(SearchQueryOutcome::Unchanged);
            }
            return Ok(match self.request_recents()? {
                Some(flight) => SearchQueryOutcome::Sent(flight),
                None => SearchQueryOutcome::Unchanged,
            });
        }
        if self
            .pending_typed_search
            .as_ref()
            .is_some_and(|(_, q)| q == trimmed)
            && self.session.search.query == trimmed
        {
            return Ok(SearchQueryOutcome::Unchanged);
        }
        if self.pending_typed_search.is_none()
            && self.session.search.query == trimmed
            && !self.session.search.recents
            && matches!(
                self.session.search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(SearchQueryOutcome::Unchanged);
        }
        let _search_gen = self.session.search.begin_query(trimmed);
        self.search_debounce_token = self.search_debounce_token.saturating_add(1);
        let token = self.search_debounce_token;
        self.pending_typed_search = Some((token, trimmed.to_string()));
        Ok(SearchQueryOutcome::Debounced { token })
    }

    /// Send the settled typed query if `token` is still the latest debounce.
    pub fn commit_debounced_search(
        &mut self,
        token: u64,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        let Some((pending_token, query)) = self.pending_typed_search.clone() else {
            return Ok(None);
        };
        if pending_token != token {
            return Ok(None);
        }
        self.pending_typed_search = None;
        self.send_typed_search(&query)
    }

    fn clear_typed_debounce(&mut self) {
        self.pending_typed_search = None;
        self.search_debounce_token = self.search_debounce_token.saturating_add(1);
    }

    fn send_typed_search(
        &mut self,
        trimmed: &str,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        let search_gen = self.session.search.generation;
        let chats_extra = self
            .session
            .request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = self
            .session
            .request_search(RequestPurpose::SearchMessages, search_gen);
        match self
            .sender
            .send_json(&search_chats(chats_extra, trimmed, SEARCH_LIMIT))
        {
            Ok(()) => {}
            Err(err) => {
                self.session.requests.take(chats_extra);
                self.session.requests.take(messages_extra);
                self.session.search.accept_chats(Vec::new(), true);
                self.session.search.accept_messages(Vec::new(), true);
                return Err(err);
            }
        }
        match self
            .sender
            .send_json(&search_messages(messages_extra, trimmed, SEARCH_LIMIT))
        {
            Ok(()) => Ok(Some(SearchFlight::Query(chats_extra, messages_extra))),
            Err(err) => {
                self.session.requests.take(messages_extra);
                self.session.search.accept_messages(Vec::new(), true);
                Err(err)
            }
        }
    }

    fn request_recents(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        let search_gen = self.session.search.begin_recents();
        let extra = self
            .session
            .request_search(RequestPurpose::SearchRecentlyFoundChats, search_gen);
        match self
            .sender
            .send_json(&search_recently_found_chats(extra, "", RECENT_SEARCH_LIMIT))
        {
            Ok(()) => Ok(Some(SearchFlight::Recents(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.search.accept_chats(Vec::new(), true);
                Err(err)
            }
        }
    }

    fn remember_found_chat(&mut self, chat_id: ChatId) {
        let extra = self
            .session
            .request(RequestPurpose::AddRecentlyFoundChat, Some(chat_id));
        if self
            .sender
            .send_json(&add_recently_found_chat(extra, chat_id))
            .is_err()
        {
            self.session.requests.take(extra);
        }
    }

    /// Open a chat from search via `addRecentlyFoundChat` then `openChat`.
    /// Flushes the leaving composer's draft before `select_chat` drops `pending_draft`.
    pub fn select_search_chat(
        &mut self,
        chat_id: ChatId,
        leaving_text: &str,
        leaving_reply: Option<MessageId>,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.flush_leaving_composer(chat_id, leaving_text, leaving_reply, now_ms)?;
        self.remember_found_chat(chat_id);
        self.session.close_search();
        self.select_chat(chat_id)
    }

    /// Jump to a found message: upsert it into history, then `select_chat`.
    /// Same flush-before-drop as [`Self::select_search_chat`].
    pub fn select_search_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        leaving_text: &str,
        leaving_reply: Option<MessageId>,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.flush_leaving_composer(chat_id, leaving_text, leaving_reply, now_ms)?;
        self.remember_found_chat(chat_id);
        self.session.promote_search_message(chat_id, message_id);
        self.session.close_search();
        self.select_chat(chat_id)
    }

    /// Persist the open chat's composer before a search result switches chats.
    fn flush_leaving_composer(
        &mut self,
        next_chat: ChatId,
        leaving_text: &str,
        leaving_reply: Option<MessageId>,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        let Some(prev) = self.session.open_chat else {
            return Ok(());
        };
        if prev == next_chat {
            return Ok(());
        }
        self.note_composer_draft(prev, leaving_text, leaving_reply, now_ms, false)?;
        Ok(())
    }

    /// tdesktop `searchInChat` when history is focused (`Command::Search` / Ctrl+F).
    pub fn open_chat_search(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(self.session.open_chat_search())
    }

    pub fn close_chat_search(&mut self) {
        self.clear_chat_search_debounce();
        self.session.close_chat_search();
    }

    /// Empty query: clear immediately (tdesktop ComposeSearch skips empty).
    /// Non-empty: debounce `AutoSearchTimeout` (900 ms), then `searchChatMessages`.
    pub fn set_chat_search_query(
        &mut self,
        query: &str,
    ) -> Result<ChatSearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_search.open && !self.session.open_chat_search() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.clear_chat_search_debounce();
            if self.session.chat_search.query.is_empty()
                && matches!(
                    self.session.chat_search.status,
                    SearchStatus::Idle | SearchStatus::Closed
                )
            {
                return Ok(ChatSearchQueryOutcome::Unchanged);
            }
            self.session.chat_search.clear_query();
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self
            .pending_typed_chat_search
            .as_ref()
            .is_some_and(|(_, q)| q == trimmed)
            && self.session.chat_search.query == trimmed
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self.pending_typed_chat_search.is_none()
            && self.session.chat_search.query == trimmed
            && matches!(
                self.session.chat_search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        let _search_gen = self.session.chat_search.begin_query(trimmed);
        self.chat_search_debounce_token = self.chat_search_debounce_token.saturating_add(1);
        let token = self.chat_search_debounce_token;
        self.pending_typed_chat_search = Some((token, trimmed.to_string()));
        Ok(ChatSearchQueryOutcome::Debounced { token })
    }

    pub fn commit_debounced_chat_search(
        &mut self,
        token: u64,
    ) -> Result<Option<ChatSearchFlight>, ConnectSendError> {
        let Some((pending_token, query)) = self.pending_typed_chat_search.clone() else {
            return Ok(None);
        };
        if pending_token != token {
            return Ok(None);
        }
        self.pending_typed_chat_search = None;
        self.send_chat_search(&query)
    }

    fn clear_chat_search_debounce(&mut self) {
        self.pending_typed_chat_search = None;
        self.chat_search_debounce_token = self.chat_search_debounce_token.saturating_add(1);
    }

    fn send_chat_search(
        &mut self,
        trimmed: &str,
    ) -> Result<Option<ChatSearchFlight>, ConnectSendError> {
        let Some(chat_id) = self.session.chat_search.chat_id.or(self.session.open_chat) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let search_gen = self.session.chat_search.generation;
        let extra = self.session.request_chat_search(
            RequestPurpose::SearchChatMessages,
            chat_id,
            search_gen,
        );
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            trimmed,
            MessageId(0),
            0,
            CHAT_SEARCH_LIMIT,
        )) {
            Ok(()) => Ok(Some(ChatSearchFlight::Query(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .chat_search
                    .accept_hits(Vec::new(), 0, MessageId(0), true);
                Err(err)
            }
        }
    }

    pub fn jump_to_chat_search_message(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        match self.session.begin_chat_search_jump(message_id) {
            ChatSearchJumpNeed::AlreadyReady | ChatSearchJumpNeed::Missing => Ok(None),
            ChatSearchJumpNeed::LoadAround => self.fetch_history_around(message_id),
        }
    }

    /// Quote-strip activation: same Unigram `LoadMessageSliceImpl` around-load
    /// + highlight pipeline as in-chat search jump (`getChatHistory` offset -25).
    pub fn jump_to_replied_message(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.jump_to_chat_search_message(message_id)
    }

    pub fn jump_selected_chat_search_hit(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self
            .session
            .chat_search
            .selected_hit()
            .map(|hit| hit.message_id)
        else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_newer(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self.session.chat_search.select_newer() else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_older(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self.session.chat_search.select_older() else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    /// Unigram `GetChatHistory(chatId, maxId, -25, 50)` around the jump target.
    fn fetch_history_around(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self.session.request_history_around(chat_id, message_id);
        match self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            message_id,
            HISTORY_AROUND_OFFSET,
            HISTORY_AROUND_LIMIT,
            false,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.chat_search.jump =
                    crate::state::ChatSearchJump::Missing { message_id };
                Err(err)
            }
        }
    }

    /// Send `close` (not `logOut`). Callers must keep receiving until Closed.
    pub fn request_close(&mut self) -> Result<RequestId, ConnectSendError> {
        if matches!(
            self.session.auth,
            AuthorizationState::Closed | AuthorizationState::Closing
        ) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_close();
        let extra = self.session.request(RequestPurpose::Close, None);
        self.sender.send_json(&close_request(extra))?;
        Ok(extra)
    }
}

/// How long Drop / `--connect-smoke` waits for `authorizationStateClosed`.
pub const CLIENT_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Send `close` and ingest until Closed. Does not join a receive thread or
/// unload tdjson. Returns whether Closed was observed.
pub fn wait_closed<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    mut recv: impl FnMut(Duration) -> Option<OwnedEnvelope>,
    timeout: Duration,
) -> bool {
    if matches!(driver.session.auth, AuthorizationState::Closed) {
        return true;
    }
    let _ = driver.request_close();
    let deadline = Instant::now() + timeout;
    loop {
        if matches!(driver.session.auth, AuthorizationState::Closed) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let slice = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(50));
        if let Some(owned) = recv(slice) {
            let _ = driver.ingest(owned);
        }
    }
}

/// Open LiveTdJson + receive bridge when gate + restore succeed.
///
/// Drop / [`LiveConnect::shutdown`] send `close`, wait for
/// `authorizationStateClosed`, then join the receive thread **before**
/// `libtdjson` is unloaded. Unloading while TDLib worker threads are still
/// running is what produced SIGSEGV (exit 139) after `--connect-smoke`.
///
/// Field order: `bridge` is dropped before `_live` (declaration order) so
/// `td_receive` is not in-flight during `dlclose`.
pub struct LiveConnect {
    pub driver: ConnectDriver<LiveSender>,
    pub bridge: ReceiveBridge,
    _live: LiveTdJson,
}

impl LiveConnect {
    /// Close the TDLib client and join the receive thread. Safe to call twice.
    /// Does not panic; a timeout still joins the thread so Drop can unload.
    pub fn shutdown(&mut self, wait: Duration) {
        if self.bridge.is_joined() {
            return;
        }
        let _ = wait_closed(
            &mut self.driver,
            |timeout| self.bridge.next_timeout(timeout),
            wait,
        );
        self.bridge.shutdown();
    }
}

impl Drop for LiveConnect {
    fn drop(&mut self) {
        self.shutdown(CLIENT_CLOSE_TIMEOUT);
    }
}

pub fn start_live_connect(
    credentials: TelegramCredentials,
    store: &(impl SecretStore + ?Sized),
    diagnostics: Arc<dyn DiagnosticSink>,
) -> Result<LiveConnect, ConnectBlocker> {
    match evaluate_gate(true) {
        ConnectGate::Blocked(b) => return Err(b),
        ConnectGate::Ready { .. } => {}
    }
    let app_root = default_app_root();
    let prepared = prepare_connect(&app_root, AccountKey::primary(), store, &credentials)?;
    let live = LiveTdJson::connect().map_err(|e| match e {
        TdJsonError::NotFound => ConnectBlocker::MissingTdjson,
        _ => ConnectBlocker::TdjsonLoad,
    })?;
    let sender = LiveSender::from_live(&live);
    let bridge = ReceiveBridge::spawn_live(live.api.clone(), diagnostics.clone());
    let session = Session::new(prepared.account.clone(), diagnostics);
    let mut driver = ConnectDriver::new(session, sender, credentials, prepared);
    driver.kickoff().map_err(|_| ConnectBlocker::TdjsonLoad)?;
    Ok(LiveConnect {
        driver,
        bridge,
        _live: live,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::platform::MemorySecretStore;
    use crate::telegram::client::copy_and_parse;
    use serde_json::Value;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::AtomicU64;

    static TDJSON_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn test_credentials() -> TelegramCredentials {
        TelegramCredentials {
            api_id: 99,
            api_hash: "unit-test-hash-not-for-network".into(),
        }
    }

    fn prepared_tmp(store: &MemorySecretStore) -> (std::path::PathBuf, PreparedConnect) {
        let dir = std::env::temp_dir().join(format!(
            "quill-connect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let prepared = prepare_connect(&dir, AccountKey::primary(), store, &test_credentials())
            .expect("prepare");
        (dir, prepared)
    }

    #[test]
    fn gate_blocks_without_credentials() {
        assert_eq!(
            evaluate_gate(false),
            ConnectGate::Blocked(ConnectBlocker::MissingCredentials)
        );
    }

    #[test]
    fn gate_blocks_without_tdjson_when_credentials_present() {
        let _lock = TDJSON_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var("QUILL_TDJSON_PATH").ok();
        // SAFETY: exclusive lock; restored below.
        unsafe { std::env::remove_var("QUILL_TDJSON_PATH") };
        let gate = evaluate_gate(true);
        if let Some(v) = previous {
            unsafe { std::env::set_var("QUILL_TDJSON_PATH", v) };
        }
        assert_eq!(gate, ConnectGate::Blocked(ConnectBlocker::MissingTdjson));
        assert!(
            ConnectBlocker::MissingTdjson
                .user_message()
                .contains("QUILL_TDJSON_PATH")
        );
        assert!(
            ConnectBlocker::MissingTdjson
                .user_message()
                .contains("native-bundle")
        );
    }

    #[test]
    fn set_tdlib_parameters_shape_includes_hash_but_debug_redacts_credentials() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let creds = test_credentials();
        let params = build_set_tdlib_parameters(&creds, &prepared.paths, &prepared.database_key);
        let json = params.to_json(RequestId(7));
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setTdlibParameters");
        assert_eq!(v["@extra"], "7");
        assert_eq!(v["api_id"], 99);
        assert_eq!(v["api_hash"], "unit-test-hash-not-for-network");
        assert_eq!(v["use_secret_chats"], false);
        assert_eq!(v["use_file_database"], true);
        assert!(v["database_directory"].as_str().unwrap().contains("tdlib"));
        assert!(!v["database_encryption_key"].as_str().unwrap().is_empty());
        let debug = format!("{creds:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("unit-test-hash"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_sends_parameters_then_reaches_wait_phone_via_injection() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let kick = driver.kickoff().unwrap();
        assert_eq!(kick.0, 1);

        let seq = AtomicU64::new(0);
        let wait_params = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_params).unwrap();
        assert!(driver.parameters_sent());

        let sent = recorder.snapshot();
        assert_eq!(sent.len(), 2); // getAuthorizationState + setTdlibParameters
        assert!(sent[0].contains("getAuthorizationState"));
        assert!(sent[1].contains("setTdlibParameters"));
        assert!(sent[1].contains("\"api_id\":99"));
        assert!(sent[1].contains("unit-test-hash-not-for-network"));
        assert!(!sink.rendered().contains("unit-test-hash"));

        let ok = copy_and_parse(r#"{"@type":"ok","@extra":"2"}"#, &seq, &dyn_sink).unwrap();
        driver.ingest(ok).unwrap();
        let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_phone).unwrap();
        assert!(matches!(
            driver.session.auth,
            AuthorizationState::WaitPhoneNumber
        ));
        assert_eq!(
            driver.session.auth_view.action,
            crate::auth::AuthAction::EnterPhone
        );

        let phone_extra = driver.submit_phone("+15551212").unwrap();
        let sent = recorder.snapshot();
        let phone_json = sent.last().unwrap();
        assert!(phone_json.contains("setAuthenticationPhoneNumber"));
        assert!(phone_json.contains(&format!("\"@extra\":\"{}\"", phone_extra.0)));
        assert!(phone_json.contains("+15551212"));
        assert!(!sink.rendered().contains("+15551212"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_submits_code_and_password_only_in_matching_states() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        assert_eq!(
            driver.submit_code("12345"),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.submit_password("secret"),
            Err(ConnectSendError::InvalidRequest)
        );

        let seq = AtomicU64::new(0);
        let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_code).unwrap();
        assert!(matches!(
            driver.session.auth,
            AuthorizationState::WaitCode {
                code_length: Some(5)
            }
        ));
        assert_eq!(
            driver.submit_password("secret"),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(
            driver.submit_code("  "),
            Err(ConnectSendError::InvalidRequest)
        );
        let code_extra = driver.submit_code("  12345 ").unwrap();
        let sent = recorder.snapshot();
        let code_json = sent.last().unwrap();
        assert!(code_json.contains("checkAuthenticationCode"));
        assert!(code_json.contains(&format!("\"@extra\":\"{}\"", code_extra.0)));
        assert!(code_json.contains("\"code\":\"12345\""));
        assert!(!sink.rendered().contains("12345"));

        let wait_password = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPassword","password_hint":"CANARY_HINT","has_recovery_email_address":true}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_password).unwrap();
        assert_eq!(
            driver.submit_code("12345"),
            Err(ConnectSendError::InvalidRequest)
        );
        let pw_extra = driver.submit_password(" unit-pw ").unwrap();
        let sent = recorder.snapshot();
        let pw_json = sent.last().unwrap();
        assert!(pw_json.contains("checkAuthenticationPassword"));
        assert!(pw_json.contains(&format!("\"@extra\":\"{}\"", pw_extra.0)));
        // Password is not trimmed.
        assert!(pw_json.contains("\"password\":\" unit-pw \""));
        assert!(!sink.rendered().contains("unit-pw"));
        assert!(!sink.rendered().contains("CANARY_HINT"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_closed_sends_close_and_reaches_closed_via_injection() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        let seq = AtomicU64::new(0);
        let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(wait_phone).unwrap();

        let envelopes = vec![
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosing"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        ];
        let mut iter = envelopes.into_iter();
        assert!(wait_closed(
            &mut driver,
            |_| iter.next(),
            Duration::from_secs(2)
        ));
        assert!(matches!(driver.session.auth, AuthorizationState::Closed));
        let sent = recorder.snapshot();
        let close_json = sent.last().expect("close request");
        assert!(close_json.contains("\"@type\":\"close\""));
        assert!(!sink.rendered().contains("unit-test-hash"));
        // Second call is a no-op once Closed (no extra send).
        let before = sent.len();
        assert!(wait_closed(&mut driver, |_| None, Duration::ZERO));
        assert_eq!(recorder.snapshot().len(), before);
        drop(driver);
        drop(recorder);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_tdjson_message_is_actionable() {
        let msg = ConnectBlocker::MissingTdjson.user_message();
        assert!(msg.contains("QUILL_TDJSON_PATH"));
        assert!(msg.contains("never searched"));
    }

    #[test]
    fn driver_loads_chats_after_ready_then_send_text() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

        assert_eq!(
            driver.select_chat(ChatId(1)),
            Err(ConnectSendError::InvalidRequest)
        );

        let seq = AtomicU64::new(0);
        let ready = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(ready).unwrap();
        assert!(matches!(driver.session.auth, AuthorizationState::Ready));

        let sent = recorder.snapshot();
        let load = sent.last().expect("loadChats after Ready");
        assert!(load.contains("\"@type\":\"loadChats\""));
        assert!(load.contains("chatListMain"));
        assert!(load.contains(&format!("\"limit\":{MAIN_CHAT_LOAD_LIMIT}")));
        let load_extra = driver
            .session
            .requests
            .has_purpose(RequestPurpose::LoadChats);
        assert!(load_extra);

        let new_chat = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(new_chat).unwrap();
        let position = copy_and_parse(
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"12","is_pinned":false}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(position).unwrap();
        assert_eq!(driver.session.ordered_chats()[0].id.0, 7);

        // In-flight loadChats: ingest of unrelated updates must not send another page.
        let loads_before = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert_eq!(loads_before, 1);

        assert!(
            driver
                .session
                .requests
                .has_purpose(RequestPurpose::LoadChats)
        );
        let load_ok = copy_and_parse(r#"{"@type":"ok","@extra":"1"}"#, &seq, &dyn_sink).unwrap();
        driver.ingest(load_ok).unwrap();
        // ok on loadChats means more may exist — one continuation page, not a per-tick loop.
        let loads_after_ok = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert_eq!(loads_after_ok, 2);

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateUpdating"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            recorder
                .snapshot()
                .iter()
                .filter(|j| j.contains("loadChats"))
                .count(),
            2,
            "unrelated ingest must not re-page loadChats"
        );

        let last_load = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains("loadChats"))
            .unwrap();
        let v: Value = serde_json::from_str(&last_load).unwrap();
        let extra = v["@extra"].as_str().unwrap();
        let err404 = copy_and_parse(
            &format!(r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{extra}"}}"#),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(err404).unwrap();
        assert!(driver.session.chats_exhausted);
        let loads_done = recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count();
        assert_eq!(loads_done, 2);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            recorder
                .snapshot()
                .iter()
                .filter(|j| j.contains("loadChats"))
                .count(),
            loads_done
        );
        assert!(!sink.rendered().contains("Not Found"));

        let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":7")),
            "select_chat must send openChat"
        );
        let history_json = sent.last().unwrap();
        assert!(history_json.contains("getChatHistory"));
        assert!(history_json.contains("\"chat_id\":7"));
        assert!(history_json.contains(&format!("\"@extra\":\"{}\"", history_extra.0)));

        let messages = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":11,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                history_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(messages).unwrap();
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .messages
                .contains_key(&11)
        );
        let view_json = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains("viewMessages"))
            .expect("viewMessages after history");
        let view: Value = serde_json::from_str(&view_json).unwrap();
        assert_eq!(view["@type"], "viewMessages");
        assert_eq!(view["chat_id"], 7);
        assert_eq!(view["message_ids"], serde_json::json!([11]));
        assert_eq!(view["source"]["@type"], "messageSourceChatHistory");
        assert_eq!(view["force_read"], true);
        assert!(!view_json.contains("hi"));

        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_SEND_ping",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"topic_id\":null"));
        assert!(send_json.contains("CANARY_SEND_ping"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));
        assert!(!sink.rendered().contains("CANARY_SEND"));

        let channel_no_post = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(channel_no_post).unwrap();
        let channel_snap = crate::composer::ComposerSnapshot::capture(
            ChatId(8),
            driver.session.view_generation,
            "nope",
        );
        assert_eq!(
            driver.send_text_snapshot(&channel_snap),
            Err(ConnectSendError::InvalidRequest)
        );
        assert!(!sink.rendered().contains("CANARY_SEND"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn channel_admin_send_succeeds_non_admin_send_rejected() {
        // Phase 2.3: the driver gate mirrors the composer gate — an admin
        // channel sends `sendMessage` with the channel chat_id; a channel
        // without posting rights rejects like a hidden composer.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Admin channel (id 9) and plain-member channel (id 10).
        for (id, title) in [(9, "Admin news"), (10, "Member news")] {
            driver
                .ingest(
                    copy_and_parse(
                        &format!(
                            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":true}},"unread_count":0}}}}"#
                        ),
                        &seq,
                        &dyn_sink,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        // getMe + getChatMember leave the viewer as an admin with the posting
        // right in channel 9.
        driver.session.my_user_id = Some(777);
        let me_extra = driver.session.request(RequestPurpose::GetMe, None);
        let admin_extra = driver
            .session
            .request(RequestPurpose::GetChatMember, Some(ChatId(9)));
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"user","@extra":"{}","id":777}}"#, me_extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":777}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":true}}}}}}"#,
                        admin_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chats.get(&9).unwrap().can_post());

        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(9),
            driver.session.view_generation,
            "CANARY_ADMIN_post",
        );
        let send_extra = driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        let send_json = sent.last().unwrap();
        assert!(send_json.contains("sendMessage"));
        assert!(send_json.contains("\"chat_id\":9"));
        assert!(send_json.contains("CANARY_ADMIN_post"));
        assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));

        // Channel 10: membership unknown → composer hidden, send rejected.
        assert!(!driver.session.chats.get(&10).unwrap().can_post());
        let member_snap = crate::composer::ComposerSnapshot::capture(
            ChatId(10),
            driver.session.view_generation,
            "nope",
        );
        assert_eq!(
            driver.send_text_snapshot(&member_snap),
            Err(ConnectSendError::InvalidRequest)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bot_info_fetched_once_on_chat_open() {
        // Phase 3.1: opening a bot chat lazily sends `getUserFullInfo` once;
        // the `userFullInfo` response populates the cache and suppresses
        // refetches. Non-bot chats never trigger the fetch.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Bot private chat (id 21) and a regular private chat (id 22).
        for json in [
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":22},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // The bot chat rides the ordinary private-chat path: no gate, and
        // the composer is shown.
        let bot_chat = driver.session.chats.get(&21).unwrap();
        assert!(bot_chat.supported());
        assert!(bot_chat.kind.gate_reason().is_none());
        assert!(bot_chat.can_post());

        driver.select_chat(ChatId(21)).unwrap();
        let info_fetches = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|j| j.contains("\"@type\":\"getUserFullInfo\""))
                .collect::<Vec<_>>()
        };
        let first = info_fetches();
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("\"user_id\":21"));
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));

        // Re-selecting while the fetch is in flight sends nothing new.
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(info_fetches().len(), 1);

        // The response populates the cache; further opens stay quiet.
        let extra = extra.expect("getUserFullInfo in flight");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"s","description":"CANARY_bot_desc","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}}]}}}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let info = driver
            .session
            .bot_info_for_chat(ChatId(21))
            .expect("bot info cached");
        assert_eq!(info.description, "CANARY_bot_desc");
        assert_eq!(info.commands.len(), 1);
        assert_eq!(info.commands[0].command, "start");
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(info_fetches().len(), 1);

        // A regular private chat never triggers the fetch.
        driver.select_chat(ChatId(22)).unwrap();
        assert_eq!(info_fetches().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bot_commands_fetched_once_on_chat_open() {
        // Phase 3.3: opening a bot chat lazily sends `getCommands` once
        // (null scope selects the default scope, schema 1.8.67 line
        // 14953). The `botCommands` response populates the cache and
        // merges below the `botInfo` commands in `command_menu_items`;
        // an `error` answer is recorded as an empty set so the fetch is
        // never retried. Non-bot chats never trigger the fetch.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        // Bot private chat (id 21) and a regular private chat (id 22).
        for json in [
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":22},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }

        driver.select_chat(ChatId(21)).unwrap();
        let cmd_fetches = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|j| j.contains("\"@type\":\"getCommands\""))
                .collect::<Vec<_>>()
        };
        let first = cmd_fetches();
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("\"scope\":null"));
        assert!(first[0].contains("\"language_code\":\"\""));
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetCommands, Some(ChatId(21)))
            .expect("getCommands in flight");

        // Re-selecting while the fetch is in flight sends nothing new.
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(cmd_fetches().len(), 1);

        // The `botCommands` response lands in the cache as global rows.
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"botCommands","@extra":"{}","bot_user_id":21,"commands":[{{"@type":"botCommand","command":"settings","description":"CANARY_global","is_ephemeral":false}}]}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let items = driver.session.command_menu_items(ChatId(21));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command, "settings");
        assert_eq!(items[0].description, "CANARY_global");
        assert!(items[0].global);
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(cmd_fetches().len(), 1);

        // `botInfo` commands merge first; duplicates keep the
        // bot-specific description and are not repeated.
        let full_extra = driver
            .session
            .request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}},{{"@type":"botCommand","command":"settings","description":"Specific settings","is_ephemeral":false}}]}}}}"#,
                        full_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let items = driver.session.command_menu_items(ChatId(21));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].command, "start");
        assert!(!items[0].global);
        assert_eq!(items[1].command, "settings");
        assert_eq!(items[1].description, "Specific settings");
        assert!(!items[1].global);

        // A regular private chat never triggers the fetch.
        driver.select_chat(ChatId(22)).unwrap();
        assert_eq!(cmd_fetches().len(), 1);
        assert!(driver.session.command_menu_items(ChatId(22)).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bot_commands_error_absorbed_without_retry() {
        // Phase 3.3: an `error` answer to `getCommands` (user sessions —
        // the schema annotates the method "for bots only") is recorded as
        // an empty command set, so opening the chat again does not
        // refetch; the menu falls back to the `botInfo` commands.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        for json in [
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        // Seed `botInfo` so the fallback menu has rows after the error.
        let full_extra = driver
            .session
            .request(RequestPurpose::GetUserFullInfo, Some(ChatId(21)));
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"","description":"","commands":[{{"@type":"botCommand","command":"start","description":"Start","is_ephemeral":false}}]}}}}"#,
                        full_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        driver.select_chat(ChatId(21)).unwrap();
        let cmd_fetches = || {
            recorder
                .snapshot()
                .into_iter()
                .filter(|j| j.contains("\"@type\":\"getCommands\""))
                .count()
        };
        assert_eq!(cmd_fetches(), 1);
        let extra = driver
            .session
            .requests
            .pending_extra_for(RequestPurpose::GetCommands, Some(ChatId(21)))
            .expect("getCommands in flight");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CANARY_bots_only"}}"#,
                        extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        // The error records an empty set; re-opening the chat refetches
        // nothing, and the menu shows the `botInfo` commands only.
        driver.select_chat(ChatId(21)).unwrap();
        assert_eq!(cmd_fetches(), 1);
        let items = driver.session.command_menu_items(ChatId(21));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command, "start");
        assert!(!items[0].global);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn callback_query_sent_for_button_press() {
        // Phase 3.2: pressing a callback button sends `getCallbackQueryAnswer`
        // (schema 1.8.67 line 13138) with the button's payload bytes
        // (base64 in JSON). Pending messages and unknown chats are refused.
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        for json in [
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Demo Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Vote","icon_custom_emoji_id":0,"style":{"@type":"buttonStyleDefault"},"type":{"@type":"inlineKeyboardButtonTypeCallback","data":"AQID"}}]],"force_reply":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Pick","entities":[]}}}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
                .unwrap();
        }
        let extra = driver
            .send_callback_query(ChatId(21), MessageId(301), &[1, 2, 3])
            .expect("callback query sends");
        let sent = recorder.snapshot();
        let query = sent
            .iter()
            .find(|j| j.contains(r#""@type":"getCallbackQueryAnswer""#))
            .expect("getCallbackQueryAnswer recorded");
        let v: serde_json::Value = serde_json::from_str(query).unwrap();
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 301);
        assert_eq!(v["payload"]["@type"], "callbackQueryPayloadData");
        assert_eq!(v["payload"]["data"], "AQID");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert!(
            driver
                .session
                .requests
                .pending_extra_for(RequestPurpose::GetCallbackQueryAnswer, Some(ChatId(21)))
                .is_some()
        );
        // Pending (unsent, negative id) messages cannot be answered.
        assert!(
            driver
                .send_callback_query(ChatId(21), MessageId(-1), &[1])
                .is_err()
        );
        // Unknown chats are refused.
        assert!(
            driver
                .send_callback_query(ChatId(99), MessageId(301), &[1])
                .is_err()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn select_chat_closes_previous_and_does_not_mark_unread_locally() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":3,"last_read_inbox_message_id":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
        driver.select_chat(ChatId(8)).unwrap();
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"closeChat\"") && j.contains("\"chat_id\":7"))
        );
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":8"))
        );
        // Unread is TDLib-authoritative; opening must not zero it locally.
        assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":9,"unread_count":0}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 0);
        assert!(!sink.rendered().contains("Alice"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn send_text_rejected_when_empty_or_no_open_chat() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(1),
            driver.session.view_generation,
            "   ",
        );
        assert_eq!(
            driver.send_text_snapshot(&snap),
            Err(ConnectSendError::InvalidRequest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    struct ViewCtlSender {
        sent: Mutex<Vec<String>>,
        fail_view: Mutex<bool>,
    }

    impl ViewCtlSender {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                sent: Mutex::new(Vec::new()),
                fail_view: Mutex::new(false),
            })
        }

        fn snapshot(&self) -> Vec<String> {
            self.sent.lock().expect("view ctl sender").clone()
        }

        fn set_fail_view(&self, fail: bool) {
            *self.fail_view.lock().expect("view ctl sender") = fail;
        }

        fn view_count(&self) -> usize {
            self.snapshot()
                .iter()
                .filter(|j| j.contains("viewMessages"))
                .count()
        }
    }

    impl JsonSender for Arc<ViewCtlSender> {
        fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
            if request.contains("viewMessages") && *self.fail_view.lock().expect("view ctl sender")
            {
                return Err(ConnectSendError::Native);
            }
            self.sent
                .lock()
                .expect("view ctl sender")
                .push(request.to_string());
            Ok(())
        }
    }

    fn ready_private_chat(
        driver: &mut ConnectDriver<Arc<ViewCtlSender>>,
        seq: &AtomicU64,
        sink: &Arc<dyn DiagnosticSink>,
    ) {
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":1}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":11,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
    }

    #[test]
    fn view_messages_send_failure_unsticks_gate_and_retries() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let sender = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);

        sender.set_fail_view(true);
        assert_eq!(driver.select_chat(ChatId(7)), Err(ConnectSendError::Native));
        assert!(
            !driver
                .session
                .requests
                .has_purpose(RequestPurpose::ViewMessages)
        );
        assert_eq!(
            driver.session.message_ids_to_view(ChatId(7)),
            vec![MessageId(11)]
        );
        assert_eq!(sender.view_count(), 0);

        sender.set_fail_view(false);
        let extra = driver
            .maybe_view_open_messages()
            .unwrap()
            .expect("retry viewMessages");
        assert!(
            driver
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
        );
        assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());
        assert_eq!(sender.view_count(), 1);
        assert!(
            sender
                .snapshot()
                .last()
                .unwrap()
                .contains(&format!("\"@extra\":\"{}\"", extra.0))
        );
        assert!(!sink.rendered().contains("hi"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn view_messages_tdlib_error_unsticks_gate_and_retries() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let sender = ViewCtlSender::new();
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        ready_private_chat(&mut driver, &seq, &dyn_sink);

        let _history = driver
            .select_chat(ChatId(7))
            .unwrap()
            .expect("history after view");
        assert_eq!(sender.view_count(), 1);
        let view_extra = sender
            .snapshot()
            .iter()
            .find(|j| j.contains("viewMessages"))
            .and_then(|j| serde_json::from_str::<Value>(j).ok())
            .and_then(|v| v["@extra"].as_str().map(str::to_string))
            .expect("view extra");
        assert!(
            driver
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
        );
        assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_VIEW_TD","@extra":"{view_extra}"}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            sender.view_count(),
            2,
            "TDLib error must retry viewMessages"
        );
        let retry_extra = sender
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains("viewMessages"))
            .and_then(|j| serde_json::from_str::<Value>(&j).ok())
            .and_then(|v| v["@extra"].as_str().map(str::to_string))
            .expect("retry extra");
        assert_ne!(
            retry_extra, view_extra,
            "retry must not reuse the failed extra"
        );
        assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());
        assert!(
            driver
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
        );
        assert!(!sink.rendered().contains("CANARY_VIEW_TD"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sticker_panel_loads_installed_set_and_send_uses_input_file_id() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        driver.open_sticker_panel().unwrap();
        let sets_extra = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("getInstalledStickerSets"))
            .expect("installed sets");
        let sets_extra: Value = serde_json::from_str(&sets_extra).unwrap();
        assert_eq!(sets_extra["sticker_type"]["@type"], "stickerTypeRegular");
        let extra = sets_extra["@extra"].as_str().unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"stickerSets","@extra":"{extra}","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":1,"covers":[]}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.stickers.selected_set_id, Some(77));
        let set_req = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("getStickerSet"))
            .expect("getStickerSet");
        let set_req: Value = serde_json::from_str(&set_req).unwrap();
        assert_eq!(set_req["set_id"], "77");
        let set_extra = set_req["@extra"].as_str().unwrap();
        let file = r#"{"@type":"file","id":41,"size":8,"expected_size":8,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":8}}"#;
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"stickerSet","@extra":"{set_extra}","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatWebp"}},"width":128,"height":128,"file":{file}}},"sticker":{file}}}],"emojis":[]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.stickers.stickers.len(), 1);
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("downloadFile") && json.contains("\"file_id\":41"))
        );
        driver
            .send_sticker(
                ChatId(7),
                StickerSend {
                    file_id: FileId(41),
                    emoji: "😀",
                    width: 512,
                    height: 512,
                    thumb: Some((FileId(41), 128, 128)),
                    reply_to: None,
                },
            )
            .unwrap();
        let sent = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("inputMessageSticker"))
            .expect("send sticker");
        let sent: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(
            sent["input_message_content"]["sticker"]["sticker"]["@type"],
            "inputFileId"
        );
        assert_eq!(
            sent["input_message_content"]["sticker"]["sticker"]["id"],
            41
        );
        assert_eq!(sent["input_message_content"]["emoji"], "😀");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gif_panel_loads_saved_animations_and_send_uses_input_animation() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        driver.open_gif_panel().unwrap();
        let saved = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("getSavedAnimations"))
            .expect("saved animations");
        let saved: Value = serde_json::from_str(&saved).unwrap();
        let extra = saved["@extra"].as_str().unwrap();
        let thumb = r#"{"@type":"file","id":42,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"t","unique_id":"tu","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}"#;
        let file = r#"{"@type":"file","id":33,"size":9,"expected_size":9,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"a","unique_id":"au","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":9}}"#;
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"animations","@extra":"{extra}","animations":[{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"wave.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":70,"file":{thumb}}},"animation":{file}}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.gifs.animations.len(), 1);
        assert_eq!(driver.session.gifs.animations[0].file_id, FileId(33));
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("downloadFile") && json.contains("\"file_id\":42"))
        );
        driver
            .send_animation(
                ChatId(7),
                AnimationSend {
                    file_id: FileId(33),
                    duration: 2,
                    width: 240,
                    height: 140,
                    reply_to: None,
                },
            )
            .unwrap();
        let sent = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("inputMessageAnimation"))
            .expect("send animation");
        let sent: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(
            sent["input_message_content"]["animation"]["@type"],
            "inputAnimation"
        );
        assert_eq!(
            sent["input_message_content"]["animation"]["animation"]["id"],
            33
        );
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateSavedAnimations","animation_ids":[33]}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            recorder
                .snapshot()
                .iter()
                .filter(|json| json.contains("getSavedAnimations"))
                .count()
                >= 2
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn voice_note_send_uses_input_file_local_and_recording_action() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
        driver.sync_voice_recording(true, 1_000).unwrap();
        driver.sync_voice_recording(true, 1_100).unwrap();
        let actions: Vec<String> = recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains("sendChatAction"))
            .collect();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("chatActionRecordingVoiceNote"));
        let voice = dir.join("note.ogg");
        std::fs::write(&voice, b"OggS").unwrap();
        let draft = VoiceDraft {
            path: voice,
            duration_secs: 3,
            bars: vec![31, 0, 1],
        };
        driver
            .send_voice_note(&draft, "", Some(MessageId(4)))
            .unwrap();
        let sent = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("inputMessageVoiceNote"))
            .expect("send voice");
        let sent: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(
            sent["input_message_content"]["voice_note"]["voice_note"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(sent["input_message_content"]["voice_note"]["duration"], 3);
        assert!(
            !sent["input_message_content"]["voice_note"]["waveform"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert_eq!(sent["input_message_content"]["caption"], Value::Null);
        assert_eq!(sent["reply_to"]["message_id"], 4);
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("chatActionCancel"))
        );
        let file = r#"{"@type":"file","id":4,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}"#;
        let history = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"waveform":"","mime_type":"audio/ogg","speech_recognition_result":null,"voice":{file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#
        );
        driver
            .ingest(copy_and_parse(&history, &seq, &dyn_sink).unwrap())
            .unwrap();
        driver.open_voice_content(ChatId(7), MessageId(8)).unwrap();
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|json| json.contains("openMessageContent"))
        );
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageContentOpened","chat_id":7,"message_id":8}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let row = driver
            .session
            .histories
            .get(&7)
            .and_then(|h| h.messages.get(&8))
            .expect("voice row");
        match &row.content {
            crate::telegram::envelope::MessageContent::VoiceNote(note) => {
                assert!(note.is_listened);
                assert_eq!(note.duration, 12);
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn photo_history_auto_downloads_thumb_and_user_open_sends_full() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
        let thumb = r#"{"@type":"file","id":1,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}"#;
        let full = r#"{"@type":"file","id":2,"size":80,"expected_size":80,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":80}}"#;
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{extra}","messages":[{{"id":20,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_MEDIA","entities":[]}},"has_spoiler":false,"is_secret":false}}}}]}}"#,
                        extra = history_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let sent = recorder.snapshot();
        let thumb_req = sent
            .iter()
            .rev()
            .find(|j| j.contains("downloadFile") && j.contains("\"file_id\":1"))
            .expect("auto thumb downloadFile");
        let thumb_json: Value = serde_json::from_str(thumb_req).unwrap();
        assert_eq!(thumb_json["priority"], THUMB_DOWNLOAD_PRIORITY);
        assert_eq!(thumb_json["synchronous"], false);
        assert!(
            !sent.iter().any(|j| j.contains("\"file_id\":2")),
            "must not auto-download the full size"
        );
        let user = driver
            .download_file(FileId(2), USER_DOWNLOAD_PRIORITY)
            .unwrap()
            .expect("user download");
        let last = recorder.snapshot();
        let user_req = last.last().unwrap();
        assert!(user_req.contains("downloadFile"));
        assert!(user_req.contains("\"file_id\":2"));
        assert!(user_req.contains(&format!("\"@extra\":\"{}\"", user.0)));
        let user_json: Value = serde_json::from_str(user_req).unwrap();
        assert_eq!(user_json["priority"], USER_DOWNLOAD_PRIORITY);
        assert_eq!(
            driver.download_file(FileId(2), USER_DOWNLOAD_PRIORITY),
            Ok(None),
            "in-flight download must not duplicate"
        );
        assert!(!sink.rendered().contains("CANARY_MEDIA"));
        assert!(!sink.rendered().contains("CANARY_REMOTE"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_sends_photo_and_document_from_picked_paths() {
        use crate::composer::{AttachmentKind, ComposerAttachment, ComposerSnapshot};
        use std::time::{SystemTime, UNIX_EPOCH};

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pick_dir =
            std::env::temp_dir().join(format!("quill-send-pick-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&pick_dir).unwrap();
        let photo = pick_dir.join("out.png");
        let doc = pick_dir.join("notes.txt");
        std::fs::write(&photo, [1, 2, 3]).unwrap();
        std::fs::write(&doc, b"hello").unwrap();

        let photo_att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
        let photo_path = photo_att.send_path_str().unwrap();
        let snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_PHOTO_CAP",
            Some(photo_att),
        );
        let extra = driver.send_snapshot(&snap).unwrap();
        let last = recorder.snapshot();
        let send_json = last.last().unwrap();
        let v: Value = serde_json::from_str(send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["input_message_content"]["@type"], "inputMessagePhoto");
        assert_eq!(
            v["input_message_content"]["photo"]["photo"]["path"],
            photo_path
        );
        assert_eq!(
            v["input_message_content"]["caption"]["text"],
            "CANARY_PHOTO_CAP"
        );

        // Pending message response upserts outgoing media.
        let pending = copy_and_parse(
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":-5,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":50,"size":3,"expected_size":3,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_CAP","entities":[]}},"has_spoiler":false,"is_secret":false}}}}"#,
                extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(pending).unwrap();
        let msg = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&-5)
            .unwrap();
        assert!(msg.pending);
        assert!(msg.is_outgoing);
        assert!(matches!(
            msg.content,
            crate::telegram::envelope::MessageContent::Photo(_)
        ));

        let doc_att = ComposerAttachment::pick(&doc, AttachmentKind::Document).unwrap();
        let doc_path = doc_att.send_path_str().unwrap();
        let doc_snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "",
            Some(doc_att),
        );
        let doc_extra = driver.send_snapshot(&doc_snap).unwrap();
        let doc_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(doc_json["@extra"], doc_extra.0.to_string());
        assert_eq!(
            doc_json["input_message_content"]["@type"],
            "inputMessageDocument"
        );
        assert_eq!(
            doc_json["input_message_content"]["document"]["document"]["path"],
            doc_path
        );
        assert_eq!(doc_json["input_message_content"]["caption"]["text"], "");

        let clip = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs/screenshots/fixtures/demo-clip.mp4");
        let video_att = ComposerAttachment::pick(&clip, AttachmentKind::Video).unwrap();
        let video_path = video_att.send_path_str().unwrap();
        let video_snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_VIDEO_CAP",
            Some(video_att),
        );
        let video_extra = driver.send_snapshot(&video_snap).unwrap();
        let video_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(video_json["@extra"], video_extra.0.to_string());
        assert_eq!(
            video_json["input_message_content"]["@type"],
            "inputMessageVideo"
        );
        assert_eq!(
            video_json["input_message_content"]["video"]["video"]["path"],
            video_path
        );
        assert_eq!(
            video_json["input_message_content"]["video"]["thumbnail"],
            Value::Null
        );
        assert_eq!(video_json["input_message_content"]["video"]["duration"], 1);
        assert_eq!(video_json["input_message_content"]["video"]["width"], 320);
        assert_eq!(video_json["input_message_content"]["video"]["height"], 180);
        assert_eq!(
            video_json["input_message_content"]["video"]["supports_streaming"],
            true
        );
        assert_eq!(
            video_json["input_message_content"]["caption"]["text"],
            "CANARY_VIDEO_CAP"
        );

        let note = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs/screenshots/fixtures/demo-video-note.mp4");
        let note_att = ComposerAttachment::pick(&note, AttachmentKind::VideoNote).unwrap();
        let note_path = note_att.send_path_str().unwrap();
        let note_snap = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "ignored caption",
            Some(note_att),
        );
        let note_extra = driver.send_snapshot(&note_snap).unwrap();
        let note_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(note_json["@extra"], note_extra.0.to_string());
        assert_eq!(
            note_json["input_message_content"]["@type"],
            "inputMessageVideoNote"
        );
        let sent_note = &note_json["input_message_content"]["video_note"];
        assert_eq!(sent_note["video_note"]["path"], note_path);
        assert_eq!(sent_note["duration"], 1);
        assert_eq!(sent_note["length"], 240);
        assert_eq!(
            note_json["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert!(note_json["input_message_content"].get("caption").is_none());
        let thumb = &sent_note["thumbnail"];
        if !thumb.is_null() {
            assert_eq!(thumb["@type"], "inputThumbnail");
            assert_eq!(thumb["width"], 240);
            assert_eq!(thumb["height"], 240);
            assert!(
                thumb["thumbnail"]["path"]
                    .as_str()
                    .unwrap_or("")
                    .ends_with(".jpg")
            );
        }
        let landscape = ComposerAttachment::pick(&clip, AttachmentKind::VideoNote).unwrap();
        let bad = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "",
            Some(landscape),
        );
        assert_eq!(
            driver.send_snapshot(&bad),
            Err(ConnectSendError::InvalidRequest)
        );

        let album_photo = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
        let album_video = ComposerAttachment::pick(&clip, AttachmentKind::Video).unwrap();
        let album_snap = ComposerSnapshot::capture_album(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_ALBUM_CAP",
            vec![album_photo, album_video],
        )
        .with_reply(None);
        let album_extra = driver.send_snapshot(&album_snap).unwrap();
        let album_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
        assert_eq!(album_json["@type"], "sendMessageAlbum");
        assert_eq!(album_json["@extra"], album_extra.0.to_string());
        assert!(album_json.get("reply_markup").is_none());
        let contents = album_json["input_message_contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["@type"], "inputMessagePhoto");
        assert_eq!(contents[0]["caption"]["text"], "");
        assert_eq!(contents[0]["show_caption_above_media"], false);
        assert_eq!(contents[1]["@type"], "inputMessageVideo");
        assert_eq!(contents[1]["caption"]["text"], "CANARY_ALBUM_CAP");
        assert_eq!(contents[1]["show_caption_above_media"], false);
        let album_reply = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":-8,"chat_id":7,"is_outgoing":true,"media_album_id":"9001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":51,"size":3,"expected_size":3,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}},{{"id":-7,"chat_id":7,"is_outgoing":true,"media_album_id":"9001","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{{"@type":"file","id":52,"size":4,"expected_size":4,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"y","unique_id":"v","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"CANARY_ALBUM_CAP","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}]}}"#,
                album_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
        driver.ingest(album_reply).unwrap();
        let history = driver.session.histories.get(&7).unwrap();
        let first = history.messages.get(&-8).unwrap();
        let second = history.messages.get(&-7).unwrap();
        assert!(first.pending && second.pending);
        assert_eq!(first.media_album_id, 9001);
        assert_eq!(second.media_album_id, 9001);
        assert_eq!(history.messages.get(&-5).unwrap().media_album_id, 0);

        // Reject paths that were not picked through ComposerAttachment.
        let forged = ComposerSnapshot::capture_with_attachment(
            ChatId(7),
            driver.session.view_generation,
            "",
            Some(crate::composer::ComposerAttachment {
                path: pick_dir.join("missing-forged.bin"),
                kind: AttachmentKind::Document,
                file_name: "missing-forged.bin".into(),
            }),
        );
        assert_eq!(
            driver.send_snapshot(&forged),
            Err(ConnectSendError::InvalidRequest)
        );
        assert!(!sink.rendered().contains("CANARY_PHOTO"));
        let _ = std::fs::remove_dir_all(&pick_dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_search_happy_empty_error_and_select() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        assert_eq!(
            driver.set_search_query("alice"),
            Err(ConnectSendError::InvalidRequest)
        );
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let extras = commit_typed_search(&mut driver, "alice");
        let SearchFlight::Query(chats_extra, messages_extra) = extras else {
            panic!("expected typed search");
        };
        let sent = recorder.snapshot();
        assert!(
            sent.iter()
                .any(|j| j.contains("\"@type\":\"searchChats\"") && j.contains("alice"))
        );
        assert!(sent.iter().any(|j| {
            j.contains("\"@type\":\"searchMessages\"") && j.contains("\"chat_list\":null")
        }));
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                        chats_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_search","entities":[]}}}}}}]}}"#,
                        messages_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Ready);
        assert_eq!(driver.session.search.chat_ids, vec![ChatId(7)]);
        driver
            .select_search_message(ChatId(7), MessageId(50), "", None, 0)
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Closed);
        assert_eq!(driver.session.open_chat, Some(ChatId(7)));
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .messages
                .contains_key(&50)
        );
        assert!(recorder.snapshot().iter().any(
            |j| j.contains("\"@type\":\"addRecentlyFoundChat\"") && j.contains("\"chat_id\":7")
        ));
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":7"))
        );

        let recents = driver.open_search().unwrap().expect("recents");
        let SearchFlight::Recents(recents_extra) = recents else {
            panic!("expected recents");
        };
        assert!(
            recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchRecentlyFoundChats\""))
        );
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                        recents_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Ready);
        assert!(driver.session.search.recents);

        let empty = commit_typed_search(&mut driver, "zzz");
        let SearchFlight::Query(empty_chats, empty_messages) = empty else {
            panic!("expected typed empty search");
        };
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                        empty_chats.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
                        empty_messages.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Empty);

        let fail = commit_typed_search(&mut driver, "nope");
        let SearchFlight::Query(fail_chats, fail_messages) = fail else {
            panic!("expected typed fail search");
        };
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR","@extra":"{}"}}"#,
                        fail_chats.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_DRV_ERR2","@extra":"{}"}}"#,
                        fail_messages.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.search.status, SearchStatus::Failed);
        driver.close_search();
        assert_eq!(driver.session.search.status, SearchStatus::Closed);
        assert!(!sink.rendered().contains("CANARY_DRV"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn commit_typed_search<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        query: &str,
    ) -> SearchFlight {
        match driver.set_search_query(query).unwrap() {
            SearchQueryOutcome::Debounced { token } => driver
                .commit_debounced_search(token)
                .unwrap()
                .expect("debounced search"),
            other => panic!("expected Debounced, got {other:?}"),
        }
    }

    #[test]
    fn driver_typed_search_debounce_settles_once() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let t1 = match driver.set_search_query("a").unwrap() {
            SearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t2 = match driver.set_search_query("al").unwrap() {
            SearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t3 = match driver.set_search_query("alice").unwrap() {
            SearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        assert_ne!(t1, t3);
        assert_ne!(t2, t3);
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchChats\""))
        );
        assert!(driver.commit_debounced_search(t1).unwrap().is_none());
        assert!(driver.commit_debounced_search(t2).unwrap().is_none());
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchChats\""))
        );
        let settled = driver
            .commit_debounced_search(t3)
            .unwrap()
            .expect("settled");
        let SearchFlight::Query(_, _) = settled else {
            panic!("expected typed pair");
        };
        let sent: Vec<String> = recorder
            .snapshot()
            .into_iter()
            .filter(|j| {
                j.contains("\"@type\":\"searchChats\"")
                    || j.contains("\"@type\":\"searchMessages\"")
            })
            .collect();
        assert_eq!(sent.len(), 2);
        assert!(sent.iter().all(|j| j.contains("alice")));
        assert!(
            !sent
                .iter()
                .any(|j| j.contains("\"query\":\"a\"") || j.contains("\"query\":\"al\""))
        );
        assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(900));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn seed_ready_alice<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        seq: &AtomicU64,
        dyn_sink: &Arc<dyn DiagnosticSink>,
    ) {
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already here","entities":[]}}}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver.select_chat(ChatId(7)).unwrap();
    }

    fn commit_typed_chat_search<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        query: &str,
    ) -> ChatSearchFlight {
        match driver.set_chat_search_query(query).unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => driver
                .commit_debounced_chat_search(token)
                .unwrap()
                .expect("debounced chat search"),
            other => panic!("expected Debounced, got {other:?}"),
        }
    }

    #[test]
    fn driver_chat_search_debounce_jump_empty_and_close() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        assert_eq!(
            driver.set_chat_search_query("hello"),
            Err(ConnectSendError::InvalidRequest)
        );
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        assert!(driver.open_chat_search().unwrap());
        assert!(driver.session.chat_search.open);

        let t1 = match driver.set_chat_search_query("h").unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t2 = match driver.set_chat_search_query("he").unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        let t3 = match driver.set_chat_search_query("hello").unwrap() {
            ChatSearchQueryOutcome::Debounced { token } => token,
            other => panic!("{other:?}"),
        };
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("\"@type\":\"searchChatMessages\""))
        );
        assert!(driver.commit_debounced_chat_search(t1).unwrap().is_none());
        assert!(driver.commit_debounced_chat_search(t2).unwrap().is_none());
        let ChatSearchFlight::Query(extra) = driver
            .commit_debounced_chat_search(t3)
            .unwrap()
            .expect("settled");
        let sent = recorder.snapshot();
        let search_json = sent
            .iter()
            .rev()
            .find(|j| j.contains("\"@type\":\"searchChatMessages\""))
            .expect("searchChatMessages");
        let v: Value = serde_json::from_str(search_json).unwrap();
        assert_eq!(v["query"], "hello");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["sender_id"], Value::Null);
        assert_eq!(v["from_message_id"], 0);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], CHAT_SEARCH_LIMIT);
        assert_eq!(v["filter"], Value::Null);
        assert!(!search_json.contains("\"query\":\"h\""));

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_chat","entities":[]}}}}}},{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.chat_search.status, SearchStatus::Ready);
        assert_eq!(
            driver.session.chat_search.jump,
            crate::state::ChatSearchJump::Ready {
                message_id: MessageId(50)
            }
        );
        let around = driver
            .jump_to_chat_search_message(MessageId(40))
            .unwrap()
            .expect("around load");
        let around_json = recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| {
                j.contains("\"@type\":\"getChatHistory\"") && j.contains("\"from_message_id\":40")
            })
            .expect("getChatHistory around");
        let around_v: Value = serde_json::from_str(&around_json).unwrap();
        assert_eq!(around_v["offset"], HISTORY_AROUND_OFFSET);
        assert_eq!(around_v["limit"], HISTORY_AROUND_LIMIT);
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":39,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
                        around.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            driver.session.chat_search.jump,
            crate::state::ChatSearchJump::Ready {
                message_id: MessageId(40)
            }
        );
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .contains(MessageId(39))
        );

        let ChatSearchFlight::Query(empty_extra) = commit_typed_chat_search(&mut driver, "zzz");
        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
                        empty_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.chat_search.status, SearchStatus::Empty);

        let before_close = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .contains_key(&50);
        driver.close_chat_search();
        assert_eq!(driver.session.chat_search.status, SearchStatus::Closed);
        assert_eq!(driver.session.open_chat, Some(ChatId(7)));
        assert!(before_close);
        assert!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .contains(MessageId(50))
        );
        assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(900));
        assert!(!sink.rendered().contains("CANARY_DRV_chat"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_send_reply_shape_and_jump_to_replied() {
        use crate::composer::ComposerReplyTo;

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let snap = crate::composer::ComposerSnapshot::capture(
            ChatId(7),
            driver.session.view_generation,
            "CANARY_REPLY_text",
        )
        .with_reply(Some(ComposerReplyTo::new(
            ChatId(7),
            MessageId(50),
            "hello already here",
        )));
        let extra = driver.send_snapshot(&snap).unwrap();
        let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 50);
        assert_eq!(v["reply_to"]["quote"], Value::Null);
        assert_eq!(v["reply_to"]["checklist_task_id"], 0);
        assert_eq!(v["reply_to"]["poll_option_id"], "");
        assert!(send_json.contains("CANARY_REPLY_text"));

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_REPLY_text","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":7,"message_id":50}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let reply = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&60)
            .unwrap();
        assert_eq!(
            driver.session.reply_quote_preview(reply).as_deref(),
            Some("hello already here")
        );
        assert_eq!(
            driver
                .jump_to_replied_message(reply.reply_to.as_ref().unwrap().message_id)
                .unwrap(),
            None
        );
        assert_eq!(
            driver.session.chat_search.jump,
            crate::state::ChatSearchJump::Ready {
                message_id: MessageId(50)
            }
        );
        assert!(
            driver
                .jump_to_replied_message(MessageId(40))
                .unwrap()
                .is_some()
        );
        assert!(!sink.rendered().contains("CANARY_REPLY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_edit_text_shape_and_incoming_rejected() {
        use crate::composer::{ComposerEdit, ComposerEditKind};

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let incoming = ComposerEdit {
            chat_id: ChatId(7),
            message_id: MessageId(50),
            original_text: "hello already here".into(),
            kind: ComposerEditKind::Text,
        };
        assert_eq!(
            driver.edit_snapshot(&incoming, "nope"),
            Err(ConnectSendError::InvalidRequest)
        );

        let edit = ComposerEdit {
            chat_id: ChatId(7),
            message_id: MessageId(60),
            original_text: "own outgoing".into(),
            kind: ComposerEditKind::Text,
        };
        assert_eq!(
            driver.edit_snapshot(&edit, "   "),
            Err(ConnectSendError::InvalidRequest)
        );
        let extra = driver.edit_snapshot(&edit, "CANARY_EDIT_text").unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("editMessageText");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageText");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 60);
        assert_eq!(v["reply_markup"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageText");
        assert_eq!(
            v["input_message_content"]["text"]["text"],
            "CANARY_EDIT_text"
        );
        assert_eq!(v["input_message_content"]["clear_draft"], false);

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageContent","chat_id":7,"message_id":60,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDIT_text","entities":[]}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            driver
                .session
                .histories
                .get(&7)
                .unwrap()
                .messages
                .get(&60)
                .unwrap()
                .content
                .preview(),
            "CANARY_EDIT_text"
        );
        assert!(!sink.rendered().contains("CANARY_EDIT"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_delete_confirm_shape_and_tombstone() {
        use crate::composer::DeleteConfirm;

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let incoming = DeleteConfirm {
            chat_id: ChatId(7),
            message_id: MessageId(50),
        };
        assert_eq!(
            driver.delete_confirmed(&incoming),
            Err(ConnectSendError::InvalidRequest)
        );

        let confirm = DeleteConfirm::own(ChatId(7), MessageId(60), true, false).unwrap();
        let extra = driver.delete_confirmed(&confirm).unwrap();
        let json = recorder.snapshot().last().cloned().expect("deleteMessages");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteMessages");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_ids"], serde_json::json!([60]));
        assert_eq!(v["revoke"], true);

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[60],"is_permanent":true,"from_cache":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let history = driver.session.histories.get(&7).unwrap();
        assert!(!history.contains(MessageId(60)));
        assert!(history.is_tombstone(MessageId(60)));
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_forward_messages_shape_and_dest_result() {
        use crate::composer::ForwardDraft;

        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":8,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

        let mut draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).unwrap();
        draft.toggle(ChatId(7), MessageId(60), false);
        let extra = driver.forward_messages(ChatId(8), &draft).unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("forwardMessages");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "forwardMessages");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 8);
        assert_eq!(v["from_chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["message_ids"], serde_json::json!([50, 60]));
        assert_eq!(v["send_copy"], false);
        assert_eq!(v["remove_caption"], false);
        assert_eq!(v["options"], Value::Null);

        driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":80,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello already here","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}},{{"id":81,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"own outgoing","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let result = driver
            .session
            .last_forward
            .as_ref()
            .expect("forward result");
        assert_eq!(result.dest_title, "Bob");
        assert_eq!(result.forwarded_ids, vec![MessageId(80), MessageId(81)]);
        assert_eq!(result.success_label(), "Forwarded 2 messages to Bob");
        assert!(
            driver
                .session
                .histories
                .get(&8)
                .unwrap()
                .contains(MessageId(80))
        );
        assert_eq!(
            driver.session.forward_from_label(
                driver
                    .session
                    .histories
                    .get(&8)
                    .unwrap()
                    .messages
                    .get(&80)
                    .unwrap()
                    .forward_info
                    .as_ref()
                    .unwrap()
            ),
            "Forwarded from Alice"
        );
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_add_and_remove_message_reaction_then_interaction_info() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver
            .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
            .unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("addMessageReaction");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "addMessageReaction");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 50);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert_eq!(v["is_big"], false);
        assert_eq!(v["update_recent_reactions"], true);
        assert!(!json.contains("setMessageReactions"));

        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let reacted = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(reacted.chosen_emoji("❤"));
        assert_eq!(
            reacted.emoji_reaction_chips()[0].chip_label().as_deref(),
            Some("❤ 1")
        );

        let remove_extra = driver
            .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
            .unwrap();
        let remove_json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("removeMessageReaction");
        let v: Value = serde_json::from_str(&remove_json).unwrap();
        assert_eq!(v["@type"], "removeMessageReaction");
        assert_eq!(v["@extra"], remove_extra.0.to_string());
        assert_eq!(v["reaction_type"]["emoji"], "❤");

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":null}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let cleared = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(cleared.emoji_reaction_chips().is_empty());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_pin_and_unpin_chat_message_then_is_pinned_update() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver.pin_chat_message(ChatId(7), MessageId(50)).unwrap();
        let json = recorder.snapshot().last().cloned().expect("pinChatMessage");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "pinChatMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 50);
        assert_eq!(v["disable_notification"], false);
        assert_eq!(v["only_for_self"], false);
        assert!(!json.contains("unpinAllChatMessages"));

        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":true}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let pinned = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(pinned.is_pinned);

        let unpin_extra = driver.unpin_chat_message(ChatId(7), MessageId(50)).unwrap();
        let unpin_json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("unpinChatMessage");
        let v: Value = serde_json::from_str(&unpin_json).unwrap();
        assert_eq!(v["@type"], "unpinChatMessage");
        assert_eq!(v["@extra"], unpin_extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 50);

        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, unpin_extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageIsPinned","chat_id":7,"message_id":50,"is_pinned":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let cleared = driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&50)
            .unwrap();
        assert!(!cleared.is_pinned);
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_mute_forever_then_unmute_and_archive() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        let extra = driver.mute_chat_forever(ChatId(7)).unwrap();
        let json = recorder
            .snapshot()
            .last()
            .cloned()
            .expect("setChatNotificationSettings");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setChatNotificationSettings");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(
            v["notification_settings"]["@type"],
            "chatNotificationSettings"
        );
        assert_eq!(v["notification_settings"]["use_default_mute_for"], false);
        assert_eq!(v["notification_settings"]["mute_for"], i32::MAX);
        driver
            .ingest(
                copy_and_parse(
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chats.get(&7).unwrap().is_muted());

        let unmute = driver.unmute_chat(ChatId(7)).unwrap();
        let unmute_json = recorder.snapshot().last().cloned().unwrap();
        let v: Value = serde_json::from_str(&unmute_json).unwrap();
        assert_eq!(v["@type"], "setChatNotificationSettings");
        assert_eq!(v["@extra"], unmute.0.to_string());
        assert_eq!(v["notification_settings"]["mute_for"], 0);

        let archive = driver.archive_chat(ChatId(7)).unwrap();
        let archive_json = recorder.snapshot().last().cloned().unwrap();
        let v: Value = serde_json::from_str(&archive_json).unwrap();
        assert_eq!(v["@type"], "addChatToList");
        assert_eq!(v["@extra"], archive.0.to_string());
        assert_eq!(v["chat_list"]["@type"], "chatListArchive");
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"0","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"4","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.ordered_chats().is_empty());
        assert_eq!(driver.session.ordered_archived_chats()[0].id.0, 7);

        let unarchive = driver.unarchive_chat(ChatId(7)).unwrap();
        let unarchive_json = recorder.snapshot().last().cloned().unwrap();
        let v: Value = serde_json::from_str(&unarchive_json).unwrap();
        assert_eq!(v["@type"], "addChatToList");
        assert_eq!(v["@extra"], unarchive.0.to_string());
        assert_eq!(v["chat_list"]["@type"], "chatListMain");
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_sends_typing_then_cancel_on_empty_and_send() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);

        driver.sync_outgoing_typing("hello", false, 1_000).unwrap();
        let typing = recorder.snapshot().last().cloned().expect("sendChatAction");
        let v: Value = serde_json::from_str(&typing).unwrap();
        assert_eq!(v["@type"], "sendChatAction");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["business_connection_id"], "");
        assert_eq!(v["action"]["@type"], "chatActionTyping");

        let before = recorder.snapshot().len();
        driver.sync_outgoing_typing("hello!", false, 2_000).unwrap();
        assert_eq!(recorder.snapshot().len(), before, "4s throttle");

        driver
            .sync_outgoing_typing("hello!!", false, 1_000 + OUTGOING_TYPING_INTERVAL_MS)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap()["action"]["@type"],
            "chatActionTyping"
        );

        driver.sync_outgoing_typing("   ", false, 9_000).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(recorder.snapshot().last().unwrap()).unwrap()["action"]["@type"],
            "chatActionCancel"
        );

        driver.sync_outgoing_typing("again", false, 10_000).unwrap();
        let snap = ComposerSnapshot::capture(ChatId(7), driver.session.view_generation, "again");
        driver.send_text_snapshot(&snap).unwrap();
        let sent = recorder.snapshot();
        assert_eq!(
            serde_json::from_str::<Value>(&sent[sent.len() - 2]).unwrap()["@type"],
            "sendMessage"
        );
        assert_eq!(
            serde_json::from_str::<Value>(sent.last().unwrap()).unwrap()["action"]["@type"],
            "chatActionCancel"
        );

        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatAction","chat_id":7,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionTyping"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(driver.session.chats.get(&7).unwrap().is_peer_typing());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn private_draft_debounces_then_flushes_and_skips_channels() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":3,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        let restored = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(restored.text, "meet at 6");
        assert_eq!(restored.reply_to_message_id, Some(MessageId(3)));
        let outcome = driver
            .note_composer_draft(ChatId(7), "meet at 6!", None, 1_000, true)
            .unwrap();
        let DraftSaveOutcome::Debounced { token, delay } = outcome else {
            panic!("expected debounce");
        };
        assert_eq!(delay, DRAFT_SAVE_DEBOUNCE);
        assert!(
            driver
                .commit_debounced_draft(token.wrapping_add(9))
                .unwrap()
                .is_none()
        );
        driver.commit_debounced_draft(token).unwrap();
        let sent = recorder.snapshot();
        let draft_json = sent.last().unwrap();
        assert!(draft_json.contains("setChatDraftMessage"));
        assert!(draft_json.contains("meet at 6!"));
        assert!(draft_json.contains("\"topic_id\":null"));
        assert_eq!(
            driver
                .session
                .chats
                .get(&7)
                .unwrap()
                .draft
                .as_ref()
                .unwrap()
                .text,
            "meet at 6!"
        );
        let flushed = driver
            .note_composer_draft(ChatId(7), "leaving", Some(MessageId(3)), 2_000, false)
            .unwrap();
        assert_eq!(flushed, DraftSaveOutcome::Sent);
        assert!(
            driver
                .note_composer_draft(ChatId(8), "nope", None, 3_000, false)
                .unwrap()
                == DraftSaveOutcome::Skipped
        );
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageSendSucceeded","message":{"id":9,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"leaving","entities":[]}}},"old_message_id":-1}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(driver.session.draft_clears, vec![ChatId(7)]);
        driver.clear_draft_after_send(ChatId(7), true).unwrap();
        assert!(driver.session.chats.get(&7).unwrap().draft.is_none());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_open_flushes_leaving_draft_and_media_send_drops_reply() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        for (id, title) in [(7, "Ada"), (8, "Bob")] {
            driver
                .ingest(
                    copy_and_parse(
                        &format!(
                            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
                        ),
                        &seq,
                        &dyn_sink,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        driver.select_chat(ChatId(7)).unwrap();
        let outcome = driver
            .note_composer_draft(ChatId(7), "hello", Some(MessageId(3)), 1_000, true)
            .unwrap();
        assert!(matches!(outcome, DraftSaveOutcome::Debounced { .. }));
        driver
            .select_search_chat(ChatId(8), "hello", Some(MessageId(3)), 1_500)
            .unwrap();
        assert_eq!(driver.session.open_chat, Some(ChatId(8)));
        let saved = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(saved.text, "hello");
        assert_eq!(saved.reply_to_message_id, Some(MessageId(3)));
        let sent = recorder.snapshot();
        assert!(sent.iter().any(|json| {
            json.contains("setChatDraftMessage")
                && json.contains("\"chat_id\":7")
                && json.contains("hello")
                && json.contains("\"message_id\":3")
        }));
        driver.select_chat(ChatId(7)).unwrap();
        driver
            .note_composer_draft(ChatId(7), "caption", Some(MessageId(3)), 2_000, false)
            .unwrap();
        driver
            .note_composer_draft(ChatId(7), "caption", None, 2_100, false)
            .unwrap();
        let after = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
        assert_eq!(after.text, "caption");
        assert_eq!(after.reply_to_message_id, None);
        driver
            .note_composer_draft(ChatId(7), "  ", Some(MessageId(9)), 3_000, false)
            .unwrap();
        driver
            .select_search_message(ChatId(8), MessageId(1), "  ", None, 3_100)
            .unwrap();
        assert!(driver.session.chats.get(&7).unwrap().draft.is_none());
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Phase 4.2: driver guards around `send_poll_answer` / `send_poll_draft`.
    type PollDriverHarness = (
        std::path::PathBuf,
        ConnectDriver<Arc<RecordingSender>>,
        Arc<RecordingSender>,
        Arc<MemorySink>,
        Arc<dyn DiagnosticSink>,
        AtomicU64,
    );
    const POLL_OPEN_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":106,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":12,"vote_percentage":55,"is_chosen":false},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":7,"vote_percentage":32,"is_chosen":false}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;
    const POLL_CLOSED_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":107,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9002,"question":{"@type":"formattedText","text":"Red planet?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Mars","entities":[]},"voter_count":18,"vote_percentage":72,"is_chosen":false}],"total_voter_count":25,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":true,"type":{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"","entities":[]}}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;

    fn poll_driver() -> PollDriverHarness {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        driver
            .ingest(copy_and_parse(POLL_OPEN_JSON, &seq, &dyn_sink).unwrap())
            .unwrap();
        driver
            .ingest(copy_and_parse(POLL_CLOSED_JSON, &seq, &dyn_sink).unwrap())
            .unwrap();
        (dir, driver, recorder, sink, dyn_sink, seq)
    }

    fn assert_invalid<T: std::fmt::Debug>(result: Result<T, ConnectSendError>) {
        match result {
            Err(ConnectSendError::InvalidRequest) => {}
            other => panic!("expected InvalidRequest, got {other:?}"),
        }
    }

    #[test]
    fn driver_poll_answer_rejects_when_chats_path_inactive() {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink);
        // No auth state seeded: `chats_path_active()` is false.
        let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(106), 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_poll_answer_rejects_unsupported_chat() {
        let (dir, mut driver, _recorder, sink, dyn_sink, seq) = poll_driver();
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":9},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_invalid(driver.send_poll_answer(ChatId(9), MessageId(106), 0));
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_poll_answer_guards_message_poll_and_index() {
        let (dir, mut driver, recorder, sink, _dyn_sink, _seq) = poll_driver();
        // Missing message.
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(404), 0));
        // Not a poll (message 50 is Alice's text message).
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(50), 0));
        // Closed poll (quiz, message 107).
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(107), 0));
        // Out-of-range option index.
        assert_invalid(driver.send_poll_answer(ChatId(7), MessageId(106), 9));
        // Valid single-answer tap: `setPollAnswer` with the 0-based position,
        // plus an optimistic chosen mark before `updatePoll` arrives.
        let extra = driver
            .send_poll_answer(ChatId(7), MessageId(106), 0)
            .unwrap();
        let send_json = recorder.snapshot().last().cloned().expect("setPollAnswer");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        assert_eq!(v["@type"], "setPollAnswer");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 106);
        assert_eq!(v["option_ids"], serde_json::json!([0]));
        let history = driver.session.histories.get(&7).unwrap();
        let message = history.messages.get(&106).unwrap();
        match &message.content {
            MessageContent::Poll(poll) => {
                assert!(poll.poll.options[0].is_chosen);
                assert!(!poll.poll.options[1].is_chosen);
            }
            other => panic!("{other:?}"),
        }
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn driver_poll_draft_guards_and_request_shape() {
        use crate::poll::PollDraft;

        let (dir, mut driver, recorder, sink, dyn_sink, seq) = poll_driver();
        let valid = PollDraft {
            question: "Lunch?".into(),
            options: vec!["Sushi".into(), "Pizza".into()],
            is_anonymous: true,
            allows_multiple_answers: false,
        };
        let invalid = PollDraft {
            question: "  ".into(),
            options: vec!["Sushi".into(), "Pizza".into()],
            is_anonymous: true,
            allows_multiple_answers: false,
        };
        // Invalid draft rejected before any request is built.
        assert_invalid(driver.send_poll_draft(ChatId(7), &invalid, None));
        // Admin-gated broadcast channel: `can_post()` is false.
        driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
        assert_invalid(driver.send_poll_draft(ChatId(13), &valid, None));
        // Valid draft: `sendMessage` + `inputMessagePoll`.
        let extra = driver
            .send_poll_draft(ChatId(7), &valid, Some(MessageId(50)))
            .unwrap();
        let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
        let v: Value = serde_json::from_str(&send_json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], extra.0.to_string());
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["reply_to"]["message_id"], 50);
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessagePoll");
        assert_eq!(content["question"]["text"], "Lunch?");
        assert_eq!(content["options"][0]["text"]["text"], "Sushi");
        assert_eq!(content["type"]["@type"], "inputPollTypeRegular");
        assert!(!sink.rendered().contains("CANARY"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
