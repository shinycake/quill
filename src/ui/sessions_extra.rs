//! B13: the Active Sessions additions (tdesktop `Settings::Sessions`):
//! "Terminate old sessions if inactive for ..." and the session details
//! view (application, system, IP address, location) with Terminate.

use super::app::QuillApp;
use super::chat_theme::danger;
use super::format_helpers::format_session_last_active;
use super::security::quiet_danger;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::privacy::{SESSION_TTL_DAYS, session_ttl_label};
use quill::telegram::envelope::ParsedSession;

impl QuillApp {
    /// "Terminate old sessions" with the five tdesktop periods as chips.
    pub(super) fn sessions_ttl_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let current = session.and_then(|s| s.privacy_data.inactive_session_ttl_days);
        let error = session.and_then(|s| s.privacy_data.error.clone());
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
                    .child("Terminate old sessions"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("If inactive for..."))
                    .child(
                        div()
                            .id("sessions-ttl-value")
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(match current {
                                Some(days) => session_ttl_label(days),
                                None => "Loading…".to_string(),
                            }),
                    ),
            )
            .child(chips)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "If you don't come online from a specific session at least once within this period, it will be terminated.",
                    ),
            );
        if let Some(line) = error {
            section = section.child(div().text_xs().text_color(danger()).child(line));
        }
        section.into_any_element()
    }

    fn set_session_ttl(&mut self, days: i32, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_inactive_session_ttl(days) {
                self.status_note = format!("couldn't change the session timeout: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.privacy_data.inactive_session_ttl_days = Some(days);
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
            "Unknown device".to_string()
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
                " (official)"
            } else {
                ""
            };
            rows.push(("Application", format!("{app_line}{official}")));
        }
        if !system_line.is_empty() {
            rows.push(("System version", system_line));
        }
        if !s.ip_address.is_empty() {
            rows.push(("IP address", s.ip_address.clone()));
        }
        if !s.location.is_empty() {
            rows.push(("Location", s.location.clone()));
        }
        if s.log_in_date > 0 {
            rows.push(("Logged in", format_session_last_active(s.log_in_date)));
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
                                "Online".to_string()
                            } else {
                                format!(
                                    "Last active: {}",
                                    format_session_last_active(s.last_active_date)
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child("Info"),
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
                .child(
                    "This location estimate is based on the IP address and may not always be accurate.",
                ),
        );
        let mut actions = div().flex().justify_between().gap_2().child(
            Button::new("session-details-back")
                .label("Back")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.privacy_ui.session_details = None;
                    cx.notify();
                })),
        );
        if !s.is_current {
            actions = actions.child(
                Button::new(format!("session-details-terminate-{session_id}"))
                    .label("Terminate Session")
                    .custom(quiet_danger(cx))
                    .disabled(mutating)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.privacy_ui.session_details = None;
                        this.begin_terminate_session(session_id, incomplete, cx);
                    })),
            );
        }
        body.child(actions).into_any_element()
    }
}
