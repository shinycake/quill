//! The Devices page sessions (tdesktop `Settings::Sessions`): one row per
//! session with its device picture, the session box it opens
//! (`SessionInfoBox`) and the terminate confirmations.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::confirm_boxes::{Ask, confirm_box};
use crate::ui::format_helpers::format_session_last_active;
use crate::ui::settings_window::rows::{group, switch_row};
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use quill::privacy::{SESSION_TTL_DAYS, session_ttl_label};
use quill::state::PrivacyControl;
use quill::telegram::envelope::SessionDevice;

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 400.;

fn device_icon(device: SessionDevice) -> IconName {
    match device {
        SessionDevice::Phone => IconName::Smartphone,
        SessionDevice::Tablet => IconName::Tablet,
        SessionDevice::Mac => IconName::Laptop,
        SessionDevice::Desktop | SessionDevice::Other => IconName::Monitor,
        SessionDevice::Browser => IconName::Globe,
    }
}

/// The round device picture: tinted by kind, like tdesktop's colored
/// session userpics.
fn device_badge(device: SessionDevice, size: f32, cx: &App) -> Div {
    let tint = match device {
        SessionDevice::Phone | SessionDevice::Tablet => cx.theme().success,
        SessionDevice::Mac | SessionDevice::Desktop => cx.theme().primary,
        SessionDevice::Browser => cx.theme().warning,
        SessionDevice::Other => cx.theme().muted_foreground,
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded_full()
        .bg(tint.opacity(0.14))
        .text_color(tint)
        .child(Icon::new(device_icon(device)).size(px(size * 0.5)))
}

fn session_title(s: &ParsedSession) -> String {
    if s.device_model.trim().is_empty() {
        tr(keys::Q_PREFS_UNKNOWN_DEVICE).to_string()
    } else {
        s.device_model.clone()
    }
}

fn joined(parts: [&str; 2], separator: &str) -> String {
    let parts: Vec<&str> = parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    parts.join(separator)
}

/// "Online" for this device, otherwise when it was last active.
fn activity(s: &ParsedSession) -> String {
    if s.is_current {
        tr(keys::Q_PREFS_ONLINE).to_string()
    } else {
        format_session_last_active(s.last_active_date)
    }
}

impl QuillApp {
    /// One session: device picture, name, "app version, platform", then
    /// where and when. The row opens the session box; other sessions get
    /// a terminate button on the right (tdesktop `SessionsContent::List`).
    pub(super) fn session_row(
        &self,
        s: &ParsedSession,
        mutating: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session_id = s.id;
        let incomplete = s.is_password_pending;
        let title = session_title(s);
        let app = joined(
            [
                &joined([&s.application_name, &s.application_version], " "),
                &joined([&s.platform, &s.system_version], " "),
            ],
            ", ",
        );
        let place = if s.location.trim().is_empty() {
            &s.ip_address
        } else {
            &s.location
        };
        let place = joined([place, &activity(s)], " · ");
        let muted = cx.theme().muted_foreground;
        let text = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .truncate()
                    .child(title.clone()),
            )
            .when(!app.is_empty(), |this| {
                this.child(div().text_xs().text_color(muted).truncate().child(app))
            })
            .child(div().text_xs().text_color(muted).truncate().child(place));
        let row = Button::new(format!("session-row-{session_id}"))
            .ghost()
            .w_full()
            .h_auto()
            .accessibility_label(title)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .w_full()
                    .py_1()
                    .child(device_badge(s.device_type, 40., cx))
                    .child(text),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.privacy.extra.session_details = Some(session_id);
                this.open_box(DialogKind::SessionDetails);
                this.privacy.extra.renaming_device = false;
                cx.notify();
            }));
        div()
            .flex()
            .items_center()
            .gap_1()
            .w_full()
            .child(div().flex_1().min_w_0().child(row))
            .when(!s.is_current, |this| {
                this.child(
                    Button::new(format!("terminate-session-{session_id}"))
                        .icon(IconName::X)
                        .ghost()
                        .small()
                        .tooltip(tr(keys::SESSIONS_TERMINATE))
                        .accessibility_label(tr(keys::SESSIONS_TERMINATE))
                        .disabled(mutating)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.begin_terminate_session(session_id, incomplete, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// "Terminate old sessions": the period as one menu button
    /// (tdesktop's "If inactive for..." row).
    pub(super) fn sessions_ttl_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let current = session.and_then(|s| s.settings.privacy_data.inactive_session_ttl_days);
        let error = session.and_then(|s| {
            s.settings
                .privacy_data
                .error(PrivacyControl::SessionTtl)
                .map(str::to_string)
        });
        let owner = cx.entity().downgrade();
        let period = Button::new("sessions-ttl")
            .label(match current {
                Some(days) => session_ttl_label(days),
                None => tr(keys::Q_PREFS_LOADING).to_string(),
            })
            .small()
            .ghost()
            .dropdown_caret(true)
            .disabled(current.is_none())
            .dropdown_menu(move |mut menu: PopupMenu, _, _| {
                for days in SESSION_TTL_DAYS {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(session_ttl_label(days))
                            .checked(current == Some(days))
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| this.set_session_ttl(days, cx));
                            }),
                    );
                }
                menu
            });
        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .w_full()
            .min_h(px(40.))
            .px_3()
            .child(div().text_sm().child(tr(keys::Q_PREFS_IF_INACTIVE_FOR)))
            .child(period)
            .into_any_element();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(group(Some(tr(keys::Q_PREFS_TERMINATE_OLD_SESSIONS)), [row]))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::SELF_DESTRUCT_SESSIONS_DESCRIPTION)),
            )
            .when_some(error, |this, line| {
                this.child(div().text_xs().text_color(danger()).child(line))
            })
            .into_any_element()
    }

    fn close_session_details(&mut self, cx: &mut Context<Self>) {
        self.privacy.extra.session_details = None;
        self.privacy.extra.renaming_device = false;
        cx.notify();
    }

    fn details_session(&self) -> Option<ParsedSession> {
        let id = self.privacy.extra.session_details?;
        self.session()?
            .settings
            .sessions
            .data()?
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    /// The session box body: the device, its info rows and, for a signed
    /// in session, the switches that pick what it accepts.
    fn session_box_body(&self, s: &ParsedSession, cx: &mut Context<Self>) -> Div {
        if s.is_current && self.privacy.extra.renaming_device {
            return self.rename_device_fields(cx);
        }
        let muted = cx.theme().muted_foreground;
        let date = if s.is_current {
            activity(s)
        } else {
            tr_with(keys::Q_LEFTOVERS_LAST_ACTIVE, &[("date", &activity(s))])
        };
        let mut body = div().flex().flex_col().gap_3().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .pb_1()
                .child(device_badge(s.device_type, 72., cx))
                .child(
                    div()
                        .pt_2()
                        .text_lg()
                        .font_semibold()
                        .text_center()
                        .child(session_title(s)),
                )
                .child(div().text_sm().text_color(muted).child(date)),
        );
        let app = joined([&s.application_name, &s.application_version], " ");
        let app = if s.is_official_application && !app.is_empty() {
            format!("{app}{}", tr(keys::Q_PREFS_OFFICIAL))
        } else {
            app
        };
        let mut rows: Vec<(IconName, &'static str, String)> = vec![
            (IconName::AppWindow, tr(keys::SESSIONS_APPLICATION), app),
            (
                device_icon(s.device_type),
                tr(keys::SESSIONS_SYSTEM),
                joined([&s.platform, &s.system_version], " "),
            ),
            (
                IconName::Network,
                tr(keys::SESSIONS_IP),
                s.ip_address.clone(),
            ),
            (
                IconName::MapPin,
                tr(keys::SESSIONS_LOCATION),
                s.location.clone(),
            ),
        ];
        if s.log_in_date > 0 {
            rows.push((
                IconName::Info,
                tr(keys::Q_PREFS_LOGGED_IN),
                format_session_last_active(s.log_in_date),
            ));
        }
        let mut info = div().flex().flex_col().gap_2().child(
            div()
                .text_xs()
                .font_medium()
                .text_color(muted)
                .child(tr(keys::SESSIONS_INFO)),
        );
        for (icon, label, value) in rows.into_iter().filter(|row| !row.2.is_empty()) {
            info = info.child(
                div()
                    .id(SharedString::from(format!("session-detail-{label}")))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Icon::new(icon).size_4().text_color(muted))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_sm().child(value))
                            .child(div().text_xs().text_color(muted).child(label)),
                    ),
            );
        }
        body = body.child(info).child(
            div()
                .text_xs()
                .text_color(muted)
                .child(tr(keys::SESSIONS_LOCATION_ABOUT)),
        );
        if !s.is_password_pending {
            let session_id = s.id;
            let (secret_on, calls_on) = (s.can_accept_secret_chats, s.can_accept_calls);
            let owner = cx.entity().downgrade();
            let calls_owner = owner.clone();
            // `switch_row` pads its own sides; pull it out to line the
            // titles up with the info rows.
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(muted)
                            .child(tr(keys::Q_PREFS_SESSION_ACCEPTS)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .mx(px(-12.))
                            .child(switch_row(
                                format!("toggle-secret-chats-{session_id}"),
                                tr(keys::Q_PREFS_SECRET_CHATS),
                                None,
                                secret_on,
                                move |on, _, cx| {
                                    if *on != secret_on {
                                        let _ = owner.update(cx, |this, cx| {
                                            this.toggle_session_secret_chats(session_id, cx);
                                        });
                                    }
                                },
                                cx,
                            ))
                            .child(switch_row(
                                format!("toggle-calls-{session_id}"),
                                tr(keys::SETTINGS_CALLS),
                                None,
                                calls_on,
                                move |on, _, cx| {
                                    if *on != calls_on {
                                        let _ = calls_owner.update(cx, |this, cx| {
                                            this.toggle_session_calls(session_id, cx);
                                        });
                                    }
                                },
                                cx,
                            )),
                    ),
            );
        }
        body
    }

    /// Done on the right; on the left Rename for this device, Terminate
    /// for any other. Renaming swaps them for Cancel and Save.
    fn session_box_footer(&self, s: &ParsedSession, mutating: bool, cx: &mut Context<Self>) -> Div {
        let session_id = s.id;
        let incomplete = s.is_password_pending;
        let footer = div().flex().items_center().gap_2().w_full();
        if s.is_current && self.privacy.extra.renaming_device {
            return footer
                .justify_end()
                .child(
                    Button::new("session-rename-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.privacy.extra.renaming_device = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("session-rename-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.save_device_name(cx))),
                );
        }
        let left = if s.is_current {
            Button::new("session-details-rename")
                .label(tr(keys::Q_PREFS_RENAME))
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| this.begin_rename_device(window, cx)))
        } else {
            Button::new(format!("session-details-terminate-{session_id}"))
                .label(tr(keys::SESSIONS_TERMINATE))
                .custom(quiet_danger(cx))
                .disabled(mutating)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.privacy.extra.session_details = None;
                    this.begin_terminate_session(session_id, incomplete, cx);
                }))
        };
        footer.justify_between().child(left).child(
            Button::new("session-details-done")
                .label(tr(keys::ABOUT_DONE))
                .primary()
                .on_click(cx.listener(|this, _, _, cx| this.close_session_details(cx))),
        )
    }

    fn build_session_details_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SessionDetails, |this, _, cx| {
                this.close_session_details(cx)
            });
        let (footer, renaming) = app.update(cx, |this, cx| {
            let mutating = this.session().is_some_and(|s| s.settings.sessions_mutating);
            let footer = this
                .details_session()
                .map(|s| this.session_box_footer(&s, mutating, cx));
            (footer, this.privacy.extra.renaming_device)
        });
        let body_app = app.clone();
        let ok_app = app.clone();
        let dialog = dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .when(renaming, |dialog| {
                dialog.title(crate::ui::shell::dialog_title(tr(
                    keys::Q_PREFS_RENAME_CURRENT_DEVICE,
                )))
            })
            .on_ok(move |_, _, cx| {
                ok_app.update(cx, |this, cx| {
                    if this.privacy.extra.renaming_device {
                        this.save_device_name(cx);
                    } else {
                        this.close_session_details(cx);
                    }
                });
                false
            })
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, _, cx| {
                    let body = body_app.update(cx, |this, cx| {
                        this.details_session()
                            .map(|s| this.session_box_body(&s, cx))
                    });
                    content.children(body)
                },
            ))
            .on_close(on_close);
        match footer {
            Some(footer) => dialog.footer(footer),
            None => dialog,
        }
    }

    fn build_sessions_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let (title, text) = match app.read(cx).privacy.sessions_confirm {
            Some(SessionsConfirm::TerminateAll) => (
                tr(keys::SESSIONS_TERMINATE_ALL),
                tr(keys::SETTINGS_RESET_SURE),
            ),
            Some(SessionsConfirm::TerminateOne {
                incomplete: true, ..
            }) => (
                tr(keys::SESSIONS_TERMINATE),
                tr(keys::Q_PREFS_TERMINATE_THIS_LOGIN_ATTEMPT),
            ),
            _ => (
                tr(keys::SESSIONS_TERMINATE),
                tr(keys::Q_PREFS_TERMINATE_THIS_SESSION),
            ),
        };
        let ask = Ask {
            title: title.into(),
            text: text.into(),
            confirm: tr(keys::SETTINGS_RESET_BUTTON).into(),
        };
        confirm_box(
            app,
            shell,
            dialog,
            DialogKind::SessionsConfirm,
            ask,
            (
                Self::cancel_sessions_confirm,
                Self::confirm_sessions_terminate,
            ),
            cx,
        )
    }
}

crate::ui::shell::register_dialogs! {
    SessionDetails => DialogSpec::new(
        614,
        |app| app.details_session().is_some(),
        QuillApp::build_session_details_dialog,
    )
    .close(|app, _, cx| app.close_session_details(cx)),

    SessionsConfirm => DialogSpec::new(
        615,
        |app| app.privacy.sessions_confirm.is_some(),
        QuillApp::build_sessions_confirm_dialog,
    )
    .close(|app, _, cx| app.cancel_sessions_confirm(cx)),
}
