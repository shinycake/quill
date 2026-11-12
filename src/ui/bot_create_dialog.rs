//! "Create Bot" (Telegram Desktop `boxes/peers/create_managed_bot_box.cpp`),
//! reached from a `tg://newbot?manager=&username=&name=` link: a name, a
//! username that always ends with "bot", a live availability check, then
//! `createBot` managed by the bot in the link. Rules: `quill::owned_bots`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::owned_bots::{self, NAME_MAX, UsernameProblem};
use quill::state::{DeepLinkAction, ManagedBotShare};
use quill::telegram::envelope::{RequestManagedBotSpec, UsernameCheckResult};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// Wait after the last keystroke before asking the server.
const CHECK_DELAY: Duration = Duration::from_millis(350);

pub(super) struct BotCreateDialog {
    manager_id: i64,
    /// `createBot via_link`: true from a `tg://newbot` link, false from the
    /// keyboard button (tdesktop sets `f_via_deeplink` only for links).
    via_link: bool,
    share: Option<ManagedBotShare>,
    name: Entity<InputState>,
    username: Entity<InputState>,
    /// The pause before the availability check, if one is running. Each
    /// keystroke replaces it, so only the last one starts a check, and
    /// closing the box cancels it.
    check: UsernameCheck,
    _subscription: Subscription,
}

