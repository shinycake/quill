mod synthetic;

use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::{AuthAction, AuthView, view_for};
use quill::composer::should_send_on_enter;
use quill::connect::{ConnectBlocker, ConnectGate, LiveConnect, evaluate_gate, start_live_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::platform::live_secret_store;
use quill::telegram::envelope::AuthorizationState;
use std::sync::Arc;
use std::time::Duration;
use synthetic::SyntheticChat;
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
}

/// Forced UI surfaces for screenshot proof (no live Telegram / no real credentials).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotDemo {
    NeedTdjson,
    WaitPhone,
    WaitCode,
    WaitPassword,
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
                            this.chat.update(cx, |chat, cx| chat.send_text(text, cx));
                            state.update(cx, |input, cx| input.set_value("", window, cx));
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
        };
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
        if send_failed {
            self.status_note = "failed to send TDLib request".into();
        } else if progressed {
            if let Some(err) = err {
                self.status_note = err.user_message().into();
            } else if new_auth != prev_auth {
                self.status_note = live_status_for(&new_auth);
            }
        }
        if progressed || send_failed {
            cx.notify();
        }
    }

    fn current_auth(&self) -> AuthorizationState {
        if let Some(live) = self.live.as_ref() {
            live.driver.session.auth.clone()
        } else {
            self.auth_demo.clone()
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
                this.chat.update(cx, |chat, cx| chat.prepend_older(cx));
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
            .child(title_bar(self.live.is_some(), cx))
            .child(
                div()
                    .id("quill-shell")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar(
                        &auth,
                        &self.connect_status,
                        &self.status_note,
                        show_phone,
                        &self.phone_input,
                        show_code,
                        &self.code_input,
                        show_password,
                        &self.password_input,
                        &self.focus_sidebar,
                        cx,
                    ))
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
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .id("conversation-history")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.chat.clone()),
            )
            .child(
                div()
                    .p_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(Textarea::new(&self.composer).h(px(88.))),
            )
    }
}

fn title_bar(live: bool, cx: &mut Context<QuillApp>) -> impl IntoElement {
    let title = if live {
        "Quill — live TDLib"
    } else {
        "Quill — synthetic chat"
    };
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
                            this.chat.update(cx, |chat, cx| chat.prepend_older(cx));
                        })),
                )
                .when(!live, |this| {
                    this.child(
                        Button::new("cycle-auth")
                            .label("Cycle auth")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_auth(cx))),
                    )
                }),
        )
}

fn sidebar(
    auth: &AuthView,
    connect_status: &ConnectUiStatus,
    status_note: &str,
    show_phone: bool,
    phone_input: &Entity<TextareaState>,
    show_code: bool,
    code_input: &Entity<TextareaState>,
    show_password: bool,
    password_input: &Entity<TextareaState>,
    focus: &FocusHandle,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id("sidebar")
        .track_focus(focus)
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
                .child("Synthetic"),
        )
        .child(chat_row("Ada Lovelace", "Mixed-height history", true, cx))
        .child(chat_row("RTL / emoji samples", "שלום 👨‍👩‍👧‍👦", false, cx))
        .child(div().mt_4().font_semibold().child("Authorization"))
        .child(div().text_sm().child(auth.title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(auth.body.clone()),
        )
        .child(auth_action_note(auth, connect_status))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(status_note.to_string()),
        )
        .when(show_phone, |this| {
            this.child(div().mt_2().font_semibold().text_sm().child("Phone"))
                .child(Textarea::new(phone_input).h(px(40.)))
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
                .child(Textarea::new(code_input).h(px(40.)))
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
            .child(Textarea::new(password_input).h(px(40.)))
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

fn chat_row(
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

fn auth_action_note(auth: &AuthView, connect_status: &ConnectUiStatus) -> impl IntoElement {
    let gate = match connect_status {
        ConnectUiStatus::NeedCredentials => "need credentials",
        ConnectUiStatus::NeedTdjson => "need tdjson",
        ConnectUiStatus::RestoreBlocked(_) => "restore blocked",
        ConnectUiStatus::DemoWaitPhone => "demo wait-phone",
        ConnectUiStatus::DemoWaitCode => "demo wait-code",
        ConnectUiStatus::DemoWaitPassword => "demo wait-password",
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
