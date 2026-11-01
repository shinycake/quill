//! restrict/permissions/promote/demote/rights-editor dialogs.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::groups::{ADMIN_RIGHT_LABELS, admin_right_get, admin_right_set};
use super::groups::{
    CHAT_PERMISSION_LABELS, admin_rights_summary, chat_permission_get, chat_permission_set,
};
use super::scheduled::picked_unix;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::wall_clock::WallClock;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::moderation::{MIN_RESTRICT_SECS, RestrictUntil, UntilError, validate_restrict_until};
use quill::moderation_exceptions::initial_until;
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
        self.admin.admin_dialog = Some(AdminDialog::promote(window, cx, chat_id));
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .fetch_supergroup_members(chat_id, MemberListFilter::Recent, "")
                .is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_LOAD_MEMBERS)));
        }
        cx.notify();
    }

    /// Phase D3b: re-run the promote picker's member search with the
    /// current query text.
    pub(super) fn search_promote_members(&mut self, cx: &mut Context<Self>) {
        let (chat_id, query) = match self.admin.admin_dialog.as_ref() {
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
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULD_NOT_SEARCH_MEMBERS,
                )));
            }
        } else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_MEMBER_SEARCH_NEEDS_A_LIVE,
            )));
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
            .and_then(|session| session.groups.admin_rights.get(&(chat_id.0, user_id)))
            .and_then(|fetch| match fetch {
                AdminRightsFetch::Loaded(rights) => Some(*rights),
                _ => None,
            });
        self.admin.admin_dialog = Some(AdminDialog::edit_rights(chat_id, user_id, cached));
        if let Some(live) = self.live.as_mut()
            && live.driver.fetch_admin_rights(chat_id, user_id).is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULD_NOT_LOAD_ADMIN_RIGHTS,
            )));
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
        self.admin.admin_dialog = Some(AdminDialog::demote_confirm(chat_id, user_id));
        cx.notify();
    }

    /// Phase D3b: close the admin-management dialog.
    pub(super) fn close_admin_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.admin_dialog = None;
        cx.notify();
    }

    /// Phase D3b: submit the admin dialog — promote / edit-rights /
    /// demote. In-flight duplicates are deduped by the driver; the
    /// dialog closes on submit and the lists refresh from the server
    /// responses (`updateChatMember` + `ok`).
    pub(super) fn submit_admin_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin.admin_dialog.take() else {
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
                        Ok(_) => (false, tr(keys::Q_PEOPLE_MEMBER_PROMOTED).to_string()),
                        Err(_) => (
                            true,
                            tr(keys::Q_PEOPLE_COULD_NOT_PROMOTE_MEMBER).to_string(),
                        ),
                    }
                }
                None => (
                    true,
                    tr(keys::Q_PEOPLE_ADMIN_ACTIONS_NEED_A_LIVE).to_string(),
                ),
            },
            AdminDialogKind::Promote {
                selected_user: None,
                ..
            } => (true, tr(keys::Q_PEOPLE_SELECT_A_MEMBER_FIRST).to_string()),
            AdminDialogKind::EditRights { user_id, rights } => {
                let Some(rights) = rights else {
                    self.admin.admin_dialog = Some(dialog);
                    self.connection.set_note(StatusNote::silent(tr(
                        keys::Q_PEOPLE_RIGHTS_ARE_STILL_LOADING,
                    )));
                    cx.notify();
                    return;
                };
                match self.live.as_mut() {
                    Some(live) => {
                        match live
                            .driver
                            .edit_admin_rights(dialog.chat_id, *user_id, rights)
                        {
                            Ok(_) => (false, tr(keys::Q_PEOPLE_ADMIN_RIGHTS_UPDATED).to_string()),
                            Err(_) => (
                                true,
                                tr(keys::Q_PEOPLE_COULD_NOT_UPDATE_ADMIN_RIGHTS).to_string(),
                            ),
                        }
                    }
                    None => (
                        true,
                        tr(keys::Q_PEOPLE_ADMIN_ACTIONS_NEED_A_LIVE).to_string(),
                    ),
                }
            }
            AdminDialogKind::DemoteConfirm { user_id } => match self.live.as_mut() {
                Some(live) => match live.driver.demote_chat_member(dialog.chat_id, *user_id) {
                    Ok(_) => (false, tr(keys::Q_PEOPLE_ADMIN_DEMOTED).to_string()),
                    Err(_) => (true, tr(keys::Q_PEOPLE_COULD_NOT_DEMOTE_ADMIN).to_string()),
                },
                None => (
                    true,
                    tr(keys::Q_PEOPLE_ADMIN_ACTIONS_NEED_A_LIVE).to_string(),
                ),
            },
        };
        if keep_dialog {
            self.admin.admin_dialog = Some(dialog);
        }
        self.connection.set_note(StatusNote::info(note));
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let defaults = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .and_then(|chat| chat.permissions)
            .unwrap_or_else(ChatPermissions::all);
        // Editing an existing exception starts from its own rights and
        // end date (tdesktop `EditRestrictedBox` with the old rights).
        let existing = if ban {
            None
        } else {
            self.restricted_member_entry(chat_id, user_id)
        };
        let current = existing.map_or(defaults, |r| r.permissions);
        let until = existing.map(|r| initial_until(r.until_date, WallClock::of(cx).now()));
        self.admin.restrict_dialog = Some(RestrictDialog::new(
            window, cx, chat_id, user_id, ban, current, until,
        ));
        cx.notify();
    }

    pub(super) fn close_restrict_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.restrict_dialog = None;
        cx.notify();
    }

    pub(super) fn toggle_restrict_permission(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(dialog) = self.admin.restrict_dialog.as_mut() {
            let enabled = chat_permission_get(&dialog.permissions, index);
            chat_permission_set(&mut dialog.permissions, index, !enabled);
            cx.notify();
        }
    }

    /// Choose how long the restriction or ban lasts.
    pub(super) fn pick_restrict_until(&mut self, until: RestrictUntil, cx: &mut Context<Self>) {
        if let Some(dialog) = self.admin.restrict_dialog.as_mut() {
            dialog.until = until;
            dialog.error = None;
            cx.notify();
        }
    }

    /// Slice G1: submit restrict/ban (`setChatMemberStatus`, schema
    /// 1.8.67 line 13592). Duration is now + days; 0 = forever.
    pub(super) fn submit_restrict_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(mut dialog) = self.admin.restrict_dialog.take() else {
            return;
        };
        // A custom time is read from the picker and bounded like
        // tdesktop's `ChooseDateTimeBox` (30 seconds to 366 days ahead);
        // a bad pick keeps the dialog open with the reason.
        let now = WallClock::of(cx).now();
        let until_date = match dialog.until {
            RestrictUntil::Custom(_) => match picked_unix(&dialog.custom, MIN_RESTRICT_SECS, cx)
                .ok_or(UntilError::Invalid)
                .and_then(|unix| validate_restrict_until(unix, now))
            {
                Ok(unix) => RestrictUntil::Custom(unix).until_date(now),
                Err(err) => {
                    dialog.error = Some(err.message());
                    self.admin.restrict_dialog = Some(dialog);
                    cx.notify();
                    return;
                }
            },
            preset => preset.until_date(now),
        };
        let note = match self.live.as_mut() {
            Some(live) => {
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
                        self.admin.add_special = None;
                        if dialog.ban {
                            StatusNote::silent(tr(keys::Q_PEOPLE_MEMBER_BANNED))
                        } else {
                            StatusNote::silent(tr(keys::Q_PEOPLE_MEMBER_RESTRICTED))
                        }
                    }
                    Ok(None) | Err(_) => {
                        self.admin.restrict_dialog = Some(dialog);
                        StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_UPDATE_MEMBER_STATUS))
                    }
                }
            }
            None => {
                self.admin.restrict_dialog = Some(dialog);
                StatusNote::info(tr(keys::Q_PEOPLE_MEMBER_ACTIONS_NEED_A_LIVE))
            }
        };
        self.connection.set_note(note);
        cx.notify();
    }

    /// Slice G1: lift a restriction or ban (`setChatMemberStatus` →
    /// `chatMemberStatusMember`).
    pub(super) fn unban_member(&mut self, chat_id: ChatId, user_id: i64, cx: &mut Context<Self>) {
        let note = match self.live.as_mut() {
            // Slice G1 fix-up: `Ok(None)` is a driver refusal, not
            // success — report it honestly.
            Some(live) => match live.driver.unban_chat_member(chat_id, user_id) {
                Ok(Some(_)) => StatusNote::silent(tr(keys::Q_PEOPLE_MEMBER_UNBANNED)),
                Ok(None) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_UNBAN_MEMBER)),
                Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_UNBAN_MEMBER)),
            },
            None => StatusNote::info(tr(keys::Q_PEOPLE_MEMBER_ACTIONS_NEED_A_LIVE)),
        };
        self.connection.set_note(note);
        self.refresh_member_dialog(cx);
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
            let Some(dialog_state) = this.admin.restrict_dialog.as_ref() else {
                return dialog
                    .title(crate::ui::shell::dialog_title(tr(keys::Q_PEOPLE_RESTRICT)))
                    .on_close(on_close);
            };
            let name = this
                .session()
                .and_then(|session| session.user(dialog_state.user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| {
                    tr_with(
                        keys::Q_PEOPLE_USER,
                        &[("user_id", &dialog_state.user_id.to_string())],
                    )
                });
            let title = format!(
                "{} {name}",
                if dialog_state.ban {
                    tr(keys::Q_PEOPLE_BAN)
                } else {
                    tr(keys::Q_PEOPLE_RESTRICT)
                }
            );
            let mut body = div().flex().flex_col().gap_2();
            if !dialog_state.ban {
                body = body
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PEOPLE_ALLOWED_WHILE_RESTRICTED)),
                    )
                    .child(this.restrict_permission_checkboxes(&dialog_state.permissions, cx));
            }
            let until = dialog_state.until;
            let selected_until = match until {
                RestrictUntil::Forever => 0,
                RestrictUntil::Day => 1,
                RestrictUntil::Week => 2,
                RestrictUntil::Month => 3,
                RestrictUntil::Custom(_) => 4,
            };
            let until_weak = cx.weak_entity();
            let mut until_bar = TabBar::new("g1-restrict-until").segmented();
            for preset in RestrictUntil::PRESETS {
                until_bar = until_bar.child(Tab::new().label(preset.label()));
            }
            until_bar = until_bar.child(Tab::new().label(RestrictUntil::Custom(0).label()));
            body = body
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if dialog_state.ban {
                            tr(keys::Q_PEOPLE_BANNED_UNTIL)
                        } else {
                            tr(keys::Q_PEOPLE_RESTRICTED_UNTIL)
                        }),
                )
                .child(
                    until_bar
                        .selected_index(selected_until)
                        .on_click(move |ix, window, cx| {
                            let choice = match *ix {
                                0 => RestrictUntil::Forever,
                                1 => RestrictUntil::Day,
                                2 => RestrictUntil::Week,
                                3 => RestrictUntil::Month,
                                _ => RestrictUntil::Custom(0),
                            };
                            let _ = until_weak.update(cx, |this, cx| {
                                this.pick_restrict_until(choice, cx);
                                this.close_kit_dialog_if_done(DialogKind::Restrict, window, cx);
                            });
                        }),
                );
            if matches!(until, RestrictUntil::Custom(_)) {
                body = body.child(
                    DatePicker::new(&dialog_state.custom.state)
                        .placeholder(tr(keys::Q_PEOPLE_PICK_A_DATE_AND_TIME)),
                );
            }
            if !dialog_state.ban
                && let Some(line) =
                    this.restricted_by_line(dialog_state.chat_id, dialog_state.user_id)
            {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(line),
                );
            }
            if let Some(error) = dialog_state.error {
                body = body.child(
                    div()
                        .id("g1-restrict-error")
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                );
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-restrict-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_restrict_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Restrict, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-restrict-submit")
                        .label(if dialog_state.ban {
                            tr(keys::Q_PEOPLE_BAN)
                        } else {
                            tr(keys::Q_PEOPLE_RESTRICT)
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_restrict_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Restrict, window, cx);
                        })),
                );
            let body = body.into_any_element();
            dialog
                .title(crate::ui::shell::dialog_title(title))
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
                        .label(tr(*label))
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
        let dialog = self.admin.admin_dialog.as_ref()?;
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
                        .label(tr(*label))
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
        let seed = match self.admin.admin_dialog.as_ref() {
            Some(dialog) => match &dialog.kind {
                AdminDialogKind::EditRights {
                    user_id,
                    rights: None,
                } if prefix == "admin-edit-right" => {
                    let cached = self
                        .session()
                        .and_then(|session| {
                            session
                                .groups
                                .admin_rights
                                .get(&(dialog.chat_id.0, *user_id))
                        })
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
        if let Some(dialog) = self.admin.admin_dialog.as_mut() {
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
}

crate::ui::shell::register_dialogs! {
    Restrict => DialogSpec::new(
        4290,
        |app| app.admin.restrict_dialog.is_some(),
        QuillApp::build_restrict_dialog,
    ),
}

mod add_special;
mod permissions_box;
mod promote_dialog_panel;
mod stops_slider;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod stops_slider_tests;
