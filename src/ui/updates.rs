//! Release checks run off the UI thread. One button carries the update
//! through its steps (`UpdateControl`): Check for updates, Update Quill,
//! the download, Restart to update. It sits in Settings and, while there is
//! an update to act on, in the title bar beside search (`update_pill`). Quill checks at launch and every few hours, and downloads a
//! new release without a click; a downloaded update also installs the next
//! time Quill starts (`update_install::apply_on_start`). Quill never
//! restarts itself.
#![deny(clippy::indexing_slicing)]
use super::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::platform::OsOpen;
use quill::updater::{DownloadProgress, UpdateControl, UpdateState, check_latest_release};
use std::time::Duration;

/// How often a running Quill looks for a new release.
const RECHECK_EVERY: Duration = Duration::from_secs(4 * 60 * 60);

impl QuillApp {
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.update_control(),
            UpdateControl::Check { busy: false }
                | UpdateControl::Install(_)
                | UpdateControl::OpenRelease(_)
        ) {
            return;
        }
        self.settings.update_state = UpdateState::Checking;
        self.settings.update_banner_dismissed = false;
        let check = cx
            .background_executor()
            .spawn(async move { check_latest_release() });
        cx.spawn(async move |this, cx| {
            let state = check.await;
            let _ = this.update(cx, |this, cx| this.apply_check_result(state, cx));
        })
        .detach();
        cx.notify();
    }

    /// Checks at launch, then every few hours while Quill runs (tdesktop's
    /// updater rechecks on a timer too). There is no launch check when the
    /// session opens on "Updated to Quill X"; a check only starts when
    /// nothing is in flight (`check_for_updates`).
    pub(super) fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        if self.settings.update_state == UpdateState::Idle {
            self.check_for_updates(cx);
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RECHECK_EVERY).await;
                if this
                    .update(cx, |this, cx| this.check_for_updates(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// The answer of a release check: shown, and a newer release this copy
    /// can install starts downloading at once.
    pub(super) fn apply_check_result(&mut self, state: UpdateState, cx: &mut Context<Self>) {
        self.settings.update_state = state;
        self.auto_download_update(cx);
        cx.notify();
    }

    /// Starts the found release's download, unless the disk bar says
    /// automatic downloads wait: then the disk's return to healthy starts it.
    pub(super) fn auto_download_update(&mut self, cx: &mut Context<Self>) {
        if self.disk.holds_auto_downloads() {
            return;
        }
        if let Some(release) = UpdateControl::auto_download(
            &self.settings.update_state,
            quill::update_install::self_update_availability(),
        ) {
            self.download_update(release, cx);
        }
    }

    /// The update button for the current state (`UpdateControl::of`).
    pub(super) fn update_control(&self) -> UpdateControl {
        UpdateControl::of(
            &self.settings.update_state,
            quill::update_install::self_update_availability(),
        )
    }

    /// The update button was pressed: the step it shows.
    pub(super) fn press_update_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.update_control() {
            UpdateControl::Check { busy: false } => self.check_for_updates(cx),
            UpdateControl::Install(release) => self.download_update(release, cx),
            UpdateControl::Restart => {
                let app = cx.entity().downgrade();
                let restart = move |_: &mut Window, cx: &mut App| {
                    let _ = app.update(cx, |this, cx| this.restart_to_update(cx));
                };
                if !self.exit_held_by_export(restart, window, cx) {
                    self.restart_to_update(cx);
                }
            }
            UpdateControl::OpenRelease(url) => self.open_link(&url, cx),
            UpdateControl::Check { busy: true }
            | UpdateControl::Downloading { .. }
            | UpdateControl::Restarting => {}
        }
    }

    /// Downloads, verifies and stages `release` in the background; the
    /// button then offers "Restart to update", and the next start applies it
    /// too (`update_install::apply_on_start`). Quill never restarts itself.
    fn download_update(&mut self, release: quill::updater::ReleaseInfo, cx: &mut Context<Self>) {
        if matches!(
            self.settings.update_state,
            UpdateState::Downloading(..) | UpdateState::Installing(_) | UpdateState::Staged(..)
        ) {
            return;
        }
        let progress = DownloadProgress::default();
        self.settings.update_state = UpdateState::Downloading(release.clone(), progress.clone());
        let download_release = release.clone();
        let download_progress = progress.clone();
        let download = cx.background_executor().spawn(async move {
            quill::update_install::download_and_stage(&download_release, &download_progress)
        });
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
        // The progress lives in the download; redraw while it runs.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let running = this.update(cx, |this, cx| {
                    let running = matches!(
                        &this.settings.update_state,
                        UpdateState::Downloading(_, current) if *current == progress
                    );
                    if running {
                        cx.notify();
                    }
                    running
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
            }
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
        match quill::update_install::hand_off(plan) {
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

    /// The text of the update button.
    pub(super) fn update_control_label(control: &UpdateControl) -> SharedString {
        match control {
            UpdateControl::Check { .. } => tr(keys::Q_OPS_CHECK_FOR_UPDATES).into(),
            UpdateControl::Install(_) => tr(keys::Q_OPS_UPDATE_QUILL).into(),
            UpdateControl::Downloading { progress, total } => tr_with(
                keys::Q_OPS_DOWNLOADING_UPDATE,
                &[(
                    "progress",
                    quill::updater::progress_text(progress.bytes(), *total).as_str(),
                )],
            )
            .into(),
            UpdateControl::Restart => tr(keys::Q_OPS_RESTART_TO_UPDATE).into(),
            UpdateControl::Restarting => tr(keys::Q_OPS_INSTALLING_UPDATE).into(),
            UpdateControl::OpenRelease(_) => tr(keys::Q_OPS_VIEW_RELEASE_ON_GITHUB).into(),
        }
    }

    /// Settings' update button.
    fn update_control_button(
        &self,
        id: &'static str,
        control: &UpdateControl,
        cx: &mut Context<Self>,
    ) -> Button {
        let label = Self::update_control_label(control);
        let button = Button::new(id)
            .label(label.clone())
            .accessibility_label(label)
            .disabled(matches!(
                control,
                UpdateControl::Check { busy: true }
                    | UpdateControl::Downloading { .. }
                    | UpdateControl::Restarting
            ))
            .on_click(cx.listener(|this, _, window, cx| {
                this.press_update_control(window, cx);
            }));
        // The step that moves the update forward is the primary action.
        if matches!(
            control,
            UpdateControl::Install(_) | UpdateControl::Restart | UpdateControl::OpenRelease(_)
        ) {
            button.primary()
        } else {
            button
        }
    }

    /// The download's progress bar under the button.
    fn update_progress_bar(id: &'static str, control: &UpdateControl) -> Option<AnyElement> {
        control.fraction().map(|fraction| {
            ProgressBar::new(id)
                .value(fraction * 100.)
                .into_any_element()
        })
    }

    pub(in crate::ui) fn update_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let control = self.update_control();
        let status = self.settings.update_state.label();
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
                div()
                    .id("quill-update-status")
                    .role(Role::Label)
                    .aria_label(status.clone())
                    .text_sm()
                    .child(status),
            )
            .children(Self::update_progress_bar("quill-update-progress", &control))
            .child(
                div()
                    .flex()
                    .gap_2()
                    // While it downloads the status line above says so.
                    .when(
                        !matches!(
                            control,
                            UpdateControl::Downloading { .. } | UpdateControl::Restarting
                        ),
                        |row| {
                            row.child(self.update_control_button(
                                "quill-update-button",
                                &control,
                                cx,
                            ))
                        },
                    )
                    .children(self.release_page_button(&control, cx)),
            )
            .children(self.update_release_notes(cx));
        if matches!(self.settings.update_state, UpdateState::Installed(_)) {
            section = section.child(
                div().flex().child(
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
                ),
            );
        }
        section.into_any_element()
    }

    /// The release the state is about, if any.
    fn known_release(&self) -> Option<&quill::updater::ReleaseInfo> {
        match &self.settings.update_state {
            UpdateState::Available(release)
            | UpdateState::Downloading(release, _)
            | UpdateState::DownloadFailed(release, _)
            | UpdateState::InstallFailed(release, _)
            | UpdateState::Staged(release, _)
            | UpdateState::Installing(release)
            | UpdateState::Installed(release) => Some(release),
            UpdateState::Idle
            | UpdateState::Checking
            | UpdateState::UpToDate
            | UpdateState::NoRelease
            | UpdateState::Failed(_) => None,
        }
    }

    /// "View release on GitHub" beside the button, unless the button is
    /// that link already.
    fn release_page_button(
        &self,
        control: &UpdateControl,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if matches!(control, UpdateControl::OpenRelease(_)) {
            return None;
        }
        let url = self.known_release()?.url.clone();
        Some(
            Button::new("open-quill-release")
                .label(tr(keys::Q_OPS_VIEW_RELEASE_ON_GITHUB))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| this.open_link(&url, cx)))
                .into_any_element(),
        )
    }

    /// The release's notes, and why this copy cannot install it itself.
    fn update_release_notes(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let release = self.known_release()?;
        let notes: SharedString = if release.notes.is_empty() {
            tr(keys::Q_OPS_NO_RELEASE_NOTES_WERE_PROVIDED).into()
        } else {
            release.notes.clone().into()
        };
        // A newer release the button cannot install: the reason.
        let reason = matches!(
            self.settings.update_state,
            UpdateState::Available(_) | UpdateState::DownloadFailed(..)
        )
        .then(quill::update_install::self_update_availability)
        .and_then(Result::err);
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .id("quill-release-notes")
                        .role(Role::Label)
                        .aria_label(notes.clone())
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(notes),
                )
                .children(reason.map(|why| {
                    div()
                        .id("quill-update-unavailable")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(why.hint()))
                }))
                .into_any_element(),
        )
    }

    /// "About Quill": version, the unofficial-client and no-warranty notice,
    /// and the bundled open-source license notices.
    pub(in crate::ui) fn about_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
                Button::new("about-whats-new")
                    .label(tr(keys::Q_PREFS_WHATS_NEW))
                    .on_click(cx.listener(|this, _, _, cx| this.open_whats_new(cx))),
            )
            .child(
                Button::new("open-source-licenses")
                    .label(tr(keys::Q_OPS_OPEN_SOURCE_LICENSES))
                    .on_click(cx.listener(|this, _, _, cx| this.open_license_notices(cx))),
            )
            .into_any_element()
    }

    fn open_license_notices(&mut self, cx: &mut Context<Self>) {
        let open = quill::about::bundled_notices().map_or(OsOpen::Answered(false), |path| {
            quill::platform::open_local_file(&path)
        });
        self.published_notices_unless_opened(open, cx);
    }

    pub(super) fn published_notices_unless_opened(&mut self, open: OsOpen, cx: &mut Context<Self>) {
        self.when_os_answers(open, cx, |this, opened, cx| {
            if !opened {
                this.open_link(quill::about::NOTICES_URL, cx);
            }
        });
    }

    /// "Updated to Quill X" after an update, with its notes; the update
    /// itself is offered in the title bar.
    pub(super) fn update_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        if !matches!(self.settings.update_state, UpdateState::Installed(_)) {
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
                            .on_click(cx.listener(|this, _, _, cx| this.open_whats_new(cx))),
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
