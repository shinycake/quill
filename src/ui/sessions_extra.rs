//! B13: the Active Sessions additions (tdesktop `Settings::Sessions`):
//! "Terminate old sessions if inactive for ..." and the session details
//! view (application, system, IP address, location) with Terminate.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::danger;
use super::format_helpers::format_session_last_active;
use super::security::quiet_danger;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::privacy::{SESSION_TTL_DAYS, session_ttl_label};
use quill::telegram::envelope::ParsedSession;

impl QuillApp {
    /// "Terminate old sessions" with the five tdesktop periods as chips.
    pub(super) fn sessions_ttl_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let current = session.and_then(|s| s.settings.privacy_data.inactive_session_ttl_days);
        let error = session.and_then(|s| s.settings.privacy_data.error.clone());
        let mut chips = div().flex().flex_wrap().gap_1();
        for days in SESSION_TTL_DAYS {
            chips = chips.child(
                Button::new(format!("sessions-ttl-{days}"))
                    .label(session_ttl_label(days))
                    .small()
                    .when(current == Some(days), |b| b.primary())
                    .when(current != Some(days), |b| b.outline())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_session_ttl(days, cx);
                    })),
            );
        }
        let mut section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_TERMINATE_OLD_SESSIONS)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(tr(keys::Q_PREFS_IF_INACTIVE_FOR)))
                    .child(
                        div()
                            .id("sessions-ttl-value")
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(match current {
                                Some(days) => session_ttl_label(days),
                                None => tr(keys::Q_PREFS_LOADING).to_string(),
                            }),
                    ),
            )
            .child(chips)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::SELF_DESTRUCT_SESSIONS_DESCRIPTION)),
            );
        if let Some(line) = error {
            section = section.child(div().text_xs().text_color(danger()).child(line));
        }
        section.into_any_element()
    }

    fn set_session_ttl(&mut self, days: i32, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_inactive_session_ttl(days) {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_SESSION_TIMEOUT,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.privacy_data.inactive_session_ttl_days = Some(days);
        }
        cx.notify();
    }

    /// The details view of one session (tdesktop `SessionInfoBox`).
    pub(super) fn session_details_body(
        &self,
        s: &ParsedSession,
        mutating: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session_id = s.id;
        let incomplete = s.is_password_pending;
        let title = if s.device_model.trim().is_empty() {
            tr(keys::Q_PREFS_UNKNOWN_DEVICE).to_string()
        } else {
            s.device_model.clone()
        };
        let app_line = format!(
            "{} {}",
            s.application_name.trim(),
            s.application_version.trim()
        )
        .trim()
        .to_string();
        let system_line = format!("{} {}", s.platform.trim(), s.system_version.trim())
            .trim()
            .to_string();
        let mut rows: Vec<(&'static str, String)> = Vec::new();
        if !app_line.is_empty() {
            let official = if s.is_official_application {
                tr(keys::Q_PREFS_OFFICIAL)
            } else {
                ""
            };
            rows.push((
                tr(keys::SESSIONS_APPLICATION),
                format!("{app_line}{official}"),
            ));
        }
        if !system_line.is_empty() {
            rows.push((tr(keys::SESSIONS_SYSTEM), system_line));
        }
        if !s.ip_address.is_empty() {
            rows.push((tr(keys::SESSIONS_IP), s.ip_address.clone()));
        }
        if !s.location.is_empty() {
            rows.push((tr(keys::SESSIONS_LOCATION), s.location.clone()));
        }
        if s.log_in_date > 0 {
            rows.push((
                tr(keys::Q_PREFS_LOGGED_IN),
                format_session_last_active(s.log_in_date),
            ));
        }
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().font_semibold().child(title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if s.is_current {
                                tr(keys::Q_PREFS_ONLINE).to_string()
                            } else {
                                tr_with(
                                    keys::Q_LEFTOVERS_LAST_ACTIVE,
                                    &[(
                                        "date",
                                        &(format_session_last_active(s.last_active_date))
                                            .to_string(),
                                    )],
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::SESSIONS_INFO)),
            );
        for (label, value) in rows {
            body = body.child(
                div()
                    .id(format!("session-detail-{label}"))
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    )
                    .child(div().text_sm().text_right().child(value)),
            );
        }
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::SESSIONS_LOCATION_ABOUT)),
        );
        if s.is_current && self.privacy.extra.renaming_device {
            return body.child(self.rename_device_form(cx)).into_any_element();
        }
        let mut actions = div().flex().justify_between().gap_2().child(
            Button::new("session-details-back")
                .label(tr(keys::Q_PREFS_BACK))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.privacy.extra.session_details = None;
                    this.privacy.extra.renaming_device = false;
                    cx.notify();
                })),
        );
        if s.is_current {
            actions = actions.child(
                Button::new("session-details-rename")
                    .label(tr(keys::Q_PREFS_RENAME))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.begin_rename_device(window, cx);
                    })),
            );
        }
        if !s.is_current {
            actions = actions.child(
                Button::new(format!("session-details-terminate-{session_id}"))
                    .label(tr(keys::SESSIONS_TERMINATE))
                    .custom(quiet_danger(cx))
                    .disabled(mutating)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.privacy.extra.session_details = None;
                        this.begin_terminate_session(session_id, incomplete, cx);
                    })),
            );
        }
        body.child(actions).into_any_element()
    }

    fn begin_rename_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .session()
            .map(|s| s.settings.device_prefs.custom_device_model.clone())
            .unwrap_or_default();
        self.privacy
            .extra
            .rename_input
            .update(cx, |input, cx| input.set_value(current, window, cx));
        self.privacy.extra.renaming_device = true;
        cx.notify();
    }

    /// tdesktop `RenameBox`: a device name field with Save and Cancel.
    fn rename_device_form(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::Q_PREFS_RENAME_CURRENT_DEVICE)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_DEVICE_NAME)),
            )
            .child(
                Input::new(&self.privacy.extra.rename_input)
                    .aria_label(tr(keys::Q_PREFS_DEVICE_NAME)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_THE_NEW_NAME_APPLIES_AFTER)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
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
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_device_name(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn save_device_name(&mut self, cx: &mut Context<Self>) {
        let raw = self.privacy.extra.rename_input.read(cx).value().to_string();
        let prefs = quill::settings::DevicePrefs {
            custom_device_model: quill::settings::clean_device_model(&raw),
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.device_prefs = prefs;
            live.driver.save_device_prefs();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.device_prefs = prefs;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEMO_DEVICE_NAME_IS_NOT,
            )));
        }
        self.privacy.extra.renaming_device = false;
        cx.notify();
    }
}
