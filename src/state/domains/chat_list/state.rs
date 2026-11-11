//! The chat list: folders, archive, unread counts, limits, suggestions and previews: the `chat_list` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

/// The Share Folder box's data for one folder: what each of its two
/// fetches stands at, with the answer inside (`Fetch`).
#[derive(Debug, Default)]
pub struct FolderShare {
    /// `getChatFolderInviteLinks` (creates and edits are applied on top).
    pub links: crate::state::fetch::Fetch<Vec<ChatFolderInviteLink>>,
    /// `getChatsForChatFolderInviteLink`: the chats a link may include.
    pub chats: crate::state::fetch::Fetch<Vec<i64>>,
}

pub struct ChatListState {
    /// Phase 7.1: folder list from `updateChatFolders` (schema 1.8.67 line
    /// 10606). Empty until TDLib pushes the update (after authorization).
    pub chat_folders: Vec<ChatFolderInfo>,
    /// `main_chat_list_position` from `updateChatFolders`
    /// (`schema/td_api.tl:11425`): where "All chats" sits among the folder
    /// tabs. Non-zero only with Premium; moved by `reorderChatFolders`.
    pub main_list_position: usize,
    /// Parity slice: `are_tags_enabled` from `updateChatFolders` (schema
    /// 1.8.67 line 10606). When true, chat rows render folder-name tag
    /// chips; toggled via `toggleChatFolderTags` (`:13376`).
    pub are_folder_tags_enabled: bool,
    /// Parity slice: cached full `chatFolder` specs from `getChatFolder`
    /// responses (keyed by folder id) — edit dialog prefill and the
    /// remove-from-folder chain. Dropped when the folder is edited
    /// through the dialog or deleted.
    ///
    /// Each folder's entry also says whether the spec may be behind the
    /// server (an edit must start from a fresh one,
    /// [`ChatListState::fresh_spec`]) and why its last fetch failed.
    pub folder_specs: FolderSpecs,
    /// Parity slice: `getChatListsToAddChat` results per chat id — the chat
    /// lists a chat may be added to. Drives the per-chat folder picker as
    /// the schema intends (`:13347` doc comment).
    pub chat_lists_for_add: HashMap<i64, crate::state::fetch::Fetch<Vec<ChatList>>>,
    /// Parity slice: where each folder's `loadChats(chatListFolder)` paging
    /// is. A folder without an entry has more pages; a failed page waits
    /// and asks again like the main list (`ListPaging::Retrying`).
    pub folder_paging: HashMap<i32, ListPaging>,
    /// Parity slice: queued remove-from-folder intents `(chat_id,
    /// folder_id)`. The driver sends `getChatFolder`, then `editChatFolder`
    /// with the chat dropped from the spec — there is no
    /// `removeChatFromList` in 1.8.67.
    pub folder_remove_queue: Vec<(ChatId, i32)>,
    /// Basic groups whose `leaveChat` was answered: the driver now sends
    /// `deleteChatHistory` with `remove_from_chat_list` for each.
    pub delete_after_leave: Vec<ChatId>,
    /// Parity slice: `getChatFolderChatsToLeave` results per folder id —
    /// chats the delete-confirm dialog offers to leave with the folder.
    pub folder_chats_to_leave: HashMap<i32, crate::state::fetch::Fetch<Vec<i64>>>,
    /// The Share Folder box's data per folder id: the invite links and the
    /// chats a link may include, each with its own load stage.
    pub folder_shares: HashMap<i32, FolderShare>,
    /// `getRecommendedChatFolders` result; `None` until fetched.
    pub recommended_folders: Option<Vec<RecommendedChatFolder>>,
    /// What the requests a folder or contact box waits on ended in, by
    /// request id ([`DialogAnswers`]).
    pub dialog_answers: DialogAnswers,
    /// `getChatFolderNewChats` answers per shared folder: chats its owner
    /// added since the user last looked (the "N new chats" bar).
    pub folder_new_chats: HashMap<i32, Vec<i64>>,
    /// When each folder's new chats were last asked for (TDLib wants one
    /// call per `chat_folder_new_chats_update_period`).
    pub folder_new_chats_asked: HashMap<i32, std::time::Instant>,
    /// Folder limits from TDLib options and `getPremiumLimit`.
    pub folder_limits: crate::folder_limits::FolderLimits,
    /// Folder requests that failed on a limit, oldest first, until the UI
    /// shows each one's box in the window of the box that sent the request
    /// (its id). Two hits between polls both show
    /// ([`ChatListState::take_folder_limit_hit`]).
    pub folder_limit_hits:
        std::collections::VecDeque<(crate::folder_limits::FolderLimitKind, Option<RequestId>)>,
    /// Slice CL1: `getOption("pinned_chat_count_max")` /
    /// `getOption("pinned_archived_chat_count_max")` via `updateOption`
    /// (schema 1.8.67, line 13674). Defaults 5 / 100 are TDLib's
    /// compiled defaults; the server raises them for Premium. Used for
    /// the client-side pin-limit pre-check (TGX `ChatsController`
    /// `PinTooMuchWarn` / `ErrorPinnedChatsLimit` behavior).
    pub pinned_chat_count_max: i32,
    pub pinned_archived_chat_count_max: i32,
    /// What the chat-list suggestions block shows from.
    pub suggestions: crate::chatlist_suggestions::SuggestionFacts,
    /// TDLib's authoritative unread totals for the main and archive chat
    /// lists (`updateUnreadMessageCount` / `updateUnreadChatCount`); the
    /// badge uses these instead of summing the (paginated) loaded chats.
    pub unread_totals: UnreadTotals,
    /// Moves whenever something the unread badges count may have changed
    /// (see `Session::note_unread_inputs`).
    pub unread_revision: u64,
    /// `updateUnreadChatCount` for each chat folder (tab counters).
    pub folder_unread_chats: HashMap<i32, UnreadPair>,
    /// Where `loadChats(chatListMain)` paging stands: more pages, waiting
    /// out a failure, or exhausted (a 404) ([`ListPaging`]).
    pub main_paging: ListPaging,
    /// The same for `loadChats(chatListArchive)` (paging starts once the
    /// main list is exhausted).
    pub archive_paging: ListPaging,
    /// Slice CL2: archive auto-settings from `getArchiveChatListSettings`
    /// (schema 1.8.67, line 13421). Idle until the first fetch; the
    /// archive-settings panel fetches on open (TGX
    /// `SettingsArchiveChatListController` does the same). A cached copy
    /// stays readable (`Fetch::data`) while it reloads or after a reload
    /// fails.
    pub archive_settings: crate::state::fetch::Fetch<ArchiveChatListSettings>,
    /// Slice CL2: the archive-settings panel is open.
    pub archive_settings_open: bool,
    /// Whether "Archive and mute" is offered (server option, seen on).
    pub archive_mute: crate::archive_mute::ArchiveMute,
    /// Slice CL: chat-list peek preview — the latest `getChatHistory`
    /// result for one unopened chat (`parity:chatlist-chat-preview`).
    pub chat_preview_fetch: HashMap<ChatId, crate::state::fetch::Fetch<Vec<ParsedMessage>>>,
    /// Chats TDLib placed in the main list as a promotion
    /// (`chatPosition.source`: a proxy sponsor or a public service
    /// announcement), tdesktop's `useTopPromotion`.
    pub promoted_chats: HashMap<i64, crate::promotion::Promotion>,
    /// The server's badge text per announcement type.
    pub psa_labels: crate::promotion::PsaLabels,
    /// The one-time "about this announcement" box.
    pub psa_about: crate::psa_about::PsaAbout,
    /// The history page preloaded for the pull to the next channel
    /// (`pull_next`), per chat: asked, or arrived. The request's error
    /// drops the entry, and a new pull asks again.
    pub history_preloads: std::collections::HashMap<i64, HistoryPreload>,
}

