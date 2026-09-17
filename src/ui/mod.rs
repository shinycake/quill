mod synthetic;

use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::{AuthAction, AuthView, view_for};
use quill::composer::{ComposerSnapshot, should_send_on_enter};
use quill::connect::{ConnectBlocker, ConnectGate, LiveConnect, evaluate_gate, start_live_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{AccountKey, ChatId};
use quill::platform::live_secret_store;
use quill::state::{
    ChatSummary, HistoryMessage, OutboxReceipt, Session, outgoing_status_label, unread_badge_text,
};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{AuthorizationState, MessageContent};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use synthetic::{SyntheticChat, session_text_bubble};
use zeroize::Zeroize;

actions!(
    quill_ui,
    [
        FocusSidebar,
        FocusComposer,
        LoadOlder,
        QuitApp,
        SubmitPhone,
        SubmitCode,
        SubmitPassword
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", QuitApp, None),
        KeyBinding::new("ctrl-q", QuitApp, None),
        KeyBinding::new("cmd-1", FocusSidebar, None),
        KeyBinding::new("ctrl-1", FocusSidebar, None),
        KeyBinding::new("cmd-l", FocusComposer, None),
        KeyBinding::new("ctrl-l", FocusComposer, None),
        KeyBinding::new("cmd-up", LoadOlder, None),
        KeyBinding::new("ctrl-up", LoadOlder, None),
    ]);
}

/// Startup connect classification for the status bar (no secrets).
#[derive(Clone, PartialEq, Eq)]
pub enum ConnectUiStatus {
    NeedCredentials,
    NeedTdjson,
    RestoreBlocked(&'static str),
    /// Synthetic WaitPhoneNumber surface for screenshot proof (no live TDLib).
    DemoWaitPhone,
    /// Synthetic WaitCode surface for screenshot proof (no live TDLib).
    DemoWaitCode,
    /// Synthetic WaitPassword surface for screenshot proof (no live TDLib).
    DemoWaitPassword,
    /// Injected Ready + main chat list (no live Telegram).
    DemoReadyChats,
    Live,
}

pub struct QuillApp {
    chat: Entity<SyntheticChat>,
    composer: Entity<TextareaState>,
    phone_input: Entity<TextareaState>,
    code_input: Entity<TextareaState>,
    password_input: Entity<TextareaState>,
    auth_demo: AuthorizationState,
    focus_sidebar: FocusHandle,
    connect_status: ConnectUiStatus,
    live: Option<LiveConnect>,
    status_note: String,
    /// Screenshot / synthetic demo: show the matching auth field without a live client.
    demo_auth_inputs: bool,
    /// Screenshot Ready list: same reducers as live, injected JSON only.
    demo_session: Option<Session>,
    demo_seq: AtomicU64,
    demo_sink: Arc<MemorySink>,
}

/// Forced UI surfaces for screenshot proof (no live Telegram / no real credentials).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotDemo {
    NeedTdjson,
    WaitPhone,
    WaitCode,
    WaitPassword,
    ReadyChats,
    ReadyChatsComposer,
    ReadyUnread,
    ReadyUnreadRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneMode {
    Synthetic,
    Connecting,
    Ready,
}

impl QuillApp {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
    ) -> Self {
        Self::new_with_demo(window, cx, credentials, None)
    }

