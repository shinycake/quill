//! "Verify accounts" for the owner of a bot that can verify others (Telegram
//! Desktop `MakeVerifyPeersBox`, `boxes/peers/verify_peers_box.cpp`): a list
//! of chats; picking one confirms verifying it with the bot, with an optional
//! description when the bot may customise it, or removing the verification
//! the bot already gave. Rules and requests: `quill::bot_verification`.

use super::app::QuillApp;
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::bot_verification::{
    DESCRIPTION_LIMIT_DEFAULT, VerifyTarget, description_to_send, verified_by_bot,
};
use quill::state::ChatSummary;
use quill::telegram::envelope::ChatKind;
use quill::telegram::requests::MessageSenderRef;
use std::cell::RefCell;
use std::rc::Rc;

/// Chats the list shows at most (the list is the loaded chat list).
const LIST_LIMIT: usize = 80;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Pick,
    Add(i64),
    Remove(i64),
}

pub(super) struct BotVerifyDialog {
    bot_id: i64,
    step: Step,
    input: Entity<TextareaState>,
}

/// Who a chat row verifies: the user, or the supergroup or channel chat.
fn sender_for(chat: &ChatSummary) -> Option<(MessageSenderRef, VerifyTarget)> {
    match chat.kind {
        ChatKind::Private { user_id } => {
            Some((MessageSenderRef::User(user_id.0), VerifyTarget::User))
        }
        ChatKind::Supergroup { is_channel, .. } => Some((
            MessageSenderRef::Chat(chat.id.0),
            if is_channel {
                VerifyTarget::Channel
            } else {
                VerifyTarget::Group
            },
        )),
        _ => None,
    }
}

