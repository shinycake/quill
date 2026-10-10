//! Release checks run off the UI thread and never install silently.
use super::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::updater::{UpdateState, check_latest_release};

impl QuillApp {
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.settings.update_state,
            UpdateState::Checking | UpdateState::Downloading(_) | UpdateState::Installing(_)
        ) {
            return;
        }
        self.settings.update_state = UpdateState::Checking;
        self.settings.update_banner_dismissed = false;
        let check = cx
            .background_executor()
            .spawn(async { check_latest_release() });
        cx.spawn(async move |this, cx| {
            let state = check.await;
            let _ = this.update(cx, |this, cx| {
                this.settings.update_state = state;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn install_update(&mut self, release: quill::updater::ReleaseInfo, cx: &mut Context<Self>) {
        if matches!(
            self.settings.update_state,
            UpdateState::Downloading(_) | UpdateState::Installing(_)
        ) {
            return;
        }
        self.settings.update_state = UpdateState::Downloading(release.clone());
        let download_release = release.clone();
        let download = cx
            .background_executor()
            .spawn(async move { quill::update_install::download_and_stage(&download_release) });
        cx.spawn(async move |this, cx| {
            let result = download.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(plan) => match quill::update_install::launch_helper(&plan) {
                        Ok(()) => {
                            this.settings.update_state = UpdateState::Installing(release);
                            cx.quit();
                        }
                        Err(_) => {
                            this.settings.update_state = UpdateState::InstallFailed(
                                release,
                                "Could not start the update installer. Retry the update.",
                            );
                        }
                    },
                    Err(message) => {
                        this.settings.update_state = UpdateState::DownloadFailed(release, message)
                    }
                }
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
            .child(
                div()
                    .id("quill-updates-heading")
                    .role(Role::Heading)
                    .aria_label("Quill updates")
                    .font_semibold()
                    .child("Quill updates"),
            )
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
            .child(
                div()
                    .id("quill-update-status")
                    .role(Role::Label)
                    .aria_label(self.settings.update_state.label())
                    .text_sm()
                    .child(self.settings.update_state.label()),
            )
            .child(
                Button::new("check-for-updates")
                    .label("Check for updates")
                    .disabled(matches!(
                        self.settings.update_state,
                        UpdateState::Checking
                            | UpdateState::Downloading(_)
                            | UpdateState::Installing(_)
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.check_for_updates(cx))),
            );
        if let UpdateState::Available(release)
        | UpdateState::DownloadFailed(release, _)
        | UpdateState::InstallFailed(release, _)
        | UpdateState::Installed(release) = &self.settings.update_state
        {
            let url = release.url.clone();
            section = section
                .child(
                    div()
                        .id("quill-release-notes")
                        .role(Role::Label)
                        .aria_label(if release.notes.is_empty() {
                            "No release notes were provided.".to_string()
                        } else {
                            release.notes.clone()
                        })
                        .text_sm()
                        .child(if release.notes.is_empty() {
                            "No release notes were provided.".into()
                        } else {
                            release.notes.clone()
                        }),
                )
                .child(
                    Button::new("open-quill-release")
                        .label("View release on GitHub")
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
                );
            if matches!(self.settings.update_state, UpdateState::Installed(_)) {
                section = section.child(
                    Button::new("acknowledge-update-changelog")
                        .label("Dismiss changelog")
                        .on_click(cx.listener(|this, _, _, cx| {
                            match quill::update_install::acknowledge_changelog() {
                                Ok(()) => this.settings.update_state = UpdateState::UpToDate,
                                Err(_) => {
                                    this.connection.status_note =
                                        "Could not save changelog dismissal. Retry.".into()
                                }
                            }
                            cx.notify();
                        })),
                );
            } else if release.asset.is_some() && quill::update_install::can_install_here() {
                let release = release.clone();
                section = section.child(
                    Button::new("install-quill-update")
                        .label("Download, install and restart")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install_update(release.clone(), cx)
                        })),
                );
            } else {
                section = section.child(
                    div()
                        .text_xs()
                        .child("Install the release package from GitHub for this installation."),
                );
            }
        }
        section.into_any_element()
    }

    /// "About Quill": version, the unofficial-client and no-warranty notice,
    /// and the bundled open-source license notices.
    pub(super) fn about_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let version = format!("Quill {}", quill::version::APP);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("about-quill-heading")
                    .role(Role::Heading)
                    .aria_label("About Quill")
                    .font_semibold()
                    .child("About Quill"),
            )
            .child(
                div()
                    .id("about-quill-version")
                    .role(Role::Label)
                    .aria_label(version.clone())
                    .text_sm()
                    .child(version),
            )
            .child(
                div()
                    .id("about-quill-disclaimer")
                    .role(Role::Label)
                    .aria_label(quill::about::DISCLAIMER)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(quill::about::DISCLAIMER),
            )
            .child(
                Button::new("open-source-licenses")
                    .label("Open-source licenses")
                    .on_click(cx.listener(|_, _, _, cx| {
                        let opened = quill::about::bundled_notices()
                            .is_some_and(|path| quill::platform::open_local_file(&path));
                        if !opened {
                            cx.open_url(quill::about::NOTICES_URL);
                        }
                    })),
            )
            .into_any_element()
    }

    pub(super) fn update_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        if !matches!(
            self.settings.update_state,
            UpdateState::Available(_) | UpdateState::Installed(_)
        ) {
            return div().into_any_element();
        }
        let label = self.settings.update_state.label();
        div()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .py_1()
            .child(
                div()
                    .id("quill-update-banner-message")
                    .role(Role::Label)
                    .aria_label(label.clone())
                    .text_sm()
                    .child(label),
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
                                this.settings.appearance_open = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("dismiss-update-banner")
                            .label("Dismiss")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.update_banner_dismissed = true;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
