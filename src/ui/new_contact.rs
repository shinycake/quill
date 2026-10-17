//! The New Contact box: Telegram Desktop's `AddContactBox` opened with no
//! user (Window > Add Contact on macOS). It imports one phone contact and
//! opens the chat when the phone belongs to a Telegram user; otherwise it
//! says the person has not joined and offers "Try someone else".
//!
//! It reuses [`AddContactDialog`] with `user_id == 0`; the known-user
//! "Add {name} to contacts" box stays in `contacts.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::contacts::AddContactDialog;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::NewContactOutcome;
use std::cell::RefCell;
use std::rc::Rc;

/// tdesktop `IsValidPhone` (`boxes/add_contact_box.cpp`): eight or more
/// digits, or one of the test numbers.
pub(crate) fn is_valid_phone(phone: &str) -> bool {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    digits.len() >= 8
        || digits == "333"
        || (digits.starts_with("42") && matches!(digits.len(), 2 | 5 | 6))
        || digits == "4242"
}

/// What the box sends: tdesktop moves a lone last name into the first name.
pub(crate) fn new_contact_names(first: &str, last: &str) -> Option<(String, String)> {
    let (first, last) = (first.trim(), last.trim());
    match (first.is_empty(), last.is_empty()) {
        (true, true) => None,
        (true, false) => Some((last.to_string(), String::new())),
        _ => Some((first.to_string(), last.to_string())),
    }
}

/// tdesktop `lng_contact_not_joined`.
pub(crate) fn not_joined_text(name: &str) -> String {
    tr_with(keys::CONTACT_NOT_JOINED, &[("name", name)])
}

impl QuillApp {
    /// Window > Add Contact: an empty New Contact box, first name focused.
    pub(super) fn open_new_contact_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = AddContactDialog::new(window, cx, 0, "", "", "");
        dialog
            .first_name_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.dialogs.add_contact_dialog = Some(dialog);
        cx.notify();
    }

    /// "Create": check the fields as tdesktop does, then `importContacts`.
    pub(super) fn submit_new_contact(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.add_contact_dialog.as_ref() else {
            return;
        };
        let first = dialog.first_name_input.read(cx).value().to_string();
        let last = dialog.last_name_input.read(cx).value().to_string();
        let phone = dialog.phone_input.read(cx).value().trim().to_string();
        let Some((first, last)) = new_contact_names(&first, &last) else {
            let input = dialog.first_name_input.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
            return;
        };
        if !is_valid_phone(&phone) {
            let input = dialog.phone_input.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
            return;
        }
        let sent = match self.live.as_mut() {
            Some(live) => live.driver.add_contact_by_phone(&phone, &first, &last),
            // Demo: no driver, so nothing can join.
            None => Ok(None),
        };
        match sent {
            Ok(_) => {
                if let Some(dialog) = self.dialogs.add_contact_dialog.as_mut() {
                    dialog.sent_name = Some(first);
                    if self.live.is_none() {
                        dialog.not_joined = true;
                    }
                }
            }
            Err(err) => self
                .connection
                .set_note(StatusNote::error(user_error(&err))),
        }
        cx.notify();
    }

    /// Apply the import's answer to the open box: close it when the chat
    /// opens, or switch to the "not joined" text.
    fn take_new_contact_outcome(&mut self, cx: &mut Context<Self>) {
        let outcome = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.users_state.new_contact_outcome.take());
        match outcome {
            None => {}
            Some(NewContactOutcome::Joined(_)) => {
                self.dialogs.add_contact_dialog = None;
                cx.notify();
            }
            Some(NewContactOutcome::NotJoined) => {
                if let Some(dialog) = self.dialogs.add_contact_dialog.as_mut() {
                    dialog.not_joined = true;
                }
                cx.notify();
            }
            Some(NewContactOutcome::Failed(code)) => {
                self.connection.set_note(StatusNote::error(tr_with(
                    keys::Q_OPS_ADD_CONTACT_FAILED_ERROR,
                    &[("code", &code.to_string())],
                )));
                cx.notify();
            }
        }
    }

    /// The New Contact box, hosted like the other kit dialogs.
    pub(super) fn build_new_contact_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::AddContact, |this, _, cx| {
                this.close_add_contact_dialog(cx);
            });
        app.update(cx, |this, cx| {
            this.take_new_contact_outcome(cx);
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::ADD_CONTACT_BUTTON)))
                .on_close(on_close);
            let Some(state) = this.dialogs.add_contact_dialog.as_ref() else {
                return dialog;
            };
            let muted = cx.theme().muted_foreground;
            let field = |label: &'static str, input| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().text_color(muted).child(label))
                    .child(Textarea::new(input).aria_label(label).h(px(40.)))
            };
            let (body, footer) = if state.not_joined {
                let name = state.sent_name.clone().unwrap_or_default();
                let body = div()
                    .id("new-contact-not-joined")
                    .role(Role::Label)
                    .aria_label(not_joined_text(&name))
                    .child(not_joined_text(&name))
                    .into_any_element();
                let footer = div().flex().justify_end().child(
                    Button::new("new-contact-retry")
                        .label(tr(keys::TRY_OTHER_CONTACT))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(dialog) = this.dialogs.add_contact_dialog.as_mut() {
                                dialog.not_joined = false;
                                dialog.sent_name = None;
                            }
                            cx.notify();
                        })),
                );
                (body, footer)
            } else {
                // tdesktop's order: first name, last name, phone.
                let body = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(field(tr(keys::Q_OPS_FIRST_NAME), &state.first_name_input))
                    .child(field(tr(keys::Q_OPS_LAST_NAME), &state.last_name_input))
                    .child(field(
                        tr(keys::NEW_CONTACT_PHONE_NUMBER),
                        &state.phone_input,
                    ))
                    .into_any_element();
                let footer = div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("new-contact-create")
                            .label(tr(keys::ADD_CONTACT))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_new_contact(window, cx);
                            })),
                    )
                    .child(
                        Button::new("new-contact-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_add_contact_dialog(cx);
                                this.close_kit_dialog_if_done(DialogKind::AddContact, window, cx);
                            })),
                    );
                (body, footer)
            };
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{is_valid_phone, new_contact_names, not_joined_text};

    #[test]
    fn phone_check_matches_telegram_desktop() {
        assert!(is_valid_phone("+1 (555) 010-0199"));
        assert!(is_valid_phone("333"));
        assert!(is_valid_phone("42"));
        assert!(is_valid_phone("42123"));
        assert!(is_valid_phone("4242"));
        assert!(!is_valid_phone("1234567"));
        assert!(!is_valid_phone(""));
        assert!(!is_valid_phone("4212"));
    }

    #[test]
    fn a_lone_last_name_becomes_the_first_name() {
        assert_eq!(new_contact_names(" ", ""), None);
        assert_eq!(
            new_contact_names("", "Lovelace"),
            Some(("Lovelace".into(), String::new()))
        );
        assert_eq!(
            new_contact_names(" Ada ", "Lovelace"),
            Some(("Ada".into(), "Lovelace".into()))
        );
        assert!(not_joined_text("Ada").starts_with("Unfortunately Ada has not joined Telegram"));
    }
}