impl QuillApp {
    /// The profile row shown only for a bot the viewer can verify with.
    pub(super) fn bot_verify_accounts_row(
        &self,
        bot_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let params = self.session()?.bot_verification_parameters(bot_id)?;
        if !quill::bot_verification::offers_verify_accounts(Some(params)) {
            return None;
        }
        Some(
            action_row(
                "info-panel-verify-accounts",
                Some(IconName::BadgeCheck),
                "Verify accounts",
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_bot_verify(bot_id, window, cx);
            }))
            .into_any_element(),
        )
    }

    fn open_bot_verify(&mut self, bot_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = self
            .session()
            .and_then(|s| s.bot_verification_parameters(bot_id))
            .map(|p| p.default_description.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Description (optional)")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| input.set_value(placeholder, window, cx));
        self.dialogs.bot_verify_dialog = Some(BotVerifyDialog {
            bot_id,
            step: Step::Pick,
            input,
        });
        cx.notify();
    }

    fn close_bot_verify(&mut self, cx: &mut Context<Self>) {
        self.dialogs.bot_verify_dialog = None;
        cx.notify();
    }

    fn set_bot_verify_step(&mut self, step: Step, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialogs.bot_verify_dialog.as_mut() {
            dialog.step = step;
        }
        cx.notify();
    }

    /// Loaded users and channels to pick from, in chat-list order.
    fn bot_verify_candidates(&self, bot_id: i64) -> Vec<(i64, String, bool)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let params = session.bot_verification_parameters(bot_id);
        session
            .local_search_chats("")
            .into_iter()
            .filter(|chat| sender_for(chat).is_some())
            // The bot itself and the viewer are not offered.
            .filter(|chat| {
                !matches!(chat.kind, ChatKind::Private { user_id }
                    if user_id.0 == bot_id || Some(user_id.0) == session.my_user_id)
            })
            .take(LIST_LIMIT)
            .map(|chat| {
                let status_icon = match chat.kind {
                    ChatKind::Private { user_id } => session
                        .user(user_id.0)
                        .map(|u| u.verification.bot_icon)
                        .unwrap_or(0),
                    ChatKind::Supergroup { supergroup_id, .. } => session
                        .groups
                        .supergroup_verification
                        .get(&supergroup_id)
                        .map(|v| v.bot_icon)
                        .unwrap_or(0),
                    _ => 0,
                };
                (
                    chat.id.0,
                    chat.title.clone(),
                    verified_by_bot(bot_id, params, None, status_icon),
                )
            })
            .collect()
    }

    /// Send the verification (or its removal) for the picked chat.
    fn submit_bot_verify(&mut self, remove: bool, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.dialogs.bot_verify_dialog.as_ref() else {
            return true;
        };
        let (Step::Add(chat_id) | Step::Remove(chat_id)) = dialog.step else {
            return false;
        };
        let bot_id = dialog.bot_id;
        let typed = dialog.input.read(cx).value().to_string();
        let Some(session) = self.session() else {
            return true;
        };
        let Some((sender, target)) = session.chats.get(&chat_id).and_then(sender_for) else {
            return true;
        };
        let name = session
            .chats
            .get(&chat_id)
            .map(|c| c.title.clone())
            .unwrap_or_default();
        let description = if remove {
            String::new()
        } else {
            let Some(params) = session.bot_verification_parameters(bot_id) else {
                return true;
            };
            match description_to_send(params, &typed, DESCRIPTION_LIMIT_DEFAULT) {
                Ok(text) => text,
                // The dialog shows the counter in red and keeps the button off.
                Err(_) => return false,
            }
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: verifying needs a live session.".into();
            cx.notify();
            return true;
        };
        let sent = if remove {
            live.driver.remove_bot_verification(bot_id, sender)
        } else {
            live.driver
                .set_bot_verification(bot_id, sender, &description)
        };
        self.connection.status_note = match (sent, remove) {
            (Ok(_), false) => format!("Verifying {name} as a {}…", target.noun()),
            (Ok(_), true) => format!("Removing the verification from {name}…"),
            (Err(_), _) => "Could not send the verification request.".into(),
        };
        cx.notify();
        true
    }

    pub(super) fn build_bot_verify_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::BotVerify, |this, _, cx| {
                this.close_bot_verify(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.dialogs.bot_verify_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let (bot_id, step, input) = (state.bot_id, state.step, state.input.clone());
            let muted = cx.theme().muted_foreground;
            let (title, body, footer) = match step {
                Step::Pick => {
                    let mut list = div().flex().flex_col().gap_0p5().child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .pb_1()
                            .child("Choose a user, channel or group to verify or unverify."),
                    );
                    let candidates = this.bot_verify_candidates(bot_id);
                    if candidates.is_empty() {
                        list = list.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child("No chats loaded yet."),
                        );
                    }
                    for (id, name, verified) in candidates {
                        let label = if verified {
                            format!("{name} (verified)")
                        } else {
                            name
                        };
                        list = list.child(
                            action_row(
                                ("bot-verify-chat", id.unsigned_abs()),
                                verified.then_some(IconName::BadgeCheck),
                                label,
                                false,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.set_bot_verify_step(
                                        if verified {
                                            Step::Remove(id)
                                        } else {
                                            Step::Add(id)
                                        },
                                        cx,
                                    );
                                    window.refresh();
                                },
                            )),
                        );
                    }
                    let footer = div().flex().justify_end().child(
                        Button::new("bot-verify-done")
                            .label("Done")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_bot_verify(cx);
                                this.close_kit_dialog_if_done(DialogKind::BotVerify, window, cx);
                            })),
                    );
                    ("Verify accounts", list.into_any_element(), footer)
                }
                Step::Add(chat_id) | Step::Remove(chat_id) => {
                    let remove = matches!(step, Step::Remove(_));
                    let chat = this.session().and_then(|s| s.chats.get(&chat_id));
                    let name = chat.map(|c| c.title.clone()).unwrap_or_default();
                    let target = chat
                        .and_then(sender_for)
                        .map_or(VerifyTarget::User, |(_, target)| target);
                    let params = this
                        .session()
                        .and_then(|s| s.bot_verification_parameters(bot_id))
                        .cloned();
                    let org = params
                        .as_ref()
                        .map(|p| p.organization_name.clone())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| "your organization".into());
                    let custom = params
                        .as_ref()
                        .is_some_and(|p| p.can_set_custom_description);
                    let typed_len = input.read(cx).value().chars().count();
                    let too_long = custom && typed_len > DESCRIPTION_LIMIT_DEFAULT;
                    let text = if remove {
                        format!("Remove the verification of {name} given by {org}?")
                    } else {
                        format!(
                            "Verify {name} as a {} on behalf of {org}? A verification mark \
                             appears next to the name.",
                            target.noun()
                        )
                    };
                    let mut body = div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(div().text_sm().child(text));
                    if custom && !remove {
                        body = body
                            .child(Textarea::new(&input).aria_label("Verification description"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if too_long { cx.theme().danger } else { muted })
                                    .child(format!(
                                        "{typed_len}/{DESCRIPTION_LIMIT_DEFAULT}. Shown on the \
                                         profile of {name}."
                                    )),
                            );
                    }
                    let footer = div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("bot-verify-back")
                                .label("Back")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_bot_verify_step(Step::Pick, cx);
                                    window.refresh();
                                })),
                        )
                        .child({
                            let button = Button::new("bot-verify-submit")
                                .label(if remove { "Remove" } else { target.title() })
                                .disabled(too_long);
                            let button = if remove {
                                button.danger()
                            } else {
                                button.primary()
                            };
                            button.on_click(cx.listener(move |this, _, window, cx| {
                                if this.submit_bot_verify(remove, cx) {
                                    this.set_bot_verify_step(Step::Pick, cx);
                                    window.refresh();
                                }
                            }))
                        });
                    (
                        if remove {
                            "Remove verification"
                        } else {
                            target.title()
                        },
                        body.into_any_element(),
                        footer,
                    )
                }
            };
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
                .title(crate::ui::shell::dialog_title(title))
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
    BotVerify => DialogSpec::new(
        5410,
        |app| app.dialogs.bot_verify_dialog.is_some(),
        QuillApp::build_bot_verify_dialog,
    ),
}