    pub fn new_with_demo(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Option<TelegramCredentials>,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        let chat = cx.new(SyntheticChat::new);
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message — Enter sends, Shift+Enter newline. IME Enter must not send.")
                .auto_grow(2, 6)
                .submit_on_enter(true)
        });
        let phone_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Phone (+country code)")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let code_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Verification code")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        let password_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Two-step password")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        cx.subscribe_in(
            &composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        let text = state.read(cx).value().to_string();
                        if !text.trim().is_empty() {
                            this.submit_composer(text, window, cx);
                        }
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &phone_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.submit_phone(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &code_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.submit_code(window, cx);
                    }
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &password_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(quill::composer::enter_event_from_kit(
                        *shift, *secondary, marked,
                    )) {
                        this.submit_password(window, cx);
                    }
                }
            },
        )
        .detach();

        let demo_sink = Arc::new(MemorySink::new());
        let mut demo_session = None;
        let (connect_status, live, status_note, auth_demo) = match demo {
            Some(ScreenshotDemo::NeedTdjson) => (
                ConnectUiStatus::NeedTdjson,
                None,
                ConnectBlocker::MissingTdjson.user_message().into(),
                AuthorizationState::WaitPhoneNumber,
            ),
            Some(ScreenshotDemo::WaitPhone) => (
                ConnectUiStatus::DemoWaitPhone,
                None,
                "screenshot demo — WaitPhoneNumber (injected auth, no live Telegram)".into(),
                AuthorizationState::WaitPhoneNumber,
            ),
            Some(ScreenshotDemo::WaitCode) => (
                ConnectUiStatus::DemoWaitCode,
                None,
                "screenshot demo — WaitCode (injected auth, no live Telegram)".into(),
                AuthorizationState::WaitCode {
                    code_length: Some(5),
                },
            ),
            Some(ScreenshotDemo::WaitPassword) => (
                ConnectUiStatus::DemoWaitPassword,
                None,
                "screenshot demo — WaitPassword (injected auth, no live Telegram)".into(),
                AuthorizationState::WaitPassword {
                    has_recovery_email: true,
                },
            ),
            Some(ScreenshotDemo::ReadyChats | ScreenshotDemo::ReadyChatsComposer) => {
                demo_session = Some(seed_ready_chats_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — Ready chat list (injected updates, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyUnread) => {
                demo_session = Some(seed_ready_unread_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — unread badge (injected updates, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            Some(ScreenshotDemo::ReadyUnreadRead) => {
                demo_session = Some(seed_ready_unread_read_session(demo_sink.clone()));
                (
                    ConnectUiStatus::DemoReadyChats,
                    None,
                    "screenshot demo — after mark-read (injected updates, no live Telegram)".into(),
                    AuthorizationState::Ready,
                )
            }
            None => bootstrap_connect(credentials),
        };

        let mut app = Self {
            chat,
            composer,
            phone_input,
            code_input,
            password_input,
            auth_demo,
            focus_sidebar: cx.focus_handle(),
            connect_status,
            live,
            status_note,
            demo_auth_inputs: matches!(
                demo,
                Some(
                    ScreenshotDemo::WaitPhone
                        | ScreenshotDemo::WaitCode
                        | ScreenshotDemo::WaitPassword
                )
            ),
            demo_session,
            demo_seq: AtomicU64::new(0),
            demo_sink,
        };
        if matches!(demo, Some(ScreenshotDemo::ReadyChatsComposer)) {
            app.composer.update(cx, |input, cx| {
                input.set_value("hello from composer", window, cx);
            });
        }
        if app.live.is_some() {
            app.spawn_poll_loop(cx);
        }
        app
    }

    fn spawn_poll_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        this.poll_live(cx);
                        this.live.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_live(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let prev_auth = live.driver.session.auth.clone();
        let mut progressed = false;
        let mut send_failed = false;
        while let Some(owned) = live.bridge.next_timeout(Duration::from_millis(0)) {
            if live.driver.ingest(owned).is_err() {
                send_failed = true;
            }
            progressed = true;
        }
        let err = live.driver.session.last_auth_error;
        let new_auth = live.driver.session.auth.clone();
        let chat_count = live.driver.session.ordered_chats().len();
        let chats_exhausted = live.driver.session.chats_exhausted;
        if send_failed {
            self.status_note = "failed to send TDLib request".into();
        } else if progressed {
            if let Some(err) = err {
                self.status_note = err.user_message().into();
            } else if new_auth != prev_auth {
                self.status_note = live_status_for(&new_auth);
            } else if matches!(new_auth, AuthorizationState::Ready) {
                self.status_note = if chats_exhausted {
                    format!("signed in — {chat_count} chats")
                } else {
                    format!("signed in — loading chats ({chat_count})")
                };
            }
        }
        if progressed || send_failed {
            cx.notify();
        }
    }

    fn current_auth(&self) -> AuthorizationState {
        if let Some(live) = self.live.as_ref() {
            live.driver.session.auth.clone()
        } else if let Some(session) = self.demo_session.as_ref() {
            session.auth.clone()
        } else {
            self.auth_demo.clone()
        }
    }

    fn session(&self) -> Option<&Session> {
        self.live
            .as_ref()
            .map(|live| &live.driver.session)
            .or(self.demo_session.as_ref())
    }

    fn pane_mode(&self) -> PaneMode {
        if self
            .session()
            .is_some_and(|session| matches!(session.auth, AuthorizationState::Ready))
        {
            return PaneMode::Ready;
        }
        match self.connect_status {
            ConnectUiStatus::NeedCredentials if self.live.is_none() && !self.demo_auth_inputs => {
                PaneMode::Synthetic
            }
            _ => PaneMode::Connecting,
        }
    }

    fn submit_composer(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        match self.pane_mode() {
            PaneMode::Connecting => {
                self.status_note = "sign in before sending".into();
                cx.notify();
            }
            PaneMode::Ready => {
                if self.live.is_some() {
                    let plan = {
                        let session = &self.live.as_ref().expect("live").driver.session;
                        (
                            session.open_chat,
                            session.view_generation,
                            session.open_chat.and_then(|id| {
                                session.chats.get(&id.0).map(|chat| chat.supported())
                            }),
                        )
                    };
                    let (open_chat, view_generation, supported) = plan;
                    let Some(chat_id) = open_chat else {
                        self.status_note = "select a chat to send".into();
                        cx.notify();
                        return;
                    };
                    if supported != Some(true) {
                        self.status_note = "this chat type is not supported yet".into();
                        cx.notify();
                        return;
                    }
                    let snap = ComposerSnapshot::capture(chat_id, view_generation, text);
                    let result = self
                        .live
                        .as_mut()
                        .expect("live")
                        .driver
                        .send_text_snapshot(&snap);
                    match result {
                        Ok(_) => {
                            self.composer
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            self.status_note = "sending…".into();
                        }
                        Err(_) => {
                            self.status_note = "could not send message".into();
                        }
                    }
                    cx.notify();
                    return;
                }
                if self.demo_session.is_some() {
                    self.apply_demo_outgoing(&text);
                    self.composer
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.status_note = "demo send applied locally (no live Telegram)".into();
                    cx.notify();
                }
            }
            PaneMode::Synthetic => {
                self.chat.update(cx, |chat, cx| chat.send_text(text, cx));
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                cx.notify();
            }
        }
    }

    fn apply_demo_outgoing(&mut self, text: &str) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}}}}}"#,
            -(session.view_generation.0 as i64),
            chat_id.0,
            serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into()),
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    fn select_listed_chat(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .select_chat(chat_id);
            self.status_note = match result {
                Ok(_) => "chat selected".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(chat_id);
        }
        cx.notify();
    }

    fn load_older_action(&mut self, cx: &mut Context<Self>) {
        match self.pane_mode() {
            PaneMode::Ready => {
                if self.live.is_some() {
                    let result = self.live.as_mut().expect("live").driver.fetch_history();
                    self.status_note = match result {
                        Ok(Some(_)) => "loading older messages".into(),
                        Ok(None) => "no older messages to load".into(),
                        Err(_) => "could not load history".into(),
                    };
                }
                cx.notify();
            }
            PaneMode::Synthetic => {
                self.chat.update(cx, |chat, cx| chat.prepend_older(cx));
            }
            PaneMode::Connecting => {}
        }
    }

    fn cycle_auth(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            return;
        }
        self.auth_demo = match &self.auth_demo {
            AuthorizationState::WaitPhoneNumber => AuthorizationState::WaitCode {
                code_length: Some(5),
            },
            AuthorizationState::WaitCode { .. } => AuthorizationState::WaitPassword {
                has_recovery_email: true,
            },
            AuthorizationState::WaitPassword { .. } => AuthorizationState::WaitPremiumPurchase,
            AuthorizationState::WaitPremiumPurchase => AuthorizationState::Ready,
            AuthorizationState::Ready => AuthorizationState::WaitPhoneNumber,
            other => other.clone(),
        };
        cx.notify();
    }

    fn submit_phone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPhoneNumber
        ) {
            return;
        }
        let phone = self.phone_input.read(cx).value().to_string();
        match live.driver.submit_phone(&phone) {
            Ok(_) => {
                self.phone_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "phone submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit phone".into();
            }
        }
        cx.notify();
    }

    fn submit_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitCode { .. }
        ) {
            return;
        }
        let mut code = self.code_input.read(cx).value().to_string();
        let result = live.driver.submit_code(&code);
        code.zeroize();
        match result {
            Ok(_) => {
                self.code_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "code submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit code".into();
            }
        }
        cx.notify();
    }

    fn submit_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            AuthorizationState::WaitPassword { .. }
        ) {
            return;
        }
        let mut password = self.password_input.read(cx).value().to_string();
        let result = live.driver.submit_password(&password);
        password.zeroize();
        match result {
            Ok(_) => {
                self.password_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.status_note = "password submitted — waiting for Telegram".into();
            }
            Err(_) => {
                self.status_note = "could not submit password".into();
            }
        }
        cx.notify();
    }
}

