//! The two dialogs of Agent access: the request an agent files with
//! `request_access`, and the consent for "All other chats" in Settings.
//! The request is derived from `quill::agent::prompts::current`, the consent
//! from `AgentUi::consent`; neither keeps a flag of its own.
#![deny(clippy::indexing_slicing)]

use super::agent_state::{AllChatsConsent, is_secret_chat_id};
use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::agent::grants::Scope;
use quill::agent::prompts::{Ask, Prompt};
use quill::lang::{keys, tr, tr_with};
use std::cell::RefCell;
use std::rc::Rc;

/// Agent-supplied text is shown, never trusted: cap what a title or a
/// quote can take.
fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max).collect();
    clipped.push('…');
    clipped
}

fn agent_name(prompt: &Prompt) -> String {
    let name = clip(&prompt.agent, 40);
    if name.is_empty() {
        tr(keys::Q_AGENT_REQUEST_UNKNOWN_AGENT).to_string()
    } else {
        name
    }
}

/// `Claude Code wants to read "Ann Lee"`, per ask and scope.
fn request_title(prompt: &Prompt) -> String {
    let agent = agent_name(prompt);
    let write = |scope: Scope| scope == Scope::ReadWrite;
    match &prompt.ask {
        Ask::Chat { title, scope, .. } => {
            let key = if write(*scope) {
                keys::Q_AGENT_REQUEST_WRITE_CHAT
            } else {
                keys::Q_AGENT_REQUEST_READ_CHAT
            };
            tr_with(key, &[("agent", &agent), ("chat", &clip(title, 60))])
        }
        Ask::AllChats { scope } => {
            let key = if write(*scope) {
                keys::Q_AGENT_REQUEST_WRITE_ALL
            } else {
                keys::Q_AGENT_REQUEST_READ_ALL
            };
            tr_with(key, &[("agent", &agent)])
        }
        Ask::Contacts => tr_with(keys::Q_AGENT_REQUEST_CONTACTS, &[("agent", &agent)]),
    }
}

/// Opens a dialog's body and footer closures from elements built once.
fn dialog_with(
    dialog: Dialog,
    title: String,
    body: AnyElement,
    footer: impl IntoElement,
) -> Dialog {
    let body = Rc::new(RefCell::new(Some(body)));
    dialog
        .overlay(true)
        .title(super::shell::dialog_title(title))
        .content(super::shell::scrollable_dialog_content(
            move |content, _, _| {
                let body = body
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| div().into_any_element());
                content.child(body)
            },
        ))
        .footer(footer)
}

fn footer_row() -> Div {
    div().flex().justify_end().gap_2()
}

impl QuillApp {
    /// A request from an agent is waiting for the active account.
    pub(super) fn agent_request_open(&self) -> bool {
        self.agent_request().is_some()
    }

    /// The consent for "All other chats" is open.
    pub(super) fn agent_consent_open(&self) -> bool {
        self.agent_ui.consent.is_some()
    }

