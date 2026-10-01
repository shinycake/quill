//! Release checks run off the UI thread and never install silently.
use super::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::updater::{UpdateState, check_latest_release};

impl QuillApp {
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.update_state == UpdateState::Checking {
            return;
        }
        self.update_state = UpdateState::Checking;
        self.update_banner_dismissed = false;
        let check = cx
            .background_executor()
            .spawn(async { check_latest_release() });
        cx.spawn(async move |this, cx| {
            let state = check.await;
            let _ = this.update(cx, |this, cx| {
                this.update_state = state;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn update_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().font_semibold().child("Quill updates"))
            .child(
                Switch::new("check-updates-on-launch")
                    .checked(self.appearance.check_updates_on_launch)
                    .accessibility_label("Check for updates on launch")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_appearance(cx, |a| a.check_updates_on_launch = on)
                    })),
            )
            .child(
                div()
                    .text_xs()
                    .child("Check GitHub Releases automatically on launch."),
            )
            .child(div().text_sm().child(self.update_state.label()))
            .child(
                Button::new("check-for-updates")
                    .label("Check for updates")
                    .disabled(self.update_state == UpdateState::Checking)
                    .on_click(cx.listener(|this, _, _, cx| this.check_for_updates(cx))),
            );
        if let UpdateState::Available(release) = &self.update_state {
            let url = release.url.clone();
            section = section
                .child(div().text_sm().child(if release.notes.is_empty() {
                    "No release notes were provided.".into()
                } else {
                    release.notes.clone()
                }))
                .child(
                    Button::new("open-quill-release")
                        .label("View release on GitHub")
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
                );
        }
        section.into_any_element()
    }

    pub(super) fn update_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let UpdateState::Available(release) = &self.update_state else {
            return div().into_any_element();
        };
        let version = release.version.clone();
        div()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .py_1()
            .child(
                div()
                    .text_sm()
                    .child(format!("Quill {version} is available")),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("show-release-notes")
                            .label("Release notes")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.appearance_open = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("dismiss-update-banner")
                            .label("Dismiss")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.update_banner_dismissed = true;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