fn bootstrap_connect(
    credentials: Option<TelegramCredentials>,
) -> (
    ConnectUiStatus,
    Option<LiveConnect>,
    String,
    AuthorizationState,
) {
    let auth_demo = AuthorizationState::WaitPhoneNumber;
    match evaluate_gate(credentials.is_some()) {
        ConnectGate::Blocked(ConnectBlocker::MissingCredentials) => (
            ConnectUiStatus::NeedCredentials,
            None,
            ConnectBlocker::MissingCredentials.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Blocked(ConnectBlocker::MissingTdjson) => (
            ConnectUiStatus::NeedTdjson,
            None,
            ConnectBlocker::MissingTdjson.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Blocked(other) => (
            ConnectUiStatus::RestoreBlocked(other.user_message()),
            None,
            other.user_message().into(),
            auth_demo,
        ),
        ConnectGate::Ready { .. } => {
            let Some(credentials) = credentials else {
                return (
                    ConnectUiStatus::NeedCredentials,
                    None,
                    ConnectBlocker::MissingCredentials.user_message().into(),
                    auth_demo,
                );
            };
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let store = live_secret_store();
            match start_live_connect(credentials, store.as_ref(), sink) {
                Ok(live) => (
                    ConnectUiStatus::Live,
                    Some(live),
                    "TDLib connected — waiting for authorization updates".into(),
                    auth_demo,
                ),
                Err(ConnectBlocker::MissingTdjson) => (
                    ConnectUiStatus::NeedTdjson,
                    None,
                    ConnectBlocker::MissingTdjson.user_message().into(),
                    auth_demo,
                ),
                Err(err) => (
                    ConnectUiStatus::RestoreBlocked(err.user_message()),
                    None,
                    err.user_message().into(),
                    auth_demo,
                ),
            }
        }
    }
}

fn live_status_for(auth: &AuthorizationState) -> String {
    match auth {
        AuthorizationState::WaitTdlibParameters => {
            "TDLib connected — waiting for authorization updates".into()
        }
        AuthorizationState::WaitPhoneNumber => "enter phone number".into(),
        AuthorizationState::WaitCode { .. } => "enter the verification code from Telegram".into(),
        AuthorizationState::WaitPassword { .. } => {
            "enter your two-step verification password".into()
        }
        AuthorizationState::Ready => "signed in — cloud chats only".into(),
        AuthorizationState::WaitOtherDeviceConfirmation => {
            "confirm on another device (QR payload is not logged)".into()
        }
        AuthorizationState::LoggingOut => "signing out".into(),
        AuthorizationState::Closing => "TDLib is closing".into(),
        AuthorizationState::Closed => "session closed".into(),
        other => view_for(other).body,
    }
}

impl Render for QuillApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let auth_state = self.current_auth();
        let auth = view_for(&auth_state);
        let inputs_live = self.live.is_some() || self.demo_auth_inputs;
        let show_phone = inputs_live && matches!(auth.action, AuthAction::EnterPhone);
        let show_code = inputs_live && matches!(auth.action, AuthAction::EnterCode);
        let show_password = inputs_live && matches!(auth.action, AuthAction::EnterPassword);
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .on_action(cx.listener(|this, _: &QuitApp, window, cx| {
                let _ = this;
                window.remove_window();
                cx.quit();
            }))
            .on_action(cx.listener(|this, _: &FocusComposer, window, cx| {
                this.composer
                    .update(cx, |input, cx| input.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &FocusSidebar, window, cx| {
                window.focus(&this.focus_sidebar, cx);
            }))
            .on_action(cx.listener(|this, _: &LoadOlder, _, cx| {
                this.load_older_action(cx);
            }))
            .on_action(cx.listener(|this, _: &SubmitPhone, window, cx| {
                this.submit_phone(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SubmitCode, window, cx| {
                this.submit_code(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SubmitPassword, window, cx| {
                this.submit_password(window, cx);
            }))
            .child(title_bar(self.pane_mode(), self.live.is_some(), cx))
            .child(
                div()
                    .id("quill-shell")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(&auth, show_phone, show_code, show_password, cx))
                    .child(self.conversation(cx)),
            )
            .child(status_bar(
                &auth,
                &self.connect_status,
                &self.status_note,
                cx,
            ))
    }
}

impl QuillApp {
    fn conversation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.pane_mode();
        let history = match mode {
            PaneMode::Synthetic => div()
                .id("conversation-history")
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(self.chat.clone())
                .into_any_element(),
            PaneMode::Connecting => pane_placeholder(
                "Not signed in",
                "Chat list and sending unlock after authorization is Ready. This window is not showing a fake inbox.",
                cx,
            )
            .into_any_element(),
            PaneMode::Ready => self.session_history(cx).into_any_element(),
        };
        let composer = match mode {
            PaneMode::Synthetic => Some(true),
            PaneMode::Connecting => None,
            PaneMode::Ready => {
                let session = self.session();
                let open = session.and_then(|s| s.open_chat);
                let supported = open
                    .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.supported())))
                    .unwrap_or(false);
                if supported { Some(true) } else { None }
            }
        };
        let composer_note = match mode {
            PaneMode::Connecting => Some("Sign in to send messages."),
            PaneMode::Ready if composer.is_none() => {
                if self.session().and_then(|s| s.open_chat).is_none() {
                    Some("Select a supported cloud chat to send.")
                } else {
                    Some("This chat type is gated until sponsored-content handling exists.")
                }
            }
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(history)
            .when(composer.is_some(), |this| {
                this.child(
                    div()
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(Textarea::new(&self.composer).h(px(88.))),
                )
            })
            .when_some(composer_note, |this, note| {
                this.child(
                    div()
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(note),
                )
            })
    }

    fn session_history(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open = session.and_then(|s| s.open_chat);
        let title = open
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.title.clone())))
            .unwrap_or_else(|| "No chat selected".into());
        let supported = open
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.supported())))
            .unwrap_or(false);
        let gate = open.and_then(|id| {
            session.and_then(|s| s.chats.get(&id.0).and_then(|c| c.kind.gate_reason()))
        });
        let messages: Vec<HistoryMessage> = open
            .and_then(|id| session.and_then(|s| s.histories.get(&id.0)))
            .map(|h| h.ordered().into_iter().cloned())
            .into_iter()
            .flatten()
            .collect();
        let chat = open.and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
        let sender_name = title.clone();
        div()
            .id("conversation-history")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .font_semibold()
                    .child(title),
            )
            .child(if let Some(reason) = gate {
                pane_placeholder("Unsupported chat", reason, cx).into_any_element()
            } else if open.is_none() {
                pane_placeholder(
                    "Select a chat",
                    "The main list is driven by loadChats + updateNewChat / updateChatPosition.",
                    cx,
                )
                .into_any_element()
            } else if !supported {
                pane_placeholder(
                    "Unsupported chat",
                    "This conversation type is not supported yet.",
                    cx,
                )
                .into_any_element()
            } else if messages.is_empty() {
                pane_placeholder(
                    "No messages yet",
                    "History arrives via getChatHistory and updates.",
                    cx,
                )
                .into_any_element()
            } else {
                let mut list = div()
                    .id("session-history")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px_3()
                    .pt_2()
                    .gap_1();
                for message in messages {
                    let body = match &message.content {
                        MessageContent::Text(text) => text.clone(),
                        MessageContent::Unsupported { type_name } => {
                            format!("({type_name})")
                        }
                    };
                    let label = if message.is_outgoing {
                        let receipt = chat
                            .map(|summary| summary.outbox_receipt(&message))
                            .unwrap_or(OutboxReceipt::Sent);
                        outgoing_status_label(message.pending, receipt).to_string()
                    } else {
                        sender_name.clone()
                    };
                    list = list.child(session_text_bubble(
                        message.id.0 as u64,
                        label,
                        body,
                        message.is_outgoing,
                    ));
                }
                list.into_any_element()
            })
    }

    fn sidebar(
        &self,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.pane_mode();
        let mut list = div()
            .id("sidebar")
            .track_focus(&self.focus_sidebar)
            .w(px(280.))
            .h_full()
            .p_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .gap_2()
            .child(div().font_semibold().child("Chats"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(chat_list_caption(mode, self.session())),
            );
        match mode {
            PaneMode::Synthetic => {
                list = list
                    .child(static_chat_row(
                        "Ada Lovelace",
                        "Mixed-height history",
                        true,
                        cx,
                    ))
                    .child(static_chat_row("RTL / emoji samples", "שלום 👨‍👩‍👧‍👦", false, cx));
            }
            PaneMode::Connecting => {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No chat list until Ready."),
                );
            }
            PaneMode::Ready => {
                let open = self.session().and_then(|s| s.open_chat);
                let chats: Vec<ChatSummary> = self
                    .session()
                    .map(|s| s.ordered_chats().into_iter().cloned().collect())
                    .unwrap_or_default();
                if chats.is_empty() {
                    let loading = self.session().is_some_and(|s| !s.chats_exhausted);
                    list = list.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loading {
                                "Loading chats…"
                            } else {
                                "No chats in the main list."
                            }),
                    );
                }
                for chat in chats {
                    let selected = open == Some(chat.id);
                    list = list.child(session_chat_row(&chat, selected, cx));
                }
            }
        }
        list = list
            .child(div().mt_4().font_semibold().child("Authorization"))
            .child(div().text_sm().child(auth.title))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(auth.body.clone()),
            )
            .child(auth_action_note(auth, &self.connect_status))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status_note.clone()),
            );
        list.when(show_phone, |this| {
            this.child(div().mt_2().font_semibold().text_sm().child("Phone"))
                .child(Textarea::new(&self.phone_input).h(px(40.)))
                .child(
                    Button::new("submit-phone")
                        .label("Submit phone")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_phone(window, cx);
                        })),
                )
        })
        .when(show_code, |this| {
            this.child(div().mt_2().font_semibold().text_sm().child("Code"))
                .child(Textarea::new(&self.code_input).h(px(40.)))
                .child(
                    Button::new("submit-code")
                        .label("Submit code")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_code(window, cx);
                        })),
                )
        })
        .when(show_password, |this| {
            this.child(
                div()
                    .mt_2()
                    .font_semibold()
                    .text_sm()
                    .child("Two-step password"),
            )
            .child(Textarea::new(&self.password_input).h(px(40.)))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sent to TDLib only — never logged"),
            )
            .child(
                Button::new("submit-password")
                    .label("Submit password")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_password(window, cx);
                    })),
            )
        })
    }
}

