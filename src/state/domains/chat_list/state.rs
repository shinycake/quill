//! The chat list: folders, archive, unread counts, limits, suggestions and previews: the `chat_list` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

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
    /// remove-from-folder chain. Stale entries are dropped when the folder
    /// is edited or deleted.
    pub folder_specs: HashMap<i32, ChatFolderSpec>,
    /// Parity slice: `getChatListsToAddChat` results per chat id — the chat
    /// lists a chat may be added to. Drives the per-chat folder picker as
    /// the schema intends (`:13347` doc comment).
    pub chat_lists_for_add: HashMap<i64, Vec<ChatList>>,
    /// Parity slice: folder ids whose `loadChats(chatListFolder)` paging hit
    /// 404 — no "load more" for these folders.
    pub folder_chats_exhausted: HashSet<i32>,
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
    pub folder_chats_to_leave: HashMap<i32, Vec<i64>>,
    /// `getChatFolderInviteLinks` results (and creates/edits applied on top)
    /// per folder id — the Share Folder dialog's link list.
    pub folder_invite_links: HashMap<i32, Vec<ChatFolderInviteLink>>,
    /// `getChatsForChatFolderInviteLink` results per folder id — the chats a
    /// link may include.
    pub folder_link_chats: HashMap<i32, Vec<i64>>,
    /// `getRecommendedChatFolders` result; `None` until fetched.
    pub recommended_folders: Option<Vec<RecommendedChatFolder>>,
    /// One-shot: a folder link create/edit was applied (the dialog returns
    /// to its list). Drained by the UI.
    pub folder_link_saved: bool,
    /// One-shot: a Share Folder / recommended-folder request failed; the
    /// text is shown in the dialog. Drained by the UI.
    pub folder_share_error: Option<String>,
    /// The `addlist` link being checked by the "Add folder" dialog.
    pub folder_invite_link: Option<String>,
    /// `checkChatFolderInviteLink` answer for `folder_invite_link`.
    pub folder_invite_info: Option<ChatFolderInviteLinkInfo>,
    /// The "Add folder" dialog's failure text (check or add).
    pub folder_invite_error: Option<String>,
    /// `addChatFolderByInviteLink` confirmed.
    pub folder_invite_done: bool,
    /// `getChatFolderNewChats` answers per shared folder: chats its owner
    /// added since the user last looked (the "N new chats" bar).
    pub folder_new_chats: HashMap<i32, Vec<i64>>,
    /// When each folder's new chats were last asked for (TDLib wants one
    /// call per `chat_folder_new_chats_update_period`).
    pub folder_new_chats_asked: HashMap<i32, std::time::Instant>,
    /// Folder limits from TDLib options and `getPremiumLimit`.
    pub folder_limits: crate::folder_limits::FolderLimits,
    /// One-shot: a folder request failed on a limit; the UI shows the box.
    /// Drained by the UI.
    pub folder_limit_hit: Option<crate::folder_limits::FolderLimitKind>,
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
    /// `updateUnreadChatCount` for each chat folder (tab counters).
    pub folder_unread_chats: HashMap<i32, UnreadPair>,
    pub chats_exhausted: bool,
    /// `loadChats(chatListArchive)` answered 404 — the archive is fully
    /// loaded (paging starts once the main list is exhausted).
    pub archive_chats_exhausted: bool,
    /// Slice CL2: archive auto-settings from `getArchiveChatListSettings`
    /// (schema 1.8.67, line 13421). `None` until the first fetch; the
    /// archive-settings panel fetches on open (TGX
    /// `SettingsArchiveChatListController` does the same).
    pub archive_chat_list_settings: Option<ArchiveChatListSettings>,
    /// Slice CL2: the archive-settings panel is fetching its truth.
    pub archive_settings_loading: bool,
    /// Slice CL2: the archive-settings panel is open.
    pub archive_settings_open: bool,
    /// Whether "Archive and mute" is offered (server option, seen on).
    pub archive_mute: crate::archive_mute::ArchiveMute,
    /// Slice CL: chat-list peek preview — the latest `getChatHistory`
    /// result for one unopened chat (`parity:chatlist-chat-preview`).
    pub chat_preview_fetch: Option<PreviewHistoryFetch>,
    /// Chats TDLib placed in the main list as a promotion
    /// (`chatPosition.source`: a proxy sponsor or a public service
    /// announcement), tdesktop's `useTopPromotion`.
    pub promoted_chats: HashMap<i64, crate::promotion::Promotion>,
    /// The server's badge text per announcement type.
    pub psa_labels: crate::promotion::PsaLabels,
    /// The one-time "about this announcement" box.
    pub psa_about: crate::psa_about::PsaAbout,
    /// Chats whose history page was preloaded for the pull to the next
    /// channel (`pull_next`): the answer arrived.
    pub preloaded_histories: HashSet<i64>,
}

impl ChatListState {
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

    pub(crate) fn new() -> Self {
        Self {
            chat_folders: Vec::new(),
            main_list_position: 0,
            are_folder_tags_enabled: false,
            folder_specs: HashMap::new(),
            chat_lists_for_add: HashMap::new(),
            folder_chats_exhausted: HashSet::new(),
            folder_remove_queue: Vec::new(),
            delete_after_leave: Vec::new(),
            folder_chats_to_leave: HashMap::new(),
            folder_invite_links: HashMap::new(),
            folder_link_chats: HashMap::new(),
            recommended_folders: None,
            folder_link_saved: false,
            folder_share_error: None,
            folder_invite_link: None,
            folder_invite_info: None,
            folder_invite_error: None,
            folder_invite_done: false,
            folder_new_chats: HashMap::new(),
            folder_new_chats_asked: HashMap::new(),
            folder_limits: crate::folder_limits::FolderLimits::default(),
            folder_limit_hit: None,
            // Slice CL1: TDLib's compiled defaults for the pin limits
            // (schema 1.8.67, line 13674); `updateOption` overrides.
            pinned_chat_count_max: 5,
            pinned_archived_chat_count_max: 100,
            suggestions: Default::default(),
            unread_totals: UnreadTotals::default(),
            folder_unread_chats: HashMap::new(),
            chats_exhausted: false,
            archive_chats_exhausted: false,
            archive_chat_list_settings: None,
            archive_settings_loading: false,
            archive_settings_open: false,
            archive_mute: Default::default(),
            chat_preview_fetch: None,
            promoted_chats: HashMap::new(),
            psa_labels: crate::promotion::PsaLabels::default(),
            psa_about: crate::psa_about::PsaAbout::default(),
            preloaded_histories: HashSet::new(),
        }
    }
}
