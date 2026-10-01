//! restrict/permissions/promote/demote/rights-editor dialogs.

use super::app::QuillApp;
use super::groups::{ADMIN_RIGHT_LABELS, admin_right_get, admin_right_set};
use super::groups::{
    CHAT_PERMISSION_LABELS, admin_rights_summary, chat_permission_get, chat_permission_set,
};
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::{AdminListFetch, AdminRightsFetch, MemberListFilter, SupergroupMembersFetch};
use quill::telegram::envelope::{ChatAdminRights, ChatPermissions, MessageSender};
use std::cell::RefCell;
use std::rc::Rc;
impl QuillApp {
    /// Phase D3b: open the promote member picker for a chat. Fetches the
    /// first page of supergroup members; the rights checkboxes start
    /// with every right enabled.
    pub(super) fn open_promote_picker(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.admin_dialog = Some(AdminDialog::promote(window, cx, chat_id));
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .fetch_supergroup_members(chat_id, MemberListFilter::Recent, "")
                .is_err()
        {
            self.status_note = "could not load members".into();
        }
        cx.notify();
    }

    /// Phase D3b: re-run the promote picker's member search with the
    /// current query text.
    pub(super) fn search_promote_members(&mut self, cx: &mut Context<Self>) {
        let (chat_id, query) = match self.admin_dialog.as_ref() {
            Some(dialog) => match &dialog.kind {
                AdminDialogKind::Promote { search_input, .. } => {
                    (dialog.chat_id, search_input.read(cx).value().to_string())
                }
                _ => return,
            },
            None => return,
        };
        if let Some(live) = self.live.as_mut() {
            let filter = if query.trim().is_empty() {
                MemberListFilter::Recent
            } else {
                MemberListFilter::Search
            };
            if live
                .driver
                .refresh_supergroup_members(chat_id, filter, query.trim())
                .is_err()
            {
                self.status_note = "could not search members".into();
            }
        } else {
            self.status_note = "member search needs a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3b: open the rights editor for one administrator. The
    /// dialog reads cached rights once the `getChatMember` lookup lands;
    /// checkbox toggles are staged locally until Save.
    pub(super) fn open_rights_editor(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        cx: &mut Context<Self>,
    ) {
        let cached = self
            .session()
            .and_then(|session| session.admin_rights.get(&(chat_id.0, user_id)))
            .and_then(|fetch| match fetch {
                AdminRightsFetch::Loaded(rights) => Some(*rights),
                _ => None,
            });
        self.admin_dialog = Some(AdminDialog::edit_rights(chat_id, user_id, cached));
        if let Some(live) = self.live.as_mut()
            && live.driver.fetch_admin_rights(chat_id, user_id).is_err()
        {
            self.status_note = "could not load admin rights".into();
        }
        cx.notify();
    }

    /// Phase D3b: open the demote confirmation for one administrator.
    pub(super) fn open_demote_confirm(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.admin_dialog = Some(AdminDialog::demote_confirm(chat_id, user_id));
        cx.notify();
    }

    /// Phase D3b: close the admin-management dialog.
    pub(super) fn close_admin_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin_dialog = None;
        cx.notify();
    }

    /// Phase D3b: submit the admin dialog — promote / edit-rights /
    /// demote. In-flight duplicates are deduped by the driver; the
    /// dialog closes on submit and the lists refresh from the server
    /// responses (`updateChatMember` + `ok`).
    pub(super) fn submit_admin_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin_dialog.take() else {
            return;
        };
        // (keep_dialog, note): validation errors keep the dialog open so
        // the user's selection and checkbox state are not lost.
        let (keep_dialog, note) = match &dialog.kind {
            AdminDialogKind::Promote {
                rights,
                selected_user: Some(user_id),
                ..
            } => match self.live.as_mut() {
                Some(live) => {
                    match live
                        .driver
                        .promote_chat_member(dialog.chat_id, *user_id, rights)
                    {
                        Ok(_) => (false, "Member promoted".to_string()),
                        Err(_) => (true, "could not promote member".to_string()),
                    }
                }
                None => (
                    true,
                    "admin actions need a live connection (demo)".to_string(),
                ),
            },
            AdminDialogKind::Promote {
                selected_user: None,
                ..
            } => (true, "select a member first".to_string()),
            AdminDialogKind::EditRights { user_id, rights } => {
                let Some(rights) = rights else {
                    self.admin_dialog = Some(dialog);
                    self.status_note = "rights are still loading".into();
                    cx.notify();
                    return;
                };
                match self.live.as_mut() {
                    Some(live) => {
                        match live
                            .driver
                            .edit_admin_rights(dialog.chat_id, *user_id, rights)
                        {
                            Ok(_) => (false, "Admin rights updated".to_string()),
                            Err(_) => (true, "could not update admin rights".to_string()),
                        }
                    }
                    None => (
                        true,
                        "admin actions need a live connection (demo)".to_string(),
                    ),
                }
            }
            AdminDialogKind::DemoteConfirm { user_id } => match self.live.as_mut() {
                Some(live) => match live.driver.demote_chat_member(dialog.chat_id, *user_id) {
                    Ok(_) => (false, "Admin demoted".to_string()),
                    Err(_) => (true, "could not demote admin".to_string()),
                },
                None => (
                    true,
                    "admin actions need a live connection (demo)".to_string(),
                ),
            },
        };
        if keep_dialog {
            self.admin_dialog = Some(dialog);
        }
        self.status_note = note;
        cx.notify();
    }

