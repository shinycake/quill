//! "Create Bot" (Telegram Desktop `boxes/peers/create_managed_bot_box.cpp`),
//! reached from a `tg://newbot?manager=&username=&name=` link: a name, a
//! username that always ends with "bot", a live availability check, then
//! `createBot` managed by the bot in the link. Rules: `quill::owned_bots`.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::owned_bots::{self, NAME_MAX, UsernameProblem};
use quill::state::DeepLinkAction;
use quill::telegram::envelope::UsernameCheckResult;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Wait after the last keystroke before asking the server.
const CHECK_DELAY: Duration = Duration::from_millis(350);

pub(super) struct BotCreateDialog {
    manager_id: i64,
    name: Entity<InputState>,
    username: Entity<InputState>,
    /// Counts keystrokes, so only the last one starts a check.
    version: u64,
    /// The person pressed Create and the answer is awaited.
    submitted: bool,
    _subscription: Subscription,
}

impl QuillApp {
    /// A managed-bot link resolved to the manager's chat: open the dialog,
    /// or say why not (tdesktop `lng_create_bot_no_manage`).
    pub(super) fn run_managed_bot_link(
        &mut self,
        chat_id: quill::ids::ChatId,
        action: &DeepLinkAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let DeepLinkAction::CreateManagedBot {
            suggested_username,
            suggested_name,
            ..
        } = action
        else {
            return false;
        };
        let Some(manager_id) = self.session().and_then(|s| s.private_chat_user_id(chat_id)) else {
            return false;
        };
        let Some(user) = self.session().and_then(|s| s.user(manager_id)).cloned() else {
            return false;
        };
        if !user.is_bot {
            return true;
        }
        if !user.can_manage_bots {
            self.connection.status_note = format!(
                "@{} doesn't have Bot Management Mode enabled.",
                user.username
            );
            cx.notify();
            return true;
        }
        self.open_bot_create(
            manager_id,
            suggested_name.clone(),
            suggested_username.clone(),
            window,
            cx,
        );
        true
    }

