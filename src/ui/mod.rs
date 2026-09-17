mod synthetic;

use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::auth::{AuthAction, AuthView, view_for};
use quill::composer::should_send_on_enter;
use quill::telegram::envelope::AuthorizationState;
use synthetic::SyntheticChat;

actions!(quill_ui, [FocusSidebar, FocusComposer, LoadOlder, QuitApp]);

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

pub struct QuillApp {
    chat: Entity<SyntheticChat>,
    composer: Entity<TextareaState>,
    auth_demo: AuthorizationState,
    focus_sidebar: FocusHandle,
    /// True when TELEGRAM_* (or local .env) credentials loaded; never stores secret values.
    credentials_present: bool,
}

impl QuillApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, credentials_present: bool) -> Self {
        let chat = cx.new(|cx| SyntheticChat::new(cx));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message — Enter sends, Shift+Enter newline. IME Enter must not send.")
                .auto_grow(2, 6)
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
        Self {
            chat,
            composer,
            auth_demo: AuthorizationState::WaitPhoneNumber,
            focus_sidebar: cx.focus_handle(),
            credentials_present,
        }
    }

    fn cycle_auth(&mut self, cx: &mut Context<Self>) {
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
}

impl Render for QuillApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let auth = view_for(&self.auth_demo);
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
            .child(title_bar(cx))
            .child(
                div()
                    .id("quill-shell")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar(
                        &auth,
                        self.credentials_present,
                        &self.focus_sidebar,
                        cx,
                    ))
                    .child(self.conversation(cx)),
            )
            .child(status_bar(&auth, self.credentials_present, cx))
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

fn title_bar(cx: &mut Context<QuillApp>) -> impl IntoElement {
    div()
        .id("title")
        .h(px(44.))
        .px_4()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().font_semibold().child("Quill — synthetic chat"))
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
                .child(
                    Button::new("cycle-auth")
                        .label("Cycle auth")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.cycle_auth(cx))),
                ),
        )
}

fn sidebar(
    auth: &AuthView,
    credentials_present: bool,
    focus: &FocusHandle,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    div()
        .id("sidebar")
        .track_focus(focus)
        .w(px(260.))
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
        .child(auth_action_note(auth, credentials_present))
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

fn auth_action_note(auth: &AuthView, credentials_present: bool) -> impl IntoElement {
    let cred = if credentials_present {
        "credentials loaded"
    } else {
        "set TELEGRAM_API_ID/HASH"
    };
    let label = match &auth.action {
        AuthAction::UnsupportedHalt { reason } => format!("Blocked: {reason}"),
        AuthAction::Ready => "Ready (synthetic)".into(),
        AuthAction::EnterPhone => format!("Phone entry ({cred}) · TDLib connect not wired yet"),
        AuthAction::EnterCode => format!("Code entry ({cred}) · TDLib connect not wired yet"),
        AuthAction::EnterPassword => {
            format!("Password entry ({cred}) · TDLib connect not wired yet")
        }
        other => format!("{other:?}"),
    };
    div().text_xs().child(label)
}

fn credentials_status_label(credentials_present: bool) -> &'static str {
    if credentials_present {
        "credentials loaded · TDLib connect not wired yet"
    } else {
        "set TELEGRAM_API_ID / TELEGRAM_API_HASH (or local .env)"
    }
}

fn status_bar(
    auth: &AuthView,
    credentials_present: bool,
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
            "Auth: {} · {} · Keyboard: ⌘1 sidebar, ⌘L composer, ⌘↑ older · VoiceOver: macOS follow-up",
            auth.title,
            credentials_status_label(credentials_present)
        ))
}

impl Focusable for QuillApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_sidebar.clone()
    }
}