    /// Slice G1: group/supergroup/channel management handlers. Every
    /// action is capability-gated in the driver (`chat_can_restrict_`
    /// `members`, `chat_is_owner`, `chat_can_add_members`,
    /// `can_be_deleted_for_all_users`); the UI mirrors the gates so
    /// buttons only appear when the action can succeed.

    /// Slice G1: open the restrict/ban dialog. Restrict starts from the
    /// chat's current default permissions block (that's what a new
    /// restricted status loosens/tightens); ban needs no permissions.
    pub(super) fn open_restrict_dialog(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        ban: bool,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .and_then(|chat| chat.permissions.clone())
            .unwrap_or_else(ChatPermissions::all);
        self.restrict_dialog = Some(RestrictDialog::new(chat_id, user_id, ban, current));
        cx.notify();
    }

    pub(super) fn close_restrict_dialog(&mut self, cx: &mut Context<Self>) {
        self.restrict_dialog = None;
        cx.notify();
    }

    pub(super) fn toggle_restrict_permission(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(dialog) = self.restrict_dialog.as_mut() {
            let enabled = chat_permission_get(&dialog.permissions, index);
            chat_permission_set(&mut dialog.permissions, index, !enabled);
            cx.notify();
        }
    }

    pub(super) fn cycle_restrict_duration(&mut self, cx: &mut Context<Self>) {
        const DURATIONS: [i32; 4] = [0, 1, 7, 30];
        if let Some(dialog) = self.restrict_dialog.as_mut() {
            let position = DURATIONS
                .iter()
                .position(|days| *days == dialog.banned_until_days)
                .unwrap_or(0);
            dialog.banned_until_days = DURATIONS[(position + 1) % DURATIONS.len()];
            cx.notify();
        }
    }

