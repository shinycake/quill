//! `Session::new`: the initial value of every reducer field.
use super::*;

impl Session {
    pub(crate) fn privacy_roundtrip_done(&mut self) {
        self.calls.privacy_pending = self.calls.privacy_pending.saturating_sub(1);
        if self.calls.privacy_pending == 0 {
            self.calls.privacy_loading = false;
            self.calls.privacy_error = false;
        }
    }

    pub fn new(account: AccountKey, diagnostics: Arc<dyn DiagnosticSink>) -> Self {
        let auth = AuthorizationState::WaitTdlibParameters;
        Self {
            account,
            account_generation: AccountGeneration(1),
            auth_view: view_for(&auth),
            auth,
            connection: ConnectionState::Initial,
            chats: HashMap::new(),
            main_order: Vec::new(),
            archive_order: Vec::new(),
            chat_order_sync: Default::default(),
            chat_list: Box::new(ChatListState::new()),
            chats_state: Box::new(ChatsState::new()),
            histories: HashMap::new(),
            history_pruning: HistoryPruning::default(),
            stale_history_requests: HashSet::new(),
            revision: 0,
            messages: Box::new(MessagesState::new()),
            media: Box::new(MediaState::new()),
            stickers: Box::new(StickersState::new()),
            groups: Box::new(GroupsState::new()),
            open_chat: None,
            window_chats: WindowViews::default(),
            active_window_chats: std::collections::BTreeSet::new(),
            app_active: true,
            settings: Box::new(SettingsState::new()),
            sync: UpdatesSync::default(),
            auth_state: AuthState::new(),
            users_state: UsersState::new(),
            calls: Box::new(CallsState::new()),
            open_topic: None,
            threads: Box::new(ThreadsState::new()),
            view_generation: ViewGeneration(1),
            requests: RequestRegistry::default(),
            shutdown: ShutdownPhase::Running,
            last_seq: 0,
            bots: Box::new(BotsState::new()),
            payments: Box::new(PaymentsState::new()),
            ton_wallet: Default::default(),
            location: Default::default(),
            search: SearchDomainState::new(),
            my_user_id: None,
            users: HashMap::new(),
            stories: Box::new(StoriesState::new()),
            diagnostics,
        }
    }
}
