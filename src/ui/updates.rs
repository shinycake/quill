//! Release checks run off the UI thread. Nothing installs without the user's
//! click, unless they turned on "Install updates automatically" (then it only
//! stages, and applies on the next quit).
#![deny(clippy::indexing_slicing)]
use super::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::updater::{UpdateState, check_release};

impl QuillApp {
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.settings.update_state,
            UpdateState::Checking
                | UpdateState::Downloading(_)
                | UpdateState::Installing(_)
                | UpdateState::Staged(..)
        ) {
            return;
        }
        self.settings.update_state = UpdateState::Checking;
        self.settings.update_banner_dismissed = false;
        let beta = quill::advanced::update_channel::install_beta();
        let check = cx
            .background_executor()
            .spawn(async move { check_release(beta) });
        cx.spawn(async move |this, cx| {
            let state = check.await;
            let _ = this.update(cx, |this, cx| {
                // "Install beta versions" changed while this ran: the
                // answer is for the other channel, ask again.
                if quill::advanced::update_channel::install_beta() != beta {
                    this.settings.update_state = UpdateState::Idle;
                    this.check_for_updates(cx);
                    return;
                }
                this.settings.update_state = state;
                this.stage_if_opted_in(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// "Install updates automatically": a release the check just found is
    /// downloaded and staged without a click (`auto_update::staging_candidate`).
    pub(super) fn stage_if_opted_in(&mut self, cx: &mut Context<Self>) {
        let candidate = quill::advanced::auto_update::staging_candidate(
            &self.settings.update_state,
            quill::advanced::auto_update::install_automatically(),
            self.appearance.check_updates_on_launch,
            quill::update_install::self_update_available(),
        );
        if let Some(release) = candidate {
            self.stage_update(release, cx);
        }
    }

    /// Downloads, verifies and stages `release` in the background. The update
    /// is applied at the next start (`update_install::apply_on_start`) or when
    /// the user presses "Restart to update"; Quill never restarts itself.
    fn stage_update(&mut self, release: quill::updater::ReleaseInfo, cx: &mut Context<Self>) {
        self.settings.update_state = UpdateState::Downloading(release.clone());
        let download_release = release.clone();
        let download = cx
            .background_executor()
            .spawn(async move { quill::update_install::download_and_stage(&download_release) });
        cx.spawn(async move |this, cx| {
            let result = download.await;
            let _ = this.update(cx, |this, cx| {
                this.settings.update_state = match result {
                    Ok(plan) => UpdateState::Staged(release, plan),
                    Err(message) => UpdateState::DownloadFailed(release, message),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// "Restart to update": install the staged update and start the new
    /// version.
    fn restart_to_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Staged(release, plan) = &self.settings.update_state else {
            return;
        };
        let release = release.clone();
        match quill::update_install::launch_helper(plan) {
            Ok(()) => {
                self.settings.update_state = UpdateState::Installing(release);
                cx.quit();
            }
            Err(_) => {
                self.settings.update_state =
                    UpdateState::InstallFailed(release, tr(keys::Q_OPS_COULD_NOT_START_THE_UPDATE));
            }
        }
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
                                tr(keys::Q_OPS_COULD_NOT_START_THE_UPDATE),
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
                    .aria_label(tr(keys::Q_OPS_QUILL_UPDATES))
                    .font_semibold()
                    .child(tr(keys::Q_OPS_QUILL_UPDATES)),
            )
            .child(
                Switch::new("check-updates-on-launch")
                    .checked(self.appearance.check_updates_on_launch)
                    .accessibility_label(tr(keys::Q_OPS_CHECK_FOR_UPDATES_ON_LAUNCH))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_appearance(cx, |a| a.check_updates_on_launch = on)
                    })),
            )
            .child(
                div()
                    .text_xs()
                    .child(tr(keys::Q_OPS_CHECK_GITHUB_RELEASES_AUTOMATICALLY_ON)),
            )
            .child(
                div()
                    .id("quill-update-status")
                    .role(Role::Label)
                    .aria_label(self.settings.update_state.label())
                    .text_sm()
                    .child(self.settings.update_state.label()),
            )
            .children(self.auto_update_row(cx))
            .children(self.install_beta_row(cx))
            .child(
                Button::new("check-for-updates")
                    .label(tr(keys::Q_OPS_CHECK_FOR_UPDATES))
                    .disabled(matches!(
                        self.settings.update_state,
                        UpdateState::Checking
                            | UpdateState::Downloading(_)
                            | UpdateState::Installing(_)
                            | UpdateState::Staged(..)
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.check_for_updates(cx))),
            );
        if let UpdateState::Available(release)
        | UpdateState::DownloadFailed(release, _)
        | UpdateState::InstallFailed(release, _)
        | UpdateState::Staged(release, _)
        | UpdateState::Installed(release) = &self.settings.update_state
        {
            let url = release.url.clone();
            section = section
                .child(
                    div()
                        .id("quill-release-notes")
                        .role(Role::Label)
                        .aria_label(if release.notes.is_empty() {
                            tr(keys::Q_OPS_NO_RELEASE_NOTES_WERE_PROVIDED).to_string()
                        } else {
                            release.notes.clone()
                        })
                        .text_sm()
                        .child(if release.notes.is_empty() {
                            tr(keys::Q_OPS_NO_RELEASE_NOTES_WERE_PROVIDED).into()
                        } else {
                            release.notes.clone()
                        }),
                )
                .child(
                    Button::new("open-quill-release")
                        .label(tr(keys::Q_OPS_VIEW_RELEASE_ON_GITHUB))
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
                );
            if matches!(self.settings.update_state, UpdateState::Installed(_)) {
                section = section.child(
                    Button::new("acknowledge-update-changelog")
                        .label(tr(keys::Q_OPS_DISMISS_CHANGELOG))
                        .on_click(cx.listener(|this, _, _, cx| {
                            match quill::update_install::acknowledge_changelog() {
                                Ok(()) => this.settings.update_state = UpdateState::UpToDate,
                                Err(_) => this.connection.set_note(StatusNote::error(tr(
                                    keys::Q_OPS_COULD_NOT_SAVE_CHANGELOG_DISMISSAL,
                                ))),
                            }
                            cx.notify();
                        })),
                );
            } else if matches!(self.settings.update_state, UpdateState::Staged(..)) {
                section = section.child(
                    Button::new("restart-to-update")
                        .label(tr(keys::Q_OPS_RESTART_TO_UPDATE))
                        .on_click(cx.listener(|this, _, _, cx| this.restart_to_update(cx))),
                );
            } else if release.asset.is_some() && quill::update_install::can_install_here() {
                let release = release.clone();
                section = section.child(
                    Button::new("install-quill-update")
                        .label(tr(keys::Q_OPS_DOWNLOAD_INSTALL_AND_RESTART))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install_update(release.clone(), cx)
                        })),
                );
            } else {
                section = section.child(
                    div()
                        .text_xs()
                        .child(tr(keys::Q_OPS_INSTALL_THE_RELEASE_PACKAGE_FROM)),
                );
            }
        }
        section.into_any_element()
    }

    /// "About Quill": version, the unofficial-client and no-warranty notice,
    /// and the bundled open-source license notices.
    pub(super) fn about_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let version = tr_with(
            keys::Q_OPS_QUILL,
            &[("quill_version_app", (quill::version::APP))],
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("about-quill-heading")
                    .role(Role::Heading)
                    .aria_label(tr(keys::Q_OPS_ABOUT_QUILL))
                    .font_semibold()
                    .child(tr(keys::Q_OPS_ABOUT_QUILL)),
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
                    .label(tr(keys::Q_OPS_OPEN_SOURCE_LICENSES))
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
            UpdateState::Available(_) | UpdateState::Staged(..) | UpdateState::Installed(_)
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
                            .label(tr(keys::Q_OPS_RELEASE_NOTES))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_appearance();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("dismiss-update-banner")
                            .label(tr(keys::Q_OPS_DISMISS))
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