    pub(super) fn build_agent_request_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let Some(prompt) = app.read(cx).agent_request() else {
            return dialog.overlay(true);
        };
        let on_close = {
            let prompt = prompt.clone();
            QuillShell::on_close_kind(app, shell, DialogKind::AgentRequest, move |this, _, cx| {
                this.answer_agent_request(&prompt, false, cx);
            })
        };
        let muted = cx.theme().muted_foreground;
        let quote_bg = cx.theme().muted;
        let radius = cx.theme().radius;
        let mut body = div().id("agent-request").flex().flex_col().gap_3();
        let reason = clip(&prompt.reason, 600);
        if !reason.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().text_color(muted).child(tr_with(
                        keys::Q_AGENT_REQUEST_REASON,
                        &[("agent", &agent_name(&prompt))],
                    )))
                    .child(
                        div()
                            .id("agent-request-reason")
                            .px_3()
                            .py_2()
                            .rounded(radius)
                            .bg(quote_bg)
                            .text_sm()
                            .child(format!("\"{reason}\"")),
                    ),
            );
        }
        match &prompt.ask {
            Ask::Chat {
                chat_id, secret, ..
            } if *secret || is_secret_chat_id(*chat_id) => {
                body = body.child(Alert::warning(
                    "agent-request-secret",
                    tr(keys::Q_AGENT_REQUEST_SECRET),
                ));
            }
            Ask::AllChats { .. } => {
                body = body.child(Alert::warning(
                    "agent-request-all",
                    tr(keys::Q_AGENT_REQUEST_ALL_CHATS),
                ));
            }
            Ask::Contacts => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(keys::Q_AGENT_REQUEST_CONTACTS_HINT)),
                );
            }
            Ask::Chat { .. } => {}
        }
        body = body.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(tr(keys::Q_AGENT_REQUEST_LATER)),
        );
        let title = request_title(&prompt);
        let footer = app.update(cx, |_, cx| {
            let answer = |allow: bool, prompt: Prompt, shell: Entity<QuillShell>| {
                cx.listener(move |this, _, window, cx| {
                    this.answer_agent_request(&prompt, allow, cx);
                    // The next request, if any, gets a dialog of its own.
                    shell.update(cx, |shell, _| {
                        if shell.open_dialog == Some(DialogKind::AgentRequest) {
                            shell.open_dialog = None;
                        }
                    });
                    window.close_dialog(cx);
                })
            };
            footer_row()
                .child(
                    Button::new("agent-request-deny")
                        .label(tr(keys::Q_AGENT_REQUEST_DENY))
                        .ghost()
                        .on_click(answer(false, prompt.clone(), shell.clone())),
                )
                .child(
                    Button::new("agent-request-allow")
                        .label(tr(keys::Q_AGENT_REQUEST_ALLOW))
                        .primary()
                        .on_click(answer(true, prompt.clone(), shell.clone())),
                )
        });
        dialog_with(dialog, title, body.into_any_element(), footer).on_close(on_close)
    }

    pub(super) fn build_agent_consent_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let scope = app
            .read(cx)
            .agent_ui
            .consent
            .as_ref()
            .map_or(Scope::Read, |AllChatsConsent { scope, .. }| *scope);
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::AgentConsent, |this, _, cx| {
                this.cancel_all_chats(cx);
            });
        let muted = cx.theme().muted_foreground;
        let what = if scope == Scope::ReadWrite {
            keys::Q_AGENT_CONSENT_READ_WRITE
        } else {
            keys::Q_AGENT_CONSENT_READ
        };
        let body = div()
            .id("agent-consent")
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().child(tr(what)))
            .child(Alert::warning(
                "agent-consent-terms",
                tr(keys::Q_AGENT_CONSENT_TERMS),
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_AGENT_CONSENT_OFF)),
            )
            .into_any_element();
        let footer = app.update(cx, |_, cx| {
            footer_row()
                .child(
                    Button::new("agent-consent-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_all_chats(cx);
                            this.close_kit_dialog_if_done(DialogKind::AgentConsent, window, cx);
                        })),
                )
                .child(
                    Button::new("agent-consent-allow")
                        .label(tr(keys::Q_AGENT_CONSENT_ALLOW))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_all_chats(cx);
                            this.close_kit_dialog_if_done(DialogKind::AgentConsent, window, cx);
                        })),
                )
        });
        dialog_with(
            dialog,
            tr(keys::Q_AGENT_CONSENT_TITLE).to_string(),
            body,
            footer,
        )
        .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    /// An agent asks for a chat, all chats or the contacts.
    AgentRequest => DialogSpec::new(
        1055,
        |app| app.agent_request_open(),
        QuillApp::build_agent_request_dialog,
    ),

    /// Settings: the user is about to give agents all chats.
    AgentConsent => DialogSpec::new(
        1056,
        |app| app.agent_consent_open(),
        QuillApp::build_agent_consent_dialog,
    ),
}

/// `QUILL_DEMO_AGENT` picks the request: `chat` (default), `write`,
/// `secret`, `all`, `all-write` or `contacts`.
fn demo_request(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    let mode = std::env::var("QUILL_DEMO_AGENT").unwrap_or_default();
    let chat = |chat_id: i64, title: &str, secret: bool, scope: Scope| Ask::Chat {
        chat_id,
        title: title.to_string(),
        secret,
        scope,
    };
    let ask = match mode.as_str() {
        "write" => chat(7, "Ann Lee", false, Scope::ReadWrite),
        "secret" => chat(-1_999_000_000_000, "Ann Lee", true, Scope::Read),
        "all" => Ask::AllChats { scope: Scope::Read },
        "all-write" => Ask::AllChats {
            scope: Scope::ReadWrite,
        },
        "contacts" => Ask::Contacts,
        _ => chat(7, "Ann Lee", false, Scope::Read),
    };
    let _ = quill::agent::prompts::file(
        super::agent_state::active_account(),
        "Claude Code".into(),
        ask,
        "You asked me to summarize last week's trip planning with Ann.".into(),
    );
}

/// The consent for "All other chats" over the Agent access page.
fn demo_consent(app: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    app.open_settings(None);
    app.settings.page = Some(quill::settings_catalog::Page::Advanced);
    app.settings.sub = Some(super::settings_window::SubPage::AgentAccess);
    app.agent_ui.consent = Some(AllChatsConsent {
        account: super::agent_state::active_account(),
        scope: Scope::Read,
    });
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-request").setup(demo_request),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-consent").setup(demo_consent),
];

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_dialogs_tests.rs"]
mod tests;