fn seed_ready_chats_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::ReadyChats)
}

fn seed_ready_unread_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::UnreadBadge)
}

fn seed_ready_unread_read_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::AfterMarkRead)
}

#[derive(Clone, Copy)]
enum DemoSeed {
    ReadyChats,
    UnreadBadge,
    AfterMarkRead,
}

fn seed_demo_session(sink: Arc<MemorySink>, kind: DemoSeed) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let seq = AtomicU64::new(0);
    let a_unread = match kind {
        DemoSeed::ReadyChats => 1,
        DemoSeed::UnreadBadge => 3,
        DemoSeed::AfterMarkRead => 3,
    };
    let jsons = [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#
            .to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":11,"title":"Demo chat A","type":{{"@type":"chatTypePrivate","user_id":11}},"unread_count":{a_unread},"last_read_inbox_message_id":100,"last_read_outbox_message_id":0}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":12,"title":"Demo chat B","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0,"last_read_inbox_message_id":40,"last_read_outbox_message_id":0}}"#
            .to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"20","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":13,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#
            .to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":12,"last_message":{"id":40,"chat_id":12,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Later.","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"20","is_pinned":false}]}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":102,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":103,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Two more waiting.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":40,"chat_id":12,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Later.","entities":[]}}}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    match kind {
        DemoSeed::ReadyChats => {
            session.open_chat(ChatId(11));
        }
        DemoSeed::UnreadBadge => {
            session.open_chat(ChatId(12));
        }
        DemoSeed::AfterMarkRead => {
            let follow = [
                r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Still waiting on a receipt.","entities":[]}}}}"#,
                r#"{"@type":"updateChatReadInbox","chat_id":11,"last_read_inbox_message_id":103,"unread_count":0}"#,
                r#"{"@type":"updateChatReadOutbox","chat_id":11,"last_read_outbox_message_id":102}"#,
            ];
            for json in follow {
                if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.open_chat(ChatId(11));
        }
    }
    session
}