    fn open_bot_create(
        &mut self,
        manager_id: i64,
        suggested_name: String,
        suggested_username: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver.reset_create_bot();
        }
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Bot name")
                .submit_on_enter(false)
        });
        let username = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Bot username")
                .submit_on_enter(false)
        });
        name.update(cx, |input, cx| input.set_value(suggested_name, window, cx));
        let subscription = cx.subscribe_in(
            &username,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.bot_username_changed(input, window, cx);
                }
            },
        );
        self.dialogs.bot_create_dialog = Some(BotCreateDialog {
            manager_id,
            name,
            username: username.clone(),
            version: 0,
            submitted: false,
            _subscription: subscription,
        });
        let typed = suggested_username.trim_start_matches('@').to_string();
        username.update(cx, |input, cx| input.set_value(typed, window, cx));
        cx.notify();
    }

    /// Keep the field within the length limit, then check the name after a
    /// pause.
    fn bot_username_changed(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typed = input.read(cx).value().to_string();
        let fitted = owned_bots::fit_username(&typed);
        if fitted != typed {
            input.update(cx, |input, cx| input.set_value(fitted, window, cx));
            return;
        }
        let Some(dialog) = self.dialogs.bot_create_dialog.as_mut() else {
            return;
        };
        dialog.version += 1;
        let version = dialog.version;
        cx.notify();
        if owned_bots::username_problem(&typed).is_some() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHECK_DELAY).await;
            let _ = this.update(cx, |this, cx| this.check_bot_username(version, cx));
        })
        .detach();
    }

    fn check_bot_username(&mut self, version: u64, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.bot_create_dialog.as_ref() else {
            return;
        };
        if dialog.version != version {
            return;
        }
        let full = owned_bots::full_username(dialog.username.read(cx).value().as_ref());
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_new_bot_username(&full);
        }
        cx.notify();
    }

    fn close_bot_create(&mut self, cx: &mut Context<Self>) {
        if self.dialogs.bot_create_dialog.take().is_some()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.reset_create_bot();
        }
        cx.notify();
    }

    fn submit_bot_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.bot_create_dialog.as_mut() else {
            return;
        };
        if dialog.submitted {
            return;
        }
        let name = dialog.name.read(cx).value().trim().to_string();
        let typed = dialog.username.read(cx).value().to_string();
        let manager_id = dialog.manager_id;
        if name.is_empty()
            || name.chars().count() > NAME_MAX
            || owned_bots::username_problem(&typed).is_some()
        {
            return;
        }
        let full = owned_bots::full_username(&typed);
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: creating a bot needs a live session.".into();
            cx.notify();
            return;
        };
        if live
            .driver
            .create_managed_bot(manager_id, &name, &full, true)
            .is_err()
        {
            self.connection.status_note = "Could not send the request.".into();
            cx.notify();
            return;
        }
        dialog.submitted = true;
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..200 {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let Ok(done) = this.update_in(cx, |this, window, cx| {
                    this.poll_bot_create(&name, &full, window, cx)
                }) else {
                    break;
                };
                if done {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Watch the `createBot` answer; `true` when it is settled.
    fn poll_bot_create(
        &mut self,
        name: &str,
        full: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.dialogs.bot_create_dialog.is_none() {
            return true;
        }
        let Some(create) = self.session().map(|s| s.bots.owned.create.clone()) else {
            return true;
        };
        if create.created.is_some() {
            let manager_id = self
                .dialogs
                .bot_create_dialog
                .as_ref()
                .map(|d| d.manager_id);
            let manager = manager_id
                .and_then(|id| self.session().and_then(|s| s.user(id)))
                .map(|u| u.first_name.clone())
                .unwrap_or_default();
            self.close_bot_create(cx);
            window.close_dialog(cx);
            // tdesktop opens the new bot's chat and toasts.
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.resolve_deep_link(DeepLinkAction::OpenUsername {
                    domain: full.to_string(),
                    start_param: None,
                    post: None,
                    story_id: None,
                });
            }
            self.connection.status_note =
                format!("{name} created! {manager} will manage this bot for you.");
            cx.notify();
            return true;
        }
        if create.creating.is_none() {
            // Failed: the dialog shows the reason and takes another try.
            if let Some(dialog) = self.dialogs.bot_create_dialog.as_mut() {
                dialog.submitted = false;
            }
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn build_bot_create_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::BotCreate, |this, _, cx| {
                this.close_bot_create(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.dialogs.bot_create_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let (manager_id, name, username) =
                (state.manager_id, state.name.clone(), state.username.clone());
            let submitted = state.submitted;
            let muted = cx.theme().muted_foreground;
            let danger = cx.theme().danger;
            let success = cx.theme().success;
            let manager = this
                .session()
                .and_then(|s| s.user(manager_id))
                .map(|u| u.first_name.clone())
                .unwrap_or_default();
            let create = this
                .session()
                .map(|s| s.bots.owned.create.clone())
                .unwrap_or_default();
            let typed = username.read(cx).value().to_string();
            let name_text = name.read(cx).value().trim().to_string();
            let full = owned_bots::full_username(&typed);
            let problem = owned_bots::username_problem(&typed);
            let verdict = create
                .check
                .as_ref()
                .filter(|(checked, _)| *checked == full)
                .map(|(_, result)| *result);
            let (line, line_color) = match (problem, verdict) {
                (Some(UsernameProblem::Empty), _) => (String::new(), muted),
                (Some(problem), _) => (problem.line().to_string(), danger),
                (None, Some(UsernameCheckResult::Available)) => {
                    (format!("{full} is available."), success)
                }
                (None, Some(_)) => ("This username is already taken.".to_string(), danger),
                (None, None) if create.check_pending.is_some() => ("Checking…".to_string(), muted),
                (None, None) => (format!("Link: t.me/{full}"), muted),
            };
            let taken = matches!(verdict, Some(result) if result != UsernameCheckResult::Available);
            let name_len = name_text.chars().count();
            let can_create =
                !submitted && name_len > 0 && name_len <= NAME_MAX && problem.is_none() && !taken;
            let suffix = owned_bots::missing_suffix(&owned_bots::clean_username(&typed));
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(format!(
                    "{manager} would like to create and manage a chatbot on your behalf."
                )))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().font_semibold().child("Bot name"))
                        .child(Input::new(&name).aria_label("Bot name")),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().font_semibold().child("Bot username"))
                        .child(Input::new(&username).aria_label("Bot username"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(if suffix.is_empty() {
                                    "The username ends with bot.".to_string()
                                } else {
                                    format!("\"{suffix}\" is added at the end.")
                                }),
                        )
                        .child(div().text_xs().text_color(line_color).child(line)),
                );
            if submitted {
                body = body.child(div().text_xs().text_color(muted).child("Creating…"));
            } else if let Some(error) = create.error.clone() {
                body = body.child(div().text_xs().text_color(danger).child(error));
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("bot-create-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_bot_create(cx);
                            this.close_kit_dialog_if_done(DialogKind::BotCreate, window, cx);
                        })),
                )
                .child(
                    Button::new("bot-create-submit")
                        .label("Create")
                        .primary()
                        .disabled(!can_create)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_bot_create(window, cx);
                        })),
                );
            let content = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .child(footer)
                .into_any_element();
            let content = Rc::new(RefCell::new(Some(content)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Create bot"))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |slot, _, _| {
                        let body = content
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        slot.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    BotCreate => DialogSpec::new(
        5430,
        |app| app.dialogs.bot_create_dialog.is_some(),
        QuillApp::build_bot_create_dialog,
    ),
}
