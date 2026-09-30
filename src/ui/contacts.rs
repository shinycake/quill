//! contacts list, import/add dialogs.

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{ContactRow, InfoPanelTarget, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::requests::{VCARD_IMPORT_LIMIT, parse_vcard};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyContacts` fixture: inject three users via `updateUser` (Ada online
/// and already a contact, Zed last-week and *not* a contact so the Add
/// affordance shows, Noor recently seen and a contact), a `getContacts`
/// `users` response through the same reducer the live path uses, and a
/// `userFullInfo` response with a bio for Zed. Opens the user info panel
/// for Zed; the demo block opens the contacts sidebar tab.
pub(super) fn apply_ready_contacts(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user_json = |id: i64,
                     first: &str,
                     last: &str,
                     phone: &str,
                     username: &str,
                     contact: bool,
                     status: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}","collectible_usernames":[]}},"phone_number":"{phone}","status":{status},"is_contact":{contact},"type":{{"@type":"userTypeRegular"}}}}}}"#,
            id = id,
            first = first,
            last = last,
            phone = phone,
            username = username,
            contact = contact,
            status = status,
        )
    };
    let jsons = [
        user_json(
            31,
            "Ada",
            "Lovelace",
            "+15550101031",
            "adalove",
            true,
            r#"{"@type":"userStatusOnline","expires":9999999999}"#,
        ),
        user_json(
            32,
            "Zed",
            "Hopper",
            "+15550101032",
            "zedhopper",
            false,
            r#"{"@type":"userStatusLastWeek","by_my_privacy_settings":false}"#,
        ),
        user_json(
            33,
            "Noor",
            "Haddad",
            "+15550101033",
            "noorhaddad",
            true,
            r#"{"@type":"userStatusRecently","by_my_privacy_settings":false}"#,
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let extra = session.request(RequestPurpose::GetContacts, None);
    let json = format!(
        r#"{{"@type":"users","@extra":"{}","total_count":3,"user_ids":[31,32,33]}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 32);
    let json = format!(
        r#"{{"@type":"userFullInfo","@extra":"{}","block_list":null,"bio":{{"@type":"formattedText","text":"Demo bio — systems programmer, occasional keyboard builder. This panel comes from the cached userFullInfo slice (Phase 6).","entities":[]}},"bot_info":null}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.open_info_panel = Some(InfoPanelTarget::User(32));
}

impl QuillApp {
    /// kit Phase 2 (redo): import contacts hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_import_contacts_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ImportContacts, |this, _, cx| {
                this.close_import_contacts_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).title("Import contacts");
            let Some(dialog_state) = this.import_contacts_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Paste the contents of a .vcf file. Only cards with a phone number are imported."),
                )
                .child(
                    div()
                        .flex_1()
                        .child(Textarea::new(&dialog_state.input).h(px(220.))),
                )
                .into_any_element();
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("import-contacts-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_import_contacts_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::ImportContacts, window, cx);
                    })),
            ).child(
                Button::new("import-contacts-submit")
                    .label("Import")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_import_contacts_dialog(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::ImportContacts, window, cx);
                    })),
            );
            dialog
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): add contact hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_add_contact_dialog(
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
            let Some(dialog_state) = this.add_contact_dialog.as_ref() else {
                return dialog.overlay(true).title("Add contact").on_close(on_close);
            };
            let name = this
                .session()
                .and_then(|s| s.user(dialog_state.user_id))
                .map(|u| u.display_name())
                .unwrap_or_else(|| format!("User {}", dialog_state.user_id));
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Phone number"),
                        )
                        .child(Textarea::new(&dialog_state.phone_input).h(px(40.))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("First name"),
                        )
                        .child(Textarea::new(&dialog_state.first_name_input).h(px(40.))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Last name"),
                        )
                        .child(Textarea::new(&dialog_state.last_name_input).h(px(40.))),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .gap_2()
                .child(
                    Button::new("add-contact-submit")
                        .label("Add contact")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_add_contact_dialog(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::AddContact, window, cx);
                        })),
                )
                .child(
                    Button::new("add-contact-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_add_contact_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::AddContact, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(format!("Add {name} to contacts"))
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    pub(super) fn retry_contacts(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_contacts()
        {
            self.status_note = format!("contacts request failed: {err:?}");
        }
        cx.notify();
    }

    /// Contacts list for the Contacts tab: loading / error / empty /
    /// rows. A tap opens the user info panel.
    pub(super) fn contacts_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<ContactRow> = self.session().map(|s| s.contact_rows()).unwrap_or_default();
        let failed = self.session().is_some_and(|s| s.contacts_error);
        let loading = self.session().is_some_and(|s| s.contacts.is_none()) && !failed;
        let mut list = div().id("contacts-list").flex().flex_col().gap_1();
        if failed {
            list = list
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Couldn’t load contacts."),
                )
                .child(
                    Button::new("contacts-retry")
                        .label("Retry")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.retry_contacts(cx);
                        })),
                );
        } else if loading {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading contacts…"),
            );
        } else if rows.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No contacts yet."),
            );
        } else {
            for row in rows {
                list = list.child(self.contact_row(&row, cx));
            }
        }
        list
    }

    /// Slice A6: set the local "Sync contacts" switch to the requested
    /// value, persist it to `contacts_prefs.json`, and refresh (or
    /// freeze) the tab.
    pub(super) fn set_contact_sync(&mut self, on: bool, cx: &mut Context<Self>) {
        let next = on;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.contact_prefs.sync_enabled = next;
            if let Err(err) = live.driver.save_contact_prefs() {
                self.status_note = format!("couldn't save contact prefs: {err}");
            }
            if next && live.driver.session.contacts.is_none() {
                // Re-enable refetches the server list so the tab
                // converges with TDLib immediately.
                if let Err(err) = live.driver.fetch_contacts() {
                    self.status_note = format!("contacts request failed: {err:?}");
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            // Demo mode — flip the in-memory pref so the toggle visibly
            // works in screenshots.
            session.contact_prefs.sync_enabled = next;
        }
        cx.notify();
    }

    /// Slice A6: show the vCard import dialog (paste the file's text;
    /// TGX reads `.vcf` files and only imports phone-number cards).
    pub(super) fn open_import_contacts_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Paste vCard (.vcf) text…")
                .auto_grow(4, 12)
                .submit_on_enter(false)
        });
        self.import_contacts_dialog = Some(ImportContactsDialog { input });
        cx.notify();
    }

    /// Slice A6: parse the pasted vCard text and send `importContacts`
    /// (schema 1.8.67, line 14517); cards without phone numbers are
    /// skipped and reported, and cards past the `VCARD_IMPORT_LIMIT`
    /// cap are truncated with the dropped count reported in the status
    /// note. Keeps the dialog open when nothing usable was pasted so the
    /// user can fix the text.
    pub(super) fn submit_import_contacts_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.import_contacts_dialog.as_ref() else {
            return;
        };
        let text = dialog.input.read(cx).text().to_string();
        let (contacts, skipped, truncated) = parse_vcard(&text);
        if contacts.is_empty() {
            self.status_note = "No phone-number contacts found in that vCard.".to_string();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.import_contacts(&contacts) {
                Ok(Some(_)) => {
                    self.status_note = if skipped == 0 && truncated == 0 {
                        "importing contacts…".to_string()
                    } else {
                        let mut details = Vec::new();
                        if skipped > 0 {
                            details.push(format!("{skipped} skipped without phone"));
                        }
                        if truncated > 0 {
                            details.push(format!(
                                "{truncated} over the {VCARD_IMPORT_LIMIT}-card limit"
                            ));
                        }
                        format!("importing contacts… ({})", details.join(", "))
                    };
                }
                Ok(None) => {
                    self.status_note = "request already in flight".to_string();
                    cx.notify();
                    return;
                }
                Err(err) => {
                    self.status_note = format!("import failed: {err:?}");
                    cx.notify();
                    return;
                }
            }
        }
        self.import_contacts_dialog = None;
        cx.notify();
        let _ = window;
    }

    /// Slice A6: dismiss the import dialog.
    pub(super) fn close_import_contacts_dialog(&mut self, cx: &mut Context<Self>) {
        self.import_contacts_dialog = None;
        cx.notify();
    }

    pub(super) fn contact_row(&self, row: &ContactRow, cx: &mut Context<Self>) -> impl IntoElement {
        let user_id = row.user_id;
        let name = row.name.clone();
        let status = row.status_text.clone();
        let selected =
            self.session().and_then(|s| s.open_info_panel) == Some(InfoPanelTarget::User(user_id));
        div()
            .id(("contact-row", user_id as u64))
            .px_2()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .pressable(cx.theme())
            .bg(if selected {
                cx.theme().accent.opacity(0.15)
            } else {
                cx.theme().sidebar
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_user_panel(user_id, window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(initials_avatar(&name, 32.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().font_medium().text_sm().child(name))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(status),
                            ),
                    ),
            )
    }

    pub(super) fn open_add_contact_dialog(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (phone, first, last) = self
            .session()
            .and_then(|s| s.user(user_id))
            .map(|u| {
                (
                    u.phone_number.clone(),
                    u.first_name.clone(),
                    u.last_name.clone(),
                )
            })
            .unwrap_or_default();
        self.add_contact_dialog = Some(AddContactDialog::new(
            window, cx, user_id, &phone, &first, &last,
        ));
        if let Some(dialog) = &self.add_contact_dialog {
            dialog
                .phone_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub(super) fn close_add_contact_dialog(&mut self, cx: &mut Context<Self>) {
        self.add_contact_dialog = None;
        cx.notify();
    }

    /// Submit the add-contact dialog. The phone field is required —
    /// `addContact` needs an `importedContact`, and Quill does not offer
    /// adding by bare user id.
    pub(super) fn submit_add_contact_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = self
            .add_contact_dialog
            .as_ref()
            .and_then(|dialog| dialog.draft(cx));
        let Some((user_id, phone, first, last)) = draft else {
            self.status_note = "Enter a phone number to add the contact.".into();
            cx.notify();
            return;
        };
        if let Some(live) = self.live.as_mut() {
            match live.driver.add_contact(user_id, &phone, &first, &last) {
                Ok(_) => {
                    self.add_contact_dialog = None;
                    self.status_note = "Contact add requested.".into();
                }
                Err(err) => {
                    self.status_note = format!("add contact failed: {err:?}");
                }
            }
        } else {
            // Demo: no driver — just close.
            self.add_contact_dialog = None;
            self.status_note = "Contact add requested.".into();
        }
        let _ = window;
        cx.notify();
    }
}