/// One chat's preloaded history page for the pull to the next channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryPreload {
    /// This request's answer makes the history ready.
    Loading(RequestId),
    Ready,
}

impl ChatListState {
    /// The folder's paging stage; `More` until a page answers.
    pub fn folder_paging_of(&self, folder_id: i32) -> ListPaging {
        self.folder_paging
            .get(&folder_id)
            .copied()
            .unwrap_or_default()
    }

    /// A folder's `loadChats` page went through.
    pub(crate) fn folder_page_ok(&mut self, folder_id: i32) {
        if let Some(paging) = self.folder_paging.get_mut(&folder_id) {
            paging.page_ok();
        }
    }

    /// A folder's `loadChats` page failed with `code`.
    pub(crate) fn folder_page_failed(
        &mut self,
        folder_id: i32,
        code: i32,
        now: std::time::Instant,
    ) {
        self.folder_paging
            .entry(folder_id)
            .or_default()
            .page_failed(code, now);
    }

    /// The next channel's history page arrived.
    pub fn history_preloaded(&self, chat: i64) -> bool {
        self.history_preloads.get(&chat) == Some(&HistoryPreload::Ready)
    }

    /// The folder tab order: the folders with "All chats" at its position.
    pub fn folder_order(&self) -> crate::folder_order::FolderOrder {
        crate::folder_order::FolderOrder {
            folder_ids: self.chat_folders.iter().map(|f| f.id).collect(),
            main_position: self.main_list_position,
        }
    }