fn pane_placeholder(
    title: &'static str,
    body: impl Into<SharedString>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(title)
        .flex()
        .flex_col()
        .flex_1()
        .p_6()
        .gap_2()
        .child(div().font_semibold().child(title))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(body.into()),
        )
}

fn chat_list_caption(mode: PaneMode, session: Option<&Session>) -> SharedString {
    match mode {
        PaneMode::Synthetic => "Synthetic".into(),
        PaneMode::Connecting => "Waiting for Ready".into(),
        PaneMode::Ready => {
            let n = session.map(|s| s.ordered_chats().len()).unwrap_or(0);
            format!("Main list · {n}").into()
        }
    }
}

fn title_bar(mode: PaneMode, live: bool, cx: &mut Context<QuillApp>) -> impl IntoElement {
    let title = match mode {
        PaneMode::Synthetic => "Quill — synthetic chat",
        PaneMode::Connecting if live => "Quill — live TDLib",
        PaneMode::Connecting => "Quill — connecting",
        PaneMode::Ready if live => "Quill — chats",
        PaneMode::Ready => "Quill — chats (demo)",
    };
    let show_cycle = mode == PaneMode::Synthetic;
    div()
        .id("title")
        .h(px(44.))
        .px_4()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().font_semibold().child(title))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .flex_none()
                .child(
                    Button::new("older")
                        .label("Load older")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.load_older_action(cx);
                        })),
                )
                .when(show_cycle, |this| {
                    this.child(
                        Button::new("cycle-auth")
                            .label("Cycle auth")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_auth(cx))),
                    )
                }),
        )
}

