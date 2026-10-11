//! edit-profile dialog + actions.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::UsernameCheckResult;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyBotProfile` fixture (Slice B2): like `apply_ready_bot_chat`,
/// plus an armed `bot_start_params` entry (START button), a re-fetched
/// `userFullInfo` whose `botInfo` carries a menu button and a
/// privacy-policy URL, and a loaded `getBotSimilarBots` answer with two
/// similar bots.
/// Slice A5: seeds the current user (id 777) with a name, two active
/// usernames, one disabled username, a bio, and a profile-photo id, so
/// the "Edit profile" dialog renders populated sections (injected, no
/// live Telegram).
pub(super) fn apply_ready_profile_edit(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.my_user_id = Some(777);
    let info_extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 777);
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":777,"first_name":"Demo","last_name":"Viewer","usernames":{"@type":"usernames","active_usernames":["demoviewer","demoviewer_alt"],"disabled_usernames":["oldhandle"],"editable_username":"demoviewer","collectible_usernames":[]},"phone_number":"+15550131","profile_accent_color_id":3,"profile_background_custom_emoji_id":0,"type":{"@type":"userTypeRegular"}}}"#.to_string(),
        format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"Quill profile slice demo — bio, usernames and photo id are injected.","entities":[]}},"photo":{{"@type":"chatPhoto","id":555001,"sizes":[]}},"block_list":null,"birthdate":null,"bot_info":null}}"#,
            info_extra.0,
        ),
        // Slice A12: seed the accent palette so the dialog shows swatches.
        r#"{"@type":"updateProfileAccentColors","colors":[{"@type":"profileAccentColor","id":1,"light_theme_colors":{"@type":"profileAccentColors","palette_colors":[16749055],"background_colors":[],"story_colors":[]},"dark_theme_colors":{"@type":"profileAccentColors","palette_colors":[16749055],"background_colors":[],"story_colors":[]}},{"@type":"profileAccentColor","id":3,"light_theme_colors":{"@type":"profileAccentColors","palette_colors":[43776],"background_colors":[],"story_colors":[]},"dark_theme_colors":{"@type":"profileAccentColors","palette_colors":[43776],"background_colors":[],"story_colors":[]}},{"@type":"profileAccentColor","id":5,"light_theme_colors":{"@type":"profileAccentColors","palette_colors":[255],"background_colors":[],"story_colors":[]},"dark_theme_colors":{"@type":"profileAccentColors","palette_colors":[255],"background_colors":[],"story_colors":[]}}],"available_accent_color_ids":[1,3,5]}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// kit Phase 2 (redo): edit profile hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_edit_profile_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EditProfile, |this, _, cx| {
                this.close_edit_profile_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Edit profile"));
            let Some(body) = this.edit_profile_dialog_body(cx) else {
                return dialog.on_close(on_close);
            };
            let footer = div().flex().justify_end().child(
                Button::new("edit-profile-close")
                    .label("Done")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_edit_profile_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::EditProfile, window, cx);
                    })),
            );
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
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
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// A5: open the edit-profile dialog prefilled from the current user
    /// (the profile edit UI entry point, `parity:auth-edit-name`). The
    /// editable username (not just the primary) prefills the username
    /// field, since that is what `setUsername` changes.
    pub(super) fn open_edit_profile_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = self.session().and_then(|s| s.my_user_id);
        let Some(me) = me else {
            return;
        };
        let (first, last, bio, username, accent) = self
            .session()
            .and_then(|s| s.user(me))
            .map(|u| {
                let bio = self
                    .session()
                    .and_then(|s| s.user_full_info(me))
                    .map(|i| i.bio.clone())
                    .unwrap_or_default();
                (
                    u.first_name.clone(),
                    u.last_name.clone(),
                    bio,
                    u.editable_username.clone(),
                    // A12: -1 = no accent color (schema 1.8.67, line 2386).
                    u.profile_accent_color_id,
                )
            })
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            live.driver.session.users_state.profile_edit_error = None;
            live.driver.session.users_state.username_check = None;
            live.driver.session.users_state.username_check_pending = None;
        }
        self.dialogs.edit_profile_dialog = Some(EditProfileDialog::new(
            window, cx, &first, &last, &bio, &username, accent,
        ));
        if let Some(dialog) = &self.dialogs.edit_profile_dialog {
            dialog
                .first_name_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub(super) fn close_edit_profile_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.edit_profile_dialog = None;
        cx.notify();
    }

    /// A5: `setName` from the dialog. The first name is required
    /// (schema: 1-64 chars).
    pub(super) fn submit_profile_name(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return;
        };
        let first = EditProfileDialog::text(&dialog.first_name_input, cx)
            .trim()
            .to_string();
        let last = EditProfileDialog::text(&dialog.last_name_input, cx)
            .trim()
            .to_string();
        if first.is_empty() {
            self.connection.status_note = "First name can't be empty.".into();
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.set_name(&first, &last) {
            Ok(_) => self.connection.status_note = "Name update requested.".into(),
            Err(err) => self.connection.status_note = format!("set name failed: {err:?}"),
        }
        cx.notify();
    }

    /// A5: `setBio` from the dialog. Newlines are collapsed — the schema
    /// allows no line feeds.
    pub(super) fn submit_profile_bio(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return;
        };
        let bio = EditProfileDialog::text(&dialog.bio_input, cx).replace(['\n', '\r'], " ");
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.set_bio(bio.trim()) {
            Ok(_) => self.connection.status_note = "Bio update requested.".into(),
            Err(err) => self.connection.status_note = format!("set bio failed: {err:?}"),
        }
        cx.notify();
    }

    /// A5: `checkChatUsername` for the typed username (the private chat
    /// with self is the documented check target for the current user's
    /// own username).
    pub(super) fn check_profile_username(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return;
        };
        let username = dialog.username_text(cx);
        if username.is_empty() {
            self.connection.status_note =
                "Enter a username to check, or save it empty to remove it.".into();
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        if let Err(err) = live.driver.check_username(&username) {
            self.connection.status_note = format!("username check failed: {err:?}");
        }
        cx.notify();
    }

    /// A5: `setUsername` from the dialog. A changed non-empty username
    /// needs a fresh "Available" check first (TGX gates its Done button
    /// the same way); an empty value removes the username.
    pub(super) fn submit_profile_username(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return;
        };
        let username = dialog.username_text(cx);
        let editable = self
            .session()
            .and_then(|s| s.my_user_id)
            .and_then(|me| self.session().and_then(|s| s.user(me)))
            .map(|u| u.editable_username.clone())
            .unwrap_or_default();
        let checked_ok = self
            .session()
            .and_then(|s| s.users_state.username_check.clone())
            .is_some_and(|(text, result)| {
                text == username && result == UsernameCheckResult::Available
            });
        if !username.is_empty() && username != editable && !checked_ok {
            self.connection.status_note =
                "Check availability first — the text changed since the last check.".into();
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.set_username(&username) {
            Ok(_) => self.connection.status_note = "Username update requested.".into(),
            Err(err) => self.connection.status_note = format!("set username failed: {err:?}"),
        }
        cx.notify();
    }

    /// A5: move an active username one slot up/down via
    /// `reorderActiveUsernames` (the schema takes the full new order).
    pub(super) fn move_profile_username(
        &mut self,
        username: &str,
        up: bool,
        cx: &mut Context<Self>,
    ) {
        let order: Vec<String> = self
            .session()
            .and_then(|s| s.my_user_id)
            .and_then(|me| self.session().and_then(|s| s.user(me)))
            .map(|u| u.active_usernames.clone())
            .unwrap_or_default();
        let Some(pos) = order.iter().position(|u| u == username) else {
            return;
        };
        let swap = if up {
            pos.checked_sub(1)
        } else {
            pos.checked_add(1).filter(|&i| i < order.len())
        };
        let Some(swap) = swap else {
            return;
        };
        let mut new_order = order;
        new_order.swap(pos, swap);
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        if let Err(err) = live.driver.reorder_active_usernames(&new_order) {
            self.connection.status_note = format!("reorder failed: {err:?}");
        }
        cx.notify();
    }

    /// A5: `toggleUsernameIsActive` for one of the user's usernames.
    pub(super) fn toggle_profile_username(
        &mut self,
        username: &str,
        is_active: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.toggle_username_is_active(username, is_active) {
            Ok(_) => self.connection.status_note = "Username update requested.".into(),
            Err(err) => self.connection.status_note = format!("username toggle failed: {err:?}"),
        }
        cx.notify();
    }

    /// A5: `setProfilePhoto` from a local path (`inputChatPhotoStatic` /
    /// `inputFileLocal`). Sets the main profile photo (`is_public=false`,
    /// not the public photo).
    pub(super) fn submit_profile_photo(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return;
        };
        let path = EditProfileDialog::text(&dialog.photo_path_input, cx)
            .trim()
            .to_string();
        if path.is_empty() {
            self.connection.status_note = "Enter a photo path first.".into();
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.set_profile_photo(&path) {
            Ok(_) => self.connection.status_note = "Photo update requested.".into(),
            Err(err) => self.connection.status_note = format!("set photo failed: {err:?}"),
        }
        cx.notify();
    }

    /// A12: `setProfileAccentColor` from the dialog. The current
    /// `profile_background_custom_emoji_id` is preserved by the driver
    /// (Quill has no background-emoji picker).
    pub(super) fn submit_profile_accent(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return;
        };
        let selected = dialog.accent_selection;
        let current = self
            .session()
            .and_then(|s| s.my_user_id)
            .and_then(|me| self.session().and_then(|s| s.user(me)))
            .map(|u| u.profile_accent_color_id)
            .unwrap_or(-1);
        if selected == current {
            self.connection.status_note = "Accent color unchanged.".into();
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.set_profile_accent_color(selected) {
            Ok(_) => self.connection.status_note = "Accent color update requested.".into(),
            Err(err) => self.connection.status_note = format!("set accent color failed: {err:?}"),
        }
        cx.notify();
    }

    /// A5: `deleteProfilePhoto` for the current photo (`chatPhoto.id`
    /// from the cached user full info).
    pub(super) fn remove_profile_photo(&mut self, cx: &mut Context<Self>) {
        let photo_id = self
            .session()
            .and_then(|s| s.my_user_id)
            .and_then(|me| self.session().and_then(|s| s.user_full_info(me)))
            .and_then(|info| info.photo_id);
        let Some(photo_id) = photo_id else {
            self.connection.status_note = "No profile photo to remove.".into();
            cx.notify();
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: profile edits need a live session.".into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.delete_profile_photo(photo_id) {
            Ok(_) => self.connection.status_note = "Photo removal requested.".into(),
            Err(err) => self.connection.status_note = format!("remove photo failed: {err:?}"),
        }
        cx.notify();
    }

    /// A5: edit-profile dialog overlay — name (`setName`), bio
    /// (`setBio`), username (`setUsername` + `checkChatUsername` +
    /// `reorderActiveUsernames` / `toggleUsernameIsActive`), and photo
    /// (`setProfilePhoto` / `deleteProfilePhoto`) sections with
    /// per-section saves. Centered over the shell like the add-contact
    /// dialog.
    pub(super) fn edit_profile_dialog_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialogs.edit_profile_dialog.as_ref()?;
        let me = self.session().and_then(|s| s.my_user_id);
        let (active, disabled, editable) = me
            .and_then(|me| self.session().and_then(|s| s.user(me)))
            .map(|u| {
                (
                    u.active_usernames.clone(),
                    u.disabled_usernames.clone(),
                    u.editable_username.clone(),
                )
            })
            .unwrap_or_default();
        let has_photo = me
            .and_then(|me| self.session().and_then(|s| s.user_full_info(me)))
            .is_some_and(|info| info.photo_id.is_some());
        let username_text = dialog.username_text(cx);
        let verdict = self
            .session()
            .and_then(|s| s.users_state.username_check.clone())
            .filter(|(text, _)| *text == username_text)
            .map(|(_, result)| result);
        let checking = verdict.is_none()
            && self
                .session()
                .and_then(|s| s.users_state.username_check_pending.clone())
                .is_some_and(|text| text == username_text);
        let error = self
            .session()
            .and_then(|s| s.users_state.profile_edit_error.clone());
        let section_muted = cx.theme().muted_foreground;
        let section = move |title: &'static str| {
            div()
                .text_xs()
                .font_semibold()
                .text_color(section_muted)
                .child(title)
        };
        let verdict_line: AnyElement = if checking {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Checking…")
                .into_any_element()
        } else if let Some(result) = verdict {
            let (text, color) = match result {
                UsernameCheckResult::Available => ("Available", success()),
                UsernameCheckResult::Occupied => ("Occupied", danger()),
                UsernameCheckResult::Invalid => ("Invalid username", danger()),
                UsernameCheckResult::Purchasable => ("Taken — purchasable on Fragment", danger()),
                UsernameCheckResult::PublicChatsTooMany => ("Too many public usernames", danger()),
                UsernameCheckResult::PublicGroupsUnavailable => ("Unavailable", danger()),
            };
            div()
                .text_sm()
                .text_color(color)
                .child(text)
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Press Check to validate the username.")
                .into_any_element()
        };
        let mut usernames_body = div().flex().flex_col().gap_1();
        for (i, name) in active.iter().enumerate() {
            let name_up = name.clone();
            let name_down = name.clone();
            let name_toggle = name.clone();
            let mut row = div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_sm().child(format!("@{name}")));
            let mut actions = div().flex().gap_1();
            if i > 0 {
                actions = actions.child(
                    Button::new(format!("edit-profile-up-{i}"))
                        .label("↑")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.move_profile_username(&name_up, true, cx);
                        })),
                );
            }
            if i + 1 < active.len() {
                actions = actions.child(
                    Button::new(format!("edit-profile-down-{i}"))
                        .label("↓")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.move_profile_username(&name_down, false, cx);
                        })),
                );
            }
            // The editable username can't be disabled (schema 1.8.67,
            // line 14835).
            if *name != editable {
                actions = actions.child(
                    Button::new(format!("edit-profile-deactivate-{i}"))
                        .label("Deactivate")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_profile_username(&name_toggle, false, cx);
                        })),
                );
            }
            row = row.child(actions);
            usernames_body = usernames_body.child(row);
        }
        for (i, name) in disabled.iter().enumerate() {
            let name_toggle = name.clone();
            usernames_body = usernames_body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("@{name} (disabled)")),
                    )
                    .child(
                        Button::new(format!("edit-profile-activate-{i}"))
                            .label("Activate")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_profile_username(&name_toggle, true, cx);
                            })),
                    ),
            );
        }
        // kit Phase 2 (redo): plain form content — the kit `Dialog`
        // provides the title, padding, and chrome via `.title()`.
        let mut panel = div().id("edit-profile-panel").flex().flex_col().gap_3();
        if let Some(error) = error {
            panel = panel.child(div().text_sm().text_color(danger()).child(error));
        }
        panel = panel
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section("Photo"))
                    .child(
                        Textarea::new(&dialog.photo_path_input)
                            .aria_label("Profile photo file path")
                            .h(px(40.)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("edit-profile-set-photo")
                                    .label("Set photo")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.submit_profile_photo(cx);
                                    })),
                            )
                            .child(
                                // No photo → nothing to remove (TGX shows no
                                // remove affordance either).
                                div().when(has_photo, |this| {
                                    this.child(
                                        Button::new("edit-profile-remove-photo")
                                            .label("Remove photo")
                                            .ghost()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.remove_profile_photo(cx);
                                            })),
                                    )
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section("Name"))
                    .child(
                        Textarea::new(&dialog.first_name_input)
                            .aria_label("First name")
                            .h(px(40.)),
                    )
                    .child(
                        Textarea::new(&dialog.last_name_input)
                            .aria_label("Last name")
                            .h(px(40.)),
                    )
                    .child(
                        div().flex().gap_2().child(
                            Button::new("edit-profile-save-name")
                                .label("Save name")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.submit_profile_name(cx);
                                })),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section("Username"))
                    .child(
                        Textarea::new(&dialog.username_input)
                            .aria_label("Public username")
                            .h(px(40.)),
                    )
                    .child(verdict_line)
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("edit-profile-check-username")
                                    .label("Check")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.check_profile_username(cx);
                                    })),
                            )
                            .child(
                                Button::new("edit-profile-save-username")
                                    .label("Set username")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.submit_profile_username(cx);
                                    })),
                            ),
                    )
                    .child(usernames_body),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section("Bio"))
                    .child(
                        Textarea::new(&dialog.bio_input)
                            .aria_label("Bio")
                            .h(px(40.)),
                    )
                    .child(
                        div().flex().gap_2().child(
                            Button::new("edit-profile-save-bio")
                                .label("Save bio")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.submit_profile_bio(cx);
                                })),
                        ),
                    ),
            );
        // A12: accent color picker — swatches from the server palette
        // (`updateProfileAccentColors`), saved via `setProfileAccentColor`.
        // The server pushes the palette after authorization; until then
        // only "None" shows (honest empty state, not a fake palette).
        let (available_ids, palette) = self
            .session()
            .map(|s| {
                (
                    s.chats_state.available_accent_color_ids.clone(),
                    s.chats_state.profile_accent_colors.clone(),
                )
            })
            .unwrap_or_default();
        let mut swatch_row = div().flex().flex_wrap().gap_2();
        for id in &available_ids {
            let color = palette
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.swatch_rgb())
                .unwrap_or(0x229ED9);
            let selected = dialog.accent_selection == *id;
            let id = *id;
            swatch_row = swatch_row.child(self.appearance_swatch(
                format!("edit-profile-accent-{id}"),
                color,
                "",
                selected,
                cx,
                move |this, cx| {
                    if let Some(dialog) = this.dialogs.edit_profile_dialog.as_mut() {
                        dialog.accent_selection = id;
                    }
                    cx.notify();
                },
            ));
        }
        let none_selected = dialog.accent_selection == -1;
        swatch_row = swatch_row.child(self.appearance_chip(
            "edit-profile-accent-none",
            "None",
            none_selected,
            cx,
            |this, cx| {
                if let Some(dialog) = this.dialogs.edit_profile_dialog.as_mut() {
                    dialog.accent_selection = -1;
                }
                cx.notify();
            },
        ));
        panel = panel.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section("Accent color"))
                .child(swatch_row)
                .child(
                    div().flex().gap_2().child(
                        Button::new("edit-profile-save-accent")
                            .label("Save accent color")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_profile_accent(cx);
                            })),
                    ),
                ),
        );
        Some(panel.into_any_element())
    }
}

crate::ui::shell::register_dialogs! {
    EditProfile => DialogSpec::new(
        6000,
        |app| app.dialogs.edit_profile_dialog.is_some(),
        QuillApp::build_edit_profile_dialog,
    ),
}
