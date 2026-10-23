//! The group permissions box (tdesktop `ShowEditPeerPermissionsBox`,
//! `boxes/peers/edit_peer_permissions_box.cpp`): "Send messages", the
//! "Send media" group with its ten sub-permissions, the rows after it,
//! the boosters and slow mode sliders, the broadcast group offer and the
//! exceptions and removed users lists. The rules live in
//! `quill::permissions_box`; this file draws the staged dialog state.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::dialogs::PermissionsDialog;
use super::super::dialogs::{GroupConfirmAction, MemberTab};
use super::super::pressable::action_row;
use super::super::shell::{DialogKind, QuillShell};
use super::stops_slider::{
    BOOSTS_KEYS, SLOW_MODE_KEYS, StopsSlider, slow_mode_about, slow_mode_label,
};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::separator::Separator;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::permissions_box::{
    BOOSTS_STOPS, MEDIA, Permission, PermissionsSave, SLOW_MODE_STOPS, bottom_rows,
    has_send_restrictions, initial, lock, media_on, set_media, suggests_broadcast_group, toggle,
};
use quill::telegram::envelope::ChatPermissions;
use std::cell::RefCell;
use std::rc::Rc;

fn boosts_label(boosts: i32) -> String {
    boosts.to_string()
}

impl QuillApp {
    /// Open the box on a group the viewer may edit, seeded from the chat.
    pub(in crate::ui) fn open_permissions_dialog(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(context) = session.permissions_box_context(chat_id) else {
            return;
        };
        let defaults = session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.permissions)
            .unwrap_or_else(ChatPermissions::all);
        self.admin.permissions_dialog = Some(PermissionsDialog {
            chat_id,
            context,
            permissions: initial(defaults, &context),
            media_open: false,
            slow_mode_delay: session.chat_slow_mode_delay(chat_id),
            boosts: session.chat_unrestrict_boost_count(chat_id),
            lock_hint: None,
        });
        cx.notify();
    }

    pub(super) fn close_permissions_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.permissions_dialog = None;
        cx.notify();
    }

    fn update_permissions_box(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut PermissionsDialog),
    ) {
        if let Some(dialog) = self.admin.permissions_dialog.as_mut() {
            change(dialog);
            cx.notify();
        }
    }

    /// A row's switch or checkbox. A locked row keeps its value and says
    /// why instead.
    pub(super) fn permissions_box_toggle(
        &mut self,
        row: Permission,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_permissions_box(cx, |dialog| {
            if lock(&dialog.context, row).is_some() {
                dialog.lock_hint = Some(row);
                return;
            }
            dialog.lock_hint = None;
            dialog.permissions = toggle(dialog.permissions, row, on);
        });
    }

    /// Save (tdesktop `EditPeerPermissionsBoxResult`). Sends only what
    /// changed; a basic group with slow mode or boosters set is upgraded
    /// first. Offline, the demo session takes the values directly.
    pub(super) fn submit_permissions_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin.permissions_dialog.take() else {
            return;
        };
        let save = PermissionsSave::new(dialog.permissions, dialog.slow_mode_delay, dialog.boosts);
        if let Some(live) = self.live.as_mut() {
            match live.driver.save_permissions_box(dialog.chat_id, save) {
                Ok(true) => self
                    .connection
                    .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_PERMISSIONS_UPDATED))),
                Ok(false) => {}
                Err(_) => {
                    self.admin.permissions_dialog = Some(dialog);
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_PEOPLE_COULD_NOT_UPDATE_PERMISSIONS,
                    )));
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.apply_permissions_save_locally(dialog.chat_id, save);
        }
        cx.notify();
    }

    /// "Exceptions" and "Removed users": the member list on that tab,
    /// shown above the box; closing it comes back here.
    fn open_permissions_list(
        &mut self,
        chat_id: ChatId,
        tab: MemberTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_member_dialog(chat_id, window, cx);
        self.member_dialog_tab(tab, cx);
    }

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
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::MANAGE_PEER_PERMISSIONS,
                )));
            let Some(body) = this.permissions_box_body(cx) else {
                return dialog.on_close(on_close);
            };
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-permissions-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_permissions_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Permissions, window, cx);
                        })),
                )
                .child(
                    div().debug_selector(|| "permissions-save".into()).child(
                        Button::new("g1-permissions-submit")
                            .primary()
                            .label(tr(keys::SETTINGS_SAVE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_permissions_dialog(cx);
                                this.close_kit_dialog_if_done(DialogKind::Permissions, window, cx);
                            })),
                    ),
                );
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
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

    fn permissions_box_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.admin.permissions_dialog.as_ref()?;
        let session = self.session()?;
        let chat_id = dialog.chat_id;
        let permissions = dialog.permissions;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent_foreground;
        let heading = |text: &'static str| {
            div()
                .px_2()
                .pt_2()
                .pb_1()
                .text_sm()
                .font_semibold()
                .text_color(accent)
                .child(text)
        };
        let note = |text: SharedString| div().px_2().pb_2().text_xs().text_color(muted).child(text);
        let mut body = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(heading(tr(keys::RIGHTS_DEFAULT_RESTRICTIONS_HEADER)))
            .child(self.permission_switch_row(dialog, Permission::SendText, cx))
            .child(self.media_group(dialog, cx));
        for row in bottom_rows(dialog.context.is_forum) {
            body = body.child(self.permission_switch_row(dialog, row, cx));
        }
        body = body.child(div().py_2().child(Separator::horizontal()));
        if session.permissions_box_has_slow_mode(chat_id) {
            if has_send_restrictions(&permissions, dialog.slow_mode_delay) {
                body = body.child(self.boosts_section(dialog, cx));
            }
            body = body
                .child(heading(tr(keys::RIGHTS_SLOWMODE_HEADER)))
                .child(
                    div().px_2().pb_1().child(
                        StopsSlider::new(
                            SLOW_MODE_KEYS,
                            chat_id.0,
                            &SLOW_MODE_STOPS,
                            dialog.slow_mode_delay,
                            slow_mode_label,
                            {
                                let app = cx.weak_entity();
                                move |delay, cx| {
                                    app.update(cx, |this, cx| {
                                        this.update_permissions_box(cx, |dialog| {
                                            dialog.slow_mode_delay = delay
                                        })
                                    })
                                    .ok();
                                }
                            },
                        )
                        .accessibility_label(tr(keys::RIGHTS_SLOWMODE_HEADER)),
                    ),
                )
                .child(note(slow_mode_about(dialog.slow_mode_delay).into()));
        }
        let member_count = session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| session.group_member_counts(chat))
            .map_or(0, |(members, _)| members);
        if session.chat_is_owner(chat_id)
            && session.chat_supergroup(chat_id).is_some()
            && suggests_broadcast_group(member_count, session.groups.supergroup_size_max)
        {
            body = body
                .child(heading(tr(keys::RIGHTS_GIGAGROUP_TITLE)))
                .child(
                    action_row(
                        "permissions-broadcast",
                        Some(IconName::Megaphone),
                        tr(keys::RIGHTS_GIGAGROUP_CONVERT),
                        false,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_permissions_dialog(cx);
                        this.open_group_confirm(chat_id, GroupConfirmAction::BroadcastIntro, cx);
                        this.close_kit_dialog_if_done(DialogKind::Permissions, window, cx);
                    })),
                )
                .child(note(tr(keys::RIGHTS_GIGAGROUP_ABOUT).into()));
        }
        // tdesktop `AddBannedButtons`. TDLib keeps restricted members for
        // supergroups only, so a basic group has neither list.
        if session.chat_supergroup(chat_id).is_some() {
            let counts = session.chat_member_counts(chat_id);
            body = body
                .child(self.permissions_list_row(
                    "permissions-exceptions",
                    IconName::Shield,
                    tr(keys::MANAGE_PEER_EXCEPTIONS),
                    counts.restricted,
                    MemberTab::Restricted,
                    chat_id,
                    cx,
                ))
                .child(self.permissions_list_row(
                    "permissions-removed",
                    IconName::UserX,
                    tr(keys::MANAGE_PEER_REMOVED_USERS),
                    counts.banned,
                    MemberTab::Banned,
                    chat_id,
                    cx,
                ));
        }
        Some(body.into_any_element())
    }

    /// One row with a trailing switch. A locked row shows a lock and, once
    /// clicked, its reason under it.
    fn permission_switch_row(
        &self,
        dialog: &PermissionsDialog,
        row: Permission,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = row.get(&dialog.permissions);
        let locked = lock(&dialog.context, row);
        let muted = cx.theme().muted_foreground;
        let id = SharedString::from(format!("permission-{}", row.id()));
        let line = action_row(id.clone(), None, tr(row.label()), false, cx)
            .aria_selected(on)
            .child(div().flex_1())
            .when(locked.is_some(), |this| {
                this.child(Icon::new(IconName::Lock).size(px(14.)).text_color(muted))
            })
            .child(
                Switch::new(SharedString::from(format!("{id}-switch")))
                    .checked(on)
                    .small()
                    .tab_stop(false)
                    .disabled(locked.is_some()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.permissions_box_toggle(row, !on, cx);
            }));
        let hint = locked.filter(|_| dialog.lock_hint == Some(row));
        div()
            .flex()
            .flex_col()
            .child(line)
            .when_some(hint, |this, lock| {
                this.child(
                    div()
                        .px_2()
                        .pb_1()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(lock.message())),
                )
            })
            .into_any_element()
    }

    /// "Send media 7/10" with its switch, and the ten checkboxes under it
    /// while expanded (tdesktop `CreateEditRestrictions` nested group).
    fn media_group(&self, dialog: &PermissionsDialog, cx: &mut Context<Self>) -> AnyElement {
        let count = media_on(&dialog.permissions);
        let open = dialog.media_open;
        let muted = cx.theme().muted_foreground;
        let label = tr(keys::RIGHTS_CHAT_SEND_MEDIA);
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .child(
                action_row("permission-media-expand", None, label, false, cx)
                    .debug_selector(|| "permission-media-expand".into())
                    .flex_1()
                    .aria_expanded(open)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child(format!("{count}/{}", MEDIA.len())),
                    )
                    .child(
                        Icon::new(if open {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .size(px(14.))
                        .text_color(muted),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_permissions_box(cx, |dialog| {
                            dialog.media_open = !dialog.media_open
                        });
                    })),
            )
            .child(
                div().pr_2().child(
                    Switch::new("permission-media")
                        .checked(count > 0)
                        .small()
                        .accessibility_label(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_permissions_box(cx, |dialog| {
                                dialog.lock_hint = None;
                                dialog.permissions = set_media(dialog.permissions, count == 0);
                            });
                        })),
                ),
            );
        let mut rows = div().flex().flex_col().gap_1p5().pl_6().pr_2().py_1();
        for row in MEDIA {
            let on = row.get(&dialog.permissions);
            rows = rows.child(
                div()
                    .debug_selector(move || format!("permission-{}", row.id()))
                    .child(
                        Checkbox::new(SharedString::from(format!("permission-{}", row.id())))
                            .checked(on)
                            .label(tr(row.label()))
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                this.permissions_box_toggle(row, *checked, cx);
                            })),
                    ),
            );
        }
        Collapsible::new()
            .open(open)
            .motion_id("permission-media-group")
            .child(header)
            .content(rows)
            .into_any_element()
    }

    /// tdesktop `AddBoostsUnrestrictSlider`: shown while anything is
    /// restricted; the slider only while the switch is on.
    fn boosts_section(&self, dialog: &PermissionsDialog, cx: &mut Context<Self>) -> AnyElement {
        let on = dialog.boosts > 0;
        let muted = cx.theme().muted_foreground;
        let row = action_row(
            "permissions-boosters",
            None,
            tr(keys::RIGHTS_BOOSTS_NO_RESTRICT),
            false,
            cx,
        )
        .aria_selected(on)
        .child(div().flex_1())
        .child(
            Switch::new("permissions-boosters-switch")
                .checked(on)
                .small()
                .tab_stop(false),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.update_permissions_box(cx, |dialog| {
                dialog.boosts = if on { 0 } else { 1 };
            });
        }));
        let about = if on {
            keys::RIGHTS_BOOSTS_ABOUT_ON
        } else {
            keys::RIGHTS_BOOSTS_ABOUT
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(row)
            .when(on, |this| {
                this.child(
                    div().px_2().child(
                        StopsSlider::new(
                            BOOSTS_KEYS,
                            dialog.chat_id.0,
                            &BOOSTS_STOPS,
                            dialog.boosts,
                            boosts_label,
                            {
                                let app = cx.weak_entity();
                                move |boosts, cx| {
                                    app.update(cx, |this, cx| {
                                        this.update_permissions_box(cx, |dialog| {
                                            dialog.boosts = boosts
                                        })
                                    })
                                    .ok();
                                }
                            },
                        )
                        .accessibility_label(tr(keys::RIGHTS_BOOSTS_NO_RESTRICT)),
                    ),
                )
            })
            .child(
                div()
                    .px_2()
                    .pb_2()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(about)),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn permissions_list_row(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        count: i32,
        tab: MemberTab,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        action_row(id, Some(icon), label, false, cx)
            .child(div().flex_1())
            .when(count > 0, |this| {
                this.child(div().text_sm().text_color(muted).child(count.to_string()))
            })
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .text_color(muted),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_permissions_list(chat_id, tab, window, cx);
            }))
            .into_any_element()
    }
}

crate::ui::shell::register_dialogs! {
    Permissions => DialogSpec::new(
        4400,
        |app| app.admin.permissions_dialog.is_some(),
        QuillApp::build_permissions_dialog,
    ),
}