/// Where the debounce of the username check stands.
enum UsernameCheck {
    /// No check is due (nothing typed, or the typed name has a problem).
    Idle,
    /// The pause after the last keystroke; the check starts when it ends.
    Waiting { _timer: Task<()> },
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
            self.set_note(StatusNote::silent(format!(
                "@{} doesn't have Bot Management Mode enabled.",
                user.username
            )));
            cx.notify();
            return true;
        }
        self.open_bot_create(
            manager_id,
            suggested_name.clone(),
            suggested_username.clone(),
            true,
            None,
            window,
            cx,
        );
        true
    }

    /// A `keyboardButtonTypeRequestManagedBot` button was pressed (tdesktop
    /// `ButtonType::CreateBot`): a one-time keyboard hides, then the create
    /// dialog opens prefilled, managed by the keyboard's bot.
    pub(super) fn press_request_managed_bot(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        spec: &RequestManagedBotSpec,
        one_time: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(manager_id) = self.session().and_then(|s| s.bot_user_id_for_chat(chat_id)) else {
            self.set_status_note(StatusNote::info(tr(keys::Q_MISC_ONLY_A_BOT_CAN_ASK)), cx);
            return;
        };
        if one_time {
            self.dismiss_custom_keyboard(chat_id, message_id, cx);
        }
        let share = ManagedBotShare {
            chat_id,
            message_id,
            button_id: spec.id,
        };
        self.open_bot_create(
            manager_id,
            spec.suggested_name.clone(),
            spec.suggested_username.clone(),
            false,
            Some(share),
            window,
            cx,
        );
    }

    pub(in crate::ui) fn open_bot_create(
        &mut self,
        manager_id: i64,
        suggested_name: String,
        suggested_username: String,
        via_link: bool,
        share: Option<ManagedBotShare>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // An earlier createBot may still be in flight (its box was closed
        // while "Creating..."). Resetting would bump the generation and its
        // answer would no longer match, so it is never reported; a new box
        // would adopt that submission and its own share would never be
        // answered. So a second press waits: `drain_bot_create` reports the
        // first answer, and the press says so.
        let creating = self
            .session()
            .is_some_and(|session| session.bots.owned.create.creating());
        if creating {
            self.set_note(StatusNote::info(tr(keys::Q_MEDIA_STILL_CREATING_A_BOT)));
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.reset_create_bot();
        }
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::CREATE_BOT_NAME_PLACEHOLDER))
                .submit_on_enter(false)
        });
        let username = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::CREATE_BOT_USERNAME_PLACEHOLDER))
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
            via_link,
            share,
            name,
            username: username.clone(),
            check: UsernameCheck::Idle,
            _subscription: subscription,
        });
        self.open_box(DialogKind::BotCreate);
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
        dialog.check = UsernameCheck::Idle;
        cx.notify();
        if owned_bots::username_problem(&typed).is_some() {
            return;
        }
        let timer = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(CHECK_DELAY).await;
            let _ = this.update(cx, |this, cx| this.check_bot_username(cx));
        });
        if let Some(dialog) = self.dialogs.bot_create_dialog.as_mut() {
            dialog.check = UsernameCheck::Waiting { _timer: timer };
        }
    }

    fn check_bot_username(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.bot_create_dialog.as_mut() else {
            return;
        };
        dialog.check = UsernameCheck::Idle;
        let full = owned_bots::full_username(dialog.username.read(cx).value().as_ref());
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_new_bot_username(&full);
        }
        cx.notify();
    }

    fn close_bot_create(&mut self, cx: &mut Context<Self>) {
        // While "Creating..." the box may close: the bot is created on the
        // server all the same, and `drain_bot_create` reports the answer
        // (toast, chat, share) whether or not the box is still there.
        let creating = self
            .session()
            .is_some_and(|session| session.bots.owned.create.creating());
        if self.dialogs.bot_create_dialog.take().is_some()
            && !creating
            && let Some(live) = self.live.as_mut()
        {
            live.driver.reset_create_bot();
        }
        cx.notify();
    }

    fn submit_bot_create(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.screen.dialogs.bot_create_dialog.as_mut() else {
            return;
        };
        let name = dialog.name.read(cx).value().trim().to_string();
        let typed = dialog.username.read(cx).value().to_string();
        let manager_id = dialog.manager_id;
        let via_link = dialog.via_link;
        let share = dialog.share;
        if name.is_empty()
            || name.chars().count() > NAME_MAX
            || owned_bots::username_problem(&typed).is_some()
        {
            return;
        }
        let full = owned_bots::full_username(&typed);
        let Some(live) = self.live.as_mut() else {
            self.set_note(StatusNote::info(tr(keys::Q_MEDIA_DEMO_MODE_CREATING_A_BOT)));
            cx.notify();
            return;
        };
        if live
            .driver
            .create_managed_bot(manager_id, &name, &full, via_link, share)
            .is_err()
        {
            self.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_SEND_THE_REQUEST,
            )));
            cx.notify();
            return;
        }
        cx.notify();
    }

    /// Report the `createBot` answer the poll loop finds in the session:
    /// the bot exists, or the create failed with no box left to show why.
    /// The box may have been closed meanwhile: the answer is reported all
    /// the same, with the name and manager fixed at the press.
    pub(super) fn drain_bot_create(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let created = live.driver.take_created_bot();
        if let Some((submission, new_bot_id)) = created {
            let manager = self
                .session()
                .and_then(|s| s.user(submission.manager_id))
                .map(|u| u.first_name.clone())
                .unwrap_or_default();
            // The shell closes the kit dialog with its flag, in whichever
            // window hosts it. The answer is in: the dialog may go.
            self.close_bot_create(cx);
            // tdesktop reports the new bot to the requesting bot.
            if let (Some(share), Some(live)) = (submission.share, self.live.as_mut()) {
                let _ = live.driver.share_users_with_bot(
                    share.chat_id,
                    share.message_id,
                    share.button_id,
                    &[new_bot_id],
                );
            }
            // tdesktop opens the new bot's chat and toasts.
            let _ = self.resolve_deep_link_here(DeepLinkAction::OpenUsername {
                domain: submission.username.clone(),
                start_param: None,
                post: None,
                story_id: None,
            });
            self.set_note(StatusNote::silent(tr_with(
                keys::Q_MEDIA_CREATED_WILL_MANAGE_THIS_BOT,
                &[("name", &submission.name), ("manager", &manager)],
            )));
            cx.notify();
            return true;
        }
        // Failed: an open dialog shows the reason and takes another try; a
        // closed box has nowhere to show it, so it is a toast.
        if self.dialogs.bot_create_dialog.is_none()
            && let Some(live) = self.live.as_mut()
            && let Some(error) = live
                .driver
                .session
                .bots
                .owned
                .create
                .error()
                .map(str::to_string)
        {
            live.driver.reset_create_bot();
            self.set_note(StatusNote::error(error));
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
            let verdict = create.verdict(&full);
            let submitted = create.creating();
            let (line, line_color) = match (problem, verdict) {
                (Some(UsernameProblem::Empty), _) => (String::new(), muted),
                (Some(problem), _) => (problem.line().to_string(), danger),
                (None, Some(UsernameCheckResult::Available)) => (
                    tr_with(
                        keys::CREATE_BOT_USERNAME_AVAILABLE,
                        &[("username", &full.to_string())],
                    ),
                    success,
                ),
                (None, Some(_)) => (tr(keys::CREATE_BOT_USERNAME_TAKEN).to_string(), danger),
                (None, None) if create.checking() => {
                    (tr(keys::Q_MEDIA_CHECKING).to_string(), muted)
                }
                (None, None) => (
                    tr_with(keys::Q_MEDIA_LINK_T_ME, &[("full", &full.to_string())]),
                    muted,
                ),
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
                .child(div().text_sm().child(tr_with(
                    keys::CREATE_BOT_SUBTITLE,
                    &[("bot", &manager.to_string())],
                )))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(tr(keys::CREATE_BOT_NAME_PLACEHOLDER)),
                        )
                        .child(Input::new(&name).aria_label(tr(keys::CREATE_BOT_NAME_PLACEHOLDER))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(tr(keys::CREATE_BOT_USERNAME_PLACEHOLDER)),
                        )
                        .child(
                            Input::new(&username)
                                .aria_label(tr(keys::CREATE_BOT_USERNAME_PLACEHOLDER)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(if suffix.is_empty() {
                                    tr(keys::Q_MEDIA_THE_USERNAME_ENDS_WITH_BOT).to_string()
                                } else {
                                    tr_with(
                                        keys::Q_MEDIA_IS_ADDED_AT_THE_END,
                                        &[("suffix", suffix)],
                                    )
                                }),
                        )
                        .child(div().text_xs().text_color(line_color).child(line)),
                );
            if submitted {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MEDIA_CREATING)),
                );
            } else if let Some(error) = create.error().map(str::to_string) {
                body = body.child(div().text_xs().text_color(danger).child(error));
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("bot-create-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .disabled(submitted)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_bot_create(cx);
                            this.close_kit_dialog_if_done(DialogKind::BotCreate, window, cx);
                        })),
                )
                .child(
                    Button::new("bot-create-submit")
                        .label(tr(keys::CREATE_BOT_BUTTON))
                        .primary()
                        .disabled(!can_create)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.submit_bot_create(cx);
                        })),
                );
            let content = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .child(footer)
                .into_any_element();
            let content = Rc::new(Cell::new(Some(content)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::CREATE_BOT_TITLE)))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |slot, _, _| {
                        let body = content.take().unwrap_or_else(|| div().into_any_element());
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
    )
    .close(|app, _, cx| app.close_bot_create(cx)),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "bot_create_tests.rs"]
mod tests;