    /// Slice G1: submit restrict/ban (`setChatMemberStatus`, schema
    /// 1.8.67 line 13592). Duration is now + days; 0 = forever.
    pub(super) fn submit_restrict_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.restrict_dialog.take() else {
            return;
        };
        let note = match self.live.as_mut() {
            Some(live) => {
                let until_date = Self::restrict_until_date(dialog.banned_until_days);
                let result = if dialog.ban {
                    live.driver
                        .ban_chat_member(dialog.chat_id, dialog.user_id, until_date)
                } else {
                    live.driver.restrict_chat_member(
                        dialog.chat_id,
                        dialog.user_id,
                        until_date,
                        &dialog.permissions,
                    )
                };
                match result {
                    // Slice G1 fix-up: `Ok(None)` means the driver
                    // refused to send (no rights, or restrict in a
                    // channel) — not success. Keep the dialog open and
                    // say so instead of claiming it happened.
                    Ok(Some(_)) => {
                        if dialog.ban {
                            "member banned".into()
                        } else {
                            "member restricted".into()
                        }
                    }
                    Ok(None) | Err(_) => {
                        self.restrict_dialog = Some(dialog);
                        "could not update member status".into()
                    }
                }
            }
            None => {
                self.restrict_dialog = Some(dialog);
                "member actions need a live connection (demo)".into()
            }
        };
        self.status_note = note;
        cx.notify();
    }

    /// Slice G1: lift a restriction or ban (`setChatMemberStatus` →
    /// `chatMemberStatusMember`).
    pub(super) fn unban_member(&mut self, chat_id: ChatId, user_id: i64, cx: &mut Context<Self>) {
        let note = match self.live.as_mut() {
            // Slice G1 fix-up: `Ok(None)` is a driver refusal, not
            // success — report it honestly.
            Some(live) => match live.driver.unban_chat_member(chat_id, user_id) {
                Ok(Some(_)) => "member unbanned".into(),
                Ok(None) => "could not unban member".into(),
                Err(_) => "could not unban member".into(),
            },
            None => "member actions need a live connection (demo)".into(),
        };
        self.status_note = note;
        self.refresh_member_dialog(cx);
    }

    /// Slice G1: open the default-permissions editor, seeded from the
    /// chat's current block.
    pub(super) fn open_permissions_dialog(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let current = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .and_then(|chat| chat.permissions.clone())
            .unwrap_or_else(ChatPermissions::all);
        self.permissions_dialog = Some(PermissionsDialog {
            chat_id,
            permissions: current,
        });
        cx.notify();
    }

    pub(super) fn close_permissions_dialog(&mut self, cx: &mut Context<Self>) {
        self.permissions_dialog = None;
        cx.notify();
    }

    pub(super) fn toggle_permission(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(dialog) = self.permissions_dialog.as_mut() {
            let enabled = chat_permission_get(&dialog.permissions, index);
            chat_permission_set(&mut dialog.permissions, index, !enabled);
            cx.notify();
        }
    }

    /// Slice G1: `setChatPermissions` (schema 1.8.67, line 13464).
    pub(super) fn submit_permissions_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.permissions_dialog.take() else {
            return;
        };
        let note = match self.live.as_mut() {
            Some(live) => match live
                .driver
                .set_chat_permissions(dialog.chat_id, &dialog.permissions)
            {
                Ok(_) => "permissions updated".into(),
                Err(_) => {
                    self.permissions_dialog = Some(dialog);
                    "could not update permissions".into()
                }
            },
            None => {
                self.permissions_dialog = Some(dialog);
                "permissions need a live connection (demo)".into()
            }
        };
        self.status_note = note;
        cx.notify();
    }

    /// kit Phase 2 (redo): permissions hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_permissions_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Permissions, |this, _, cx| {
                this.close_permissions_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).title("Default permissions");
            let Some(dialog_state) = this.permissions_dialog.as_ref() else {
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
                        .child("What new members may do by default"),
                )
                .child(this.permission_checkboxes(&dialog_state.permissions, cx))
                .into_any_element();
            let footer =
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("g1-permissions-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_permissions_dialog(cx);
                                this.close_kit_dialog_if_done(DialogKind::Permissions, window, cx);
                            })),
                    )
                    .child(Button::new("g1-permissions-submit").label("Save").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.submit_permissions_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Permissions, window, cx);
                        }),
                    ));
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

    /// kit Phase 2 (redo): restrict/ban hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_restrict_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Restrict, |this, _, cx| {
                this.close_restrict_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.restrict_dialog.as_ref() else {
                return dialog.title("Restrict").on_close(on_close);
            };
            let name = this
                .session()
                .and_then(|session| session.user(dialog_state.user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {}", dialog_state.user_id));
            let duration_label = match dialog_state.banned_until_days {
                0 => "Forever".to_string(),
                1 => "1 day".to_string(),
                days => format!("{days} days"),
            };
            let title = format!(
                "{} {name}",
                if dialog_state.ban { "Ban" } else { "Restrict" }
            );
            let mut body = div().flex().flex_col().gap_2();
            if !dialog_state.ban {
                body = body
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Allowed while restricted"),
                    )
                    .child(this.restrict_permission_checkboxes(&dialog_state.permissions, cx));
            }
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Duration:"),
                    )
                    .child(
                        Button::new("g1-restrict-duration")
                            .label(duration_label)
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cycle_restrict_duration(cx);
                                this.close_kit_dialog_if_done(DialogKind::Restrict, window, cx);
                            })),
                    ),
            );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-restrict-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_restrict_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Restrict, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-restrict-submit")
                        .label(if dialog_state.ban { "Ban" } else { "Restrict" })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_restrict_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Restrict, window, cx);
                        })),
                );
            let body = body.into_any_element();
            dialog
                .title(title)
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

    /// Slice G1: 16 permission checkboxes bound to the dialog's staged
    /// copy (mirrors `rights_checkboxes`).
    pub(super) fn permission_checkboxes(
        &self,
        permissions: &ChatPermissions,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap_1();
        let mut row = div().flex().gap_2();
        for (index, label) in CHAT_PERMISSION_LABELS.iter().enumerate() {
            let enabled = chat_permission_get(permissions, index);
            row = row.child(
                div().flex_1().child(
                    // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                    Checkbox::new(format!("g1-permission-{index}"))
                        .checked(enabled)
                        .label(*label)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if on != enabled {
                                this.toggle_permission(index, cx);
                            }
                        })),
                ),
            );
            if index % 2 == 1 {
                list = list.child(row);
                row = div().flex().gap_2();
            }
        }
        list.into_any_element()
    }

    /// Slice G1: permission checkboxes for the restrict dialog (same
    /// labels, separate toggle handler).
    pub(super) fn restrict_permission_checkboxes(
        &self,
        permissions: &ChatPermissions,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap_1();
        let mut row = div().flex().gap_2();
        for (index, label) in CHAT_PERMISSION_LABELS.iter().enumerate() {
            let enabled = chat_permission_get(permissions, index);
            row = row.child(
                div().flex_1().child(
                    // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                    Checkbox::new(format!("g1-restrict-permission-{index}"))
                        .checked(enabled)
                        .label(*label)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if on != enabled {
                                this.toggle_restrict_permission(index, cx);
                            }
                        })),
                ),
            );
            if index % 2 == 1 {
                list = list.child(row);
                row = div().flex().gap_2();
            }
        }
        list.into_any_element()
    }

    /// Phase D3b: the admin-management dialog, rendered above the
    /// composer like the poll / invite-link dialogs. Three flows share
    /// one slot: the promote member picker (search + member list +
    /// rights checkboxes), the rights editor for an existing admin, and
    /// the demote confirmation. All element IDs are namespaced
    /// `admin-*` so the screenshot/replay harnesses can find them.
    pub(super) fn admin_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.admin_dialog.as_ref()?;
        match &dialog.kind {
            AdminDialogKind::Promote {
                search_input,
                rights,
                selected_user,
            } => Some(self.promote_dialog_panel(
                dialog.chat_id,
                search_input,
                rights,
                *selected_user,
                cx,
            )),
            AdminDialogKind::EditRights { user_id, rights } => {
                Some(self.rights_editor_panel(dialog.chat_id, *user_id, *rights, cx))
            }
            AdminDialogKind::DemoteConfirm { user_id } => {
                Some(self.demote_confirm_panel(*user_id, cx))
            }
        }
    }

    /// Phase D3b: 18 rights checkboxes for a dialog's staged rights, bound
    /// to a toggle handler. `id_prefix` namespaces the button IDs
    /// (`{id_prefix}-{index}`).
    pub(super) fn rights_checkboxes(
        &self,
        rights: &ChatAdminRights,
        id_prefix: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap_1();
        let mut row = div().flex().gap_2();
        for (index, label) in ADMIN_RIGHT_LABELS.iter().enumerate() {
            let enabled = admin_right_get(rights, index);
            let prefix = id_prefix.to_string();
            row = row.child(
                div().flex_1().child(
                    // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                    Checkbox::new(format!("{prefix}-{index}"))
                        .checked(enabled)
                        .label(*label)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if on != enabled {
                                this.toggle_admin_right(prefix.as_str(), index, cx);
                            }
                        })),
                ),
            );
            if index % 2 == 1 {
                list = list.child(row);
                row = div().flex().gap_2();
            }
        }
        list.into_any_element()
    }

    /// Phase D3b: flip one rights checkbox on the open admin dialog. The
    /// promote dialog owns its staged rights; the rights editor seeds its
    /// staged copy from the cached `getChatMember` rights on the first
    /// toggle so edits never mutate the session cache.
    pub(super) fn toggle_admin_right(
        &mut self,
        prefix: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let seed = match self.admin_dialog.as_ref() {
            Some(dialog) => match &dialog.kind {
                AdminDialogKind::EditRights {
                    user_id,
                    rights: None,
                } if prefix == "admin-edit-right" => {
                    let cached = self
                        .session()
                        .and_then(|session| session.admin_rights.get(&(dialog.chat_id.0, *user_id)))
                        .and_then(|fetch| match fetch {
                            AdminRightsFetch::Loaded(rights) => Some(*rights),
                            _ => None,
                        });
                    Some((*user_id, cached))
                }
                _ => None,
            },
            None => None,
        };
        if let Some(dialog) = self.admin_dialog.as_mut() {
            match &mut dialog.kind {
                AdminDialogKind::Promote { rights, .. } if prefix == "admin-right" => {
                    let current = admin_right_get(rights, index);
                    admin_right_set(rights, index, !current);
                }
                AdminDialogKind::EditRights {
                    user_id,
                    rights: staged,
                } if prefix == "admin-edit-right" => {
                    if staged.is_none()
                        && let Some((seed_user, seed_rights)) = seed
                        && seed_user == *user_id
                    {
                        *staged = seed_rights;
                    }
                    if let Some(staged) = staged {
                        let current = admin_right_get(staged, index);
                        admin_right_set(staged, index, !current);
                    }
                }
                _ => {}
            }
        }
        cx.notify();
    }

    /// Phase D3b: the promote member picker — search field, member rows
    /// (click to select; already-admin members are excluded), rights
    /// checkboxes, Promote / Cancel.
    pub(super) fn promote_dialog_panel(
        &self,
        chat_id: ChatId,
        search_input: &Entity<TextareaState>,
        rights: &ChatAdminRights,
        selected_user: Option<i64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let admin_ids: std::collections::HashSet<i64> = self
            .session()
            .and_then(|session| session.admin_lists.get(&chat_id.0))
            .and_then(|fetch| match fetch {
                AdminListFetch::Loaded(list) => Some(list),
                _ => None,
            })
            .map(|list| list.iter().map(|entry| entry.user_id).collect())
            .unwrap_or_default();
        let members = self.session().and_then(|session| {
            // Slice G1: the promote picker browses Recent / Search
            // pages; show whichever matches the current query.
            let query = search_input.read(cx).value();
            let filter = if query.trim().is_empty() {
                MemberListFilter::Recent
            } else {
                MemberListFilter::Search
            };
            session
                .supergroup_members
                .get(&(chat_id.0, filter))
                .cloned()
        });
        let mut panel = div()
            .id("admin-promote-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_bright())
                    .child("Promote member"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(Textarea::new(search_input).h(px(40.))))
                    .child(
                        Button::new("admin-promote-search")
                            .label("Search")
                            .ghost()
                            .text_color(text_bright())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.search_promote_members(cx);
                            })),
                    ),
            );
        match members {
            None | Some(SupergroupMembersFetch::Loading) => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading members…"),
                );
            }
            Some(SupergroupMembersFetch::Failed(message)) => {
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(message),
                        )
                        .child(
                            Button::new("admin-promote-retry")
                                .label("Retry")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.search_promote_members(cx);
                                })),
                        ),
                );
            }
            Some(SupergroupMembersFetch::Loaded { members, .. }) => {
                // Phase 6: kit RadioGroup (was: buttons with a ●/○ prefix).
                // Controlled: the chosen index writes the value.
                let candidates: Vec<(i64, String)> = members
                    .iter()
                    .filter_map(|member| match member.member_id {
                        MessageSender::User { user_id } => Some(user_id),
                        _ => None,
                    })
                    .filter(|user_id| !admin_ids.contains(user_id))
                    .take(30)
                    .map(|user_id| {
                        let name = self
                            .session()
                            .and_then(|session| session.user(user_id))
                            .map(|user| user.display_name())
                            .unwrap_or_else(|| format!("User {user_id}"));
                        (user_id, name)
                    })
                    .collect();
                if candidates.is_empty() {
                    panel = panel.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No members found."),
                    );
                } else {
                    let selected_ix = candidates
                        .iter()
                        .position(|(user_id, _)| selected_user == Some(*user_id));
                    panel = panel.child(
                        RadioGroup::vertical("admin-promote-member")
                            .selected_index(selected_ix)
                            .children(candidates.iter().map(|(user_id, name)| {
                                Radio::new(format!("admin-promote-member-{user_id}"))
                                    .label(name.clone())
                            }))
                            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                                let user_id = candidates[ix].0;
                                if let Some(dialog) = this.admin_dialog.as_mut()
                                    && let AdminDialogKind::Promote { selected_user, .. } =
                                        &mut dialog.kind
                                {
                                    *selected_user = Some(user_id);
                                }
                                cx.notify();
                            })),
                    );
                }
            }
        }
        let summary = admin_rights_summary(rights);
        panel = panel
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Rights ({summary})")),
            )
            .child(self.rights_checkboxes(rights, "admin-right", cx))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("admin-promote-submit")
                            .label("Promote")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_admin_dialog(cx);
                            })),
                    )
                    .child(
                        Button::new("admin-promote-cancel")
                            .label("Cancel")
                            .ghost()
                            .text_color(text_bright())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_admin_dialog(cx);
                            })),
                    ),
            );
        panel.into_any_element()
    }

    /// Phase D3b: the rights editor for one administrator. While the
    /// `getChatMember` lookup is in flight the dialog shows a loading
    /// row; once rights land (cached or staged in the dialog) the 18
    /// checkboxes bind to the dialog's staged copy and Save sends
    /// `setChatMemberStatus`.
    pub(super) fn rights_editor_panel(
        &self,
        chat_id: ChatId,
        user_id: i64,
        staged: Option<ChatAdminRights>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .session()
            .and_then(|session| session.user(user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        let cached = self
            .session()
            .and_then(|session| session.admin_rights.get(&(chat_id.0, user_id)))
            .and_then(|fetch| match fetch {
                AdminRightsFetch::Loaded(rights) => Some(*rights),
                _ => None,
            });
        let rights = staged.or(cached);
        let mut panel = div()
            .id("admin-rights-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_bright())
                    .child(format!("Edit rights — {name}")),
            );
        match rights {
            None => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading rights…"),
                );
            }
            Some(rights) => {
                panel = panel
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(admin_rights_summary(&rights)),
                    )
                    .child(self.rights_checkboxes(&rights, "admin-edit-right", cx));
            }
        }
        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("admin-rights-save")
                        .label("Save")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.submit_admin_dialog(cx);
                        })),
                )
                .child(
                    Button::new("admin-rights-cancel")
                        .label("Cancel")
                        .ghost()
                        .text_color(text_bright())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_admin_dialog(cx);
                        })),
                ),
        );
        panel.into_any_element()
    }

    /// Phase D3b: demote confirmation for one administrator.
    pub(super) fn demote_confirm_panel(&self, user_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let name = self
            .session()
            .and_then(|session| session.user(user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        div()
            .id("admin-demote-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_bright())
                    .child(format!("Demote {name}?")),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("They will become a regular member and lose all admin rights."),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Button::new("admin-demote-submit").label("Demote").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.submit_admin_dialog(cx);
                        }),
                    ))
                    .child(
                        Button::new("admin-demote-cancel")
                            .label("Cancel")
                            .ghost()
                            .text_color(text_bright())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_admin_dialog(cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