fn static_chat_row(
    title: &'static str,
    preview: &'static str,
    selected: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id(title)
        .px_2()
        .py_2()
        .rounded_md()
        .bg(if selected {
            cx.theme().accent.opacity(0.15)
        } else {
            cx.theme().sidebar
        })
        .child(div().font_medium().child(title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(preview),
        )
}

fn session_chat_row(
    chat: &ChatSummary,
    selected: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let id = chat.id;
    let title = chat.title.clone();
    let preview = chat.sidebar_preview();
    let badge = unread_badge_text(chat.unread_count);
    div()
        .id(("chat-row", id.0 as u64))
        .px_2()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(if selected {
            cx.theme().accent.opacity(0.15)
        } else {
            cx.theme().sidebar
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_listed_chat(id, cx);
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().font_medium().min_w_0().child(title))
                .when_some(badge, |this, label| this.child(unread_badge(label, id))),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(preview),
        )
}

fn unread_badge(label: String, chat_id: ChatId) -> impl IntoElement {
    div()
        .id(("unread-badge", chat_id.0 as u64))
        .h(px(20.))
        .min_w(px(20.))
        .px_1()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x1f6feb))
        .text_color(rgb(0xffffff))
        .text_xs()
        .font_semibold()
        .child(label)
}

fn auth_action_note(auth: &AuthView, connect_status: &ConnectUiStatus) -> impl IntoElement {
    let gate = match connect_status {
        ConnectUiStatus::NeedCredentials => "need credentials",
        ConnectUiStatus::NeedTdjson => "need tdjson",
        ConnectUiStatus::RestoreBlocked(_) => "restore blocked",
        ConnectUiStatus::DemoWaitPhone => "demo wait-phone",
        ConnectUiStatus::DemoWaitCode => "demo wait-code",
        ConnectUiStatus::DemoWaitPassword => "demo wait-password",
        ConnectUiStatus::DemoReadyChats => "demo ready-chats",
        ConnectUiStatus::Live => "live TDLib",
    };
    let label = match &auth.action {
        AuthAction::UnsupportedHalt { reason } => format!("Blocked: {reason}"),
        AuthAction::Ready => format!("Ready ({gate})"),
        AuthAction::EnterPhone => format!("Phone entry ({gate})"),
        AuthAction::EnterCode => format!("Code entry ({gate})"),
        AuthAction::EnterPassword => format!("Password entry ({gate})"),
        AuthAction::ProvideParameters => format!("Sending TDLib parameters ({gate})"),
        other => format!("{other:?} ({gate})"),
    };
    div().text_xs().child(label)
}