    /// Puts the folders and "All chats" in `order` (an optimistic reorder;
    /// folders it does not name keep their place after the named ones).
    pub fn apply_folder_order(&mut self, order: &crate::folder_order::FolderOrder) {
        let index: HashMap<i32, usize> = order
            .folder_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        self.chat_folders
            .sort_by_key(|f| index.get(&f.id).copied().unwrap_or(usize::MAX));
        self.main_list_position = order.main_position;
    }

    /// `folder_id`'s spec when nothing changed the folder since it was
    /// fetched: the only spec an edit may start from.
    pub fn fresh_spec(&self, folder_id: i32) -> Option<&ChatFolderSpec> {
        self.folder_specs.fresh(folder_id)
    }

    /// A `getChatFolder` answer: the folder's spec as it is now.
    pub(crate) fn store_spec(&mut self, folder_id: i32, spec: ChatFolderSpec) {
        self.folder_specs.insert(folder_id, spec);
    }

    /// Something changed `folder_id`: its cached spec stays for display,
    /// but the next edit fetches it again.
    pub(crate) fn mark_spec_stale(&mut self, folder_id: i32) {
        self.folder_specs.mark_stale(folder_id);
    }

    /// Some folder changed (`updateChatFolders` names no folder): every
    /// cached spec is fetched again before an edit.
    pub(crate) fn mark_specs_stale(&mut self) {
        self.folder_specs.mark_all_stale();
    }

    /// The next folder request that failed on a limit, for the UI to show.
    pub fn take_folder_limit_hit(
        &mut self,
    ) -> Option<(crate::folder_limits::FolderLimitKind, Option<RequestId>)> {
        self.folder_limit_hits.pop_front()
    }

    /// The archive settings, when fetched or edited.
    pub fn archive_chat_list_settings(&self) -> Option<ArchiveChatListSettings> {
        self.archive_settings.data().copied()
    }

    /// The archive-settings panel is fetching its first copy.
    pub fn archive_settings_loading(&self) -> bool {
        self.archive_settings.is_loading()
    }

    /// An optimistic edit of the archive settings, or the rollback of one
    /// (`None`: there was no copy). A fetch in flight keeps running.
    pub fn edit_archive_settings(&mut self, settings: Option<ArchiveChatListSettings>) {
        match (settings, self.archive_settings.data_mut()) {
            (Some(settings), Some(held)) => *held = settings,
            (Some(settings), None) => self.archive_settings.insert_data_if_missing(|| settings),
            (None, _) => {
                if !self.archive_settings.is_loading() {
                    self.archive_settings = Default::default();
                }
            }
        }
    }

    pub(crate) fn new() -> Self {
        Self {
            chat_folders: Vec::new(),
            main_list_position: 0,
            are_folder_tags_enabled: false,
            folder_specs: FolderSpecs::default(),
            chat_lists_for_add: HashMap::new(),
            folder_paging: HashMap::new(),
            folder_remove_queue: Vec::new(),
            delete_after_leave: Vec::new(),
            folder_chats_to_leave: HashMap::new(),
            folder_shares: HashMap::new(),
            recommended_folders: None,
            dialog_answers: DialogAnswers::default(),
            folder_new_chats: HashMap::new(),
            folder_new_chats_asked: HashMap::new(),
            folder_limits: crate::folder_limits::FolderLimits::default(),
            folder_limit_hits: Default::default(),
            // Slice CL1: TDLib's compiled defaults for the pin limits
            // (schema 1.8.67, line 13674); `updateOption` overrides.
            pinned_chat_count_max: 5,
            pinned_archived_chat_count_max: 100,
            suggestions: Default::default(),
            unread_totals: UnreadTotals::default(),
            unread_revision: 0,
            folder_unread_chats: HashMap::new(),
            main_paging: ListPaging::More,
            archive_paging: ListPaging::More,
            archive_settings: Default::default(),
            archive_settings_open: false,
            archive_mute: Default::default(),
            chat_preview_fetch: HashMap::new(),
            promoted_chats: HashMap::new(),
            psa_labels: crate::promotion::PsaLabels::default(),
            psa_about: crate::psa_about::PsaAbout::default(),
            history_preloads: std::collections::HashMap::new(),
        }
    }
}
