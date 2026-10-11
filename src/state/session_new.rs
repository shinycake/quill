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
            chat_list: ChatListState::new(),
            chats_state: ChatsState::new(),
            histories: HashMap::new(),
            stale_history_requests: HashSet::new(),
            revision: 0,
            messages: MessagesState::new(),
            media: MediaState::new(),
            stickers: StickersState::new(),
            groups: GroupsState::new(),
            open_chat: None,
            app_active: true,
            settings: SettingsState::new(),
            sync: UpdatesSync::default(),
            auth_state: AuthState::new(),
            users_state: UsersState::new(),
            calls: CallsState::new(),
            open_topic: None,
            threads: ThreadsState::new(),
            view_generation: ViewGeneration(1),
            requests: RequestRegistry::default(),
            shutdown: ShutdownPhase::Running,
            last_seq: 0,
            bots: BotsState::new(),
            payments: PaymentsState::new(),
            search: SearchDomainState::new(),
            my_user_id: None,
            users: HashMap::new(),
            stories: StoriesState::new(),
            diagnostics,
        }
    }
}
