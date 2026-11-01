//! What a message notification reveals: tdesktop's "Name" and "Text"
//! switches (`Core::Settings::NotifyView`), shown while desktop
//! notifications are on, and Quill's "Show photo previews" (tdesktop shows
//! the sender's picture only).

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};
use quill::notify::NotifyView;

impl QuillApp {
    pub(in crate::ui) fn notify_view(&self) -> NotifyView {
        self.session()
            .map(|s| s.settings.notify_view)
            .unwrap_or_default()
    }

    /// Save the view to `prefs.json` and the session mirror, so the next
    /// notification already follows it.
    pub(in crate::ui) fn set_notify_view(&mut self, view: NotifyView, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.notify_view = view;
            live.driver.save_notify_view();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.notify_view = view;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEMO_NOTIFICATION_PREVIEW_SETTINGS_ARE,
            )));
        }
        cx.notify();
    }

    pub(in crate::ui) fn notify_photo_previews(&self) -> bool {
        self.session()
            .is_none_or(|s| s.settings.notify_photo_previews)
    }

    /// Save "Show photo previews" to `prefs.json` and the session mirror.
    pub(in crate::ui) fn set_notify_photo_previews(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.notify_photo_previews = on;
            live.driver.save_notify_photo_previews();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.notify_photo_previews = on;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEMO_NOTIFICATION_PREVIEW_SETTINGS_ARE,
            )));
        }
        cx.notify();
    }

    /// "Show sender name" and "Show message text". As in tdesktop, the
    /// text needs the name: turning the name off turns the text off, and
    /// turning the text on turns the name on.
    pub(in crate::ui) fn notify_view_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.desktop_notifications_enabled() {
            return None;
        }
        let view = self.notify_view();
        let photos = self.notify_photo_previews();
        let row = |id: &'static str, label: &'static str, checked: bool, name: bool| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_sm().child(label))
                .child(
                    Switch::new(id)
                        .checked(checked)
                        .accessibility_label(label)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            let current = this.notify_view();
                            let next = current.toggled(name, on);
                            if next != current {
                                this.set_notify_view(next, cx);
                            }
                        })),
                )
        };
        Some(
            div()
                .id("notify-view-section")
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .child(row(
                    "notify-view-name",
                    tr(keys::Q_PREFS_SHOW_SENDER_NAME),
                    view.shows_name(),
                    true,
                ))
                .child(row(
                    "notify-view-text",
                    tr(keys::Q_PREFS_SHOW_MESSAGE_TEXT),
                    view.shows_text(),
                    false,
                ))
                .child(
                    // The photo is part of the message: hidden with the text.
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_sm().child(tr(keys::Q_PREFS_SHOW_PHOTO_PREVIEWS)))
                        .child(
                            Switch::new("notify-view-photos")
                                .checked(photos && view.shows_text())
                                .disabled(!view.shows_text())
                                .accessibility_label(tr(keys::Q_PREFS_SHOW_PHOTO_PREVIEWS))
                                .on_click(cx.listener(|this, &on, _, cx| {
                                    this.set_notify_photo_previews(on, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