fn connect_status_label(status: &ConnectUiStatus) -> String {
    match status {
        ConnectUiStatus::NeedCredentials => {
            "set TELEGRAM_API_ID / TELEGRAM_API_HASH (or local .env)".into()
        }
        ConnectUiStatus::NeedTdjson => {
            "credentials loaded · tdjson missing (QUILL_TDJSON_PATH / bundle)".into()
        }
        ConnectUiStatus::RestoreBlocked(msg) => format!("credentials loaded · {msg}"),
        ConnectUiStatus::DemoWaitPhone => {
            "credentials loaded · WaitPhoneNumber (screenshot demo)".into()
        }
        ConnectUiStatus::DemoWaitCode => "credentials loaded · WaitCode (screenshot demo)".into(),
        ConnectUiStatus::DemoWaitPassword => {
            "credentials loaded · WaitPassword (screenshot demo)".into()
        }
        ConnectUiStatus::DemoReadyChats => {
            "injected Ready · chat list + composer (screenshot demo)".into()
        }
        ConnectUiStatus::Live => "credentials loaded · TDLib live".into(),
    }
}

fn status_bar(
    auth: &AuthView,
    connect_status: &ConnectUiStatus,
    status_note: &str,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id("status")
        .h(px(28.))
        .px_3()
        .flex()
        .items_center()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(format!(
            "Auth: {} · {} · {} · Keyboard: ⌘1 sidebar, ⌘L composer, ⌘↑ older · VoiceOver: macOS follow-up",
            auth.title,
            connect_status_label(connect_status),
            status_note
        ))
}

impl Focusable for QuillApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_sidebar.clone()
    }
}
