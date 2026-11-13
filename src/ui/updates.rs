//! Release checks run off the UI thread. One button carries the update
//! through its steps (`UpdateControl`): Check for updates, Update Quill,
//! the download, Restart to update. It sits in Settings and, while there is
//! an update to act on, in the title bar beside search (`update_pill`). Quill checks at launch and every few hours, and downloads a
//! new release without a click; a downloaded update also installs the next
//! time Quill starts (`update_install::apply_on_start`). Quill never
//! restarts itself.
#![deny(clippy::indexing_slicing)]
use super::QuillApp;
use crate::ui::settings_window::rows::{group, icon_row};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::updater::{
    ChangelogBanner, DownloadProgress, Job, UpdateControl, UpdateState, check_latest_release,
};
use std::time::Duration;

/// How long Quill waits for its own quit after handing the update over.
const RESTART_GIVE_UP: Duration = Duration::from_secs(30);

/// How often a running Quill looks for a new release.
const RECHECK_EVERY: Duration = Duration::from_secs(4 * 60 * 60);

impl QuillApp {
    /// "Updated to Quill X" stays until the user has seen the changelog
    /// (`acknowledge_changelog`): a check would replace it without that, and
    /// the banner would come back at the next launch. So no check, from the
    /// button or the timer, starts over it.
    pub(super) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        self.check_for_updates_keeping(None, cx);
    }

    /// A check that, when it fails, ends in `DownloadFailed(kept, message)`
    /// instead of `Failed`: the kept release stays on offer.
    fn check_for_updates_keeping(
        &mut self,
        kept: Option<quill::updater::ReleaseInfo>,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            self.update_control(),
            UpdateControl::Check { busy: false }
                | UpdateControl::Install(_)
                | UpdateControl::OpenRelease(_)
        ) {
            return;
        }
        let check = cx
            .background_executor()
            .spawn(async move { check_latest_release() });
        // The stage holds the task: replacing the stage cancels the check.
        let task = cx.spawn(async move |this, cx| {
            let state = match (check.await, kept) {
                (UpdateState::Failed(message), Some(release)) => {
                    UpdateState::DownloadFailed(release, message)
                }
                (state, _) => state,
            };
            let _ = this.update(cx, |this, cx| this.apply_check_result(state, cx));
        });
        self.preferences.update_state = UpdateState::Checking(Job::hold(task));
        cx.notify();
    }

    /// The timer's check, which starts only where it has nothing to lose: an
    /// update already found, held (the disk bar), failed or on its way is
    /// not replaced by a check that may fail offline (`Checking` keeps no
    /// offer to return to). The button's check can still start over those.
    /// A download that failed is checked again by the timer, keeping its
    /// release: a check that fails (offline) puts `DownloadFailed` back with
    /// that release, and one that finds a release (a bad checksum fixed, an
    /// asset taken down) shows it. When the kept release is still the
    /// latest, the found update's own start downloads it again
    /// (`auto_download_update`).
    pub(super) fn recheck_for_updates(&mut self, cx: &mut Context<Self>) {
        match &self.preferences.update_state {
            UpdateState::Idle
            | UpdateState::UpToDate
            | UpdateState::NoRelease
            | UpdateState::Failed(_) => self.check_for_updates(cx),
            UpdateState::DownloadFailed(release, _) => {
                let kept = release.clone();
                self.check_for_updates_keeping(Some(kept), cx);
            }
            _ => {}
        }
    }

    /// Checks at launch, then every few hours while Quill runs (tdesktop's
    /// updater rechecks on a timer too). There is no launch check when the
    /// session opens on "Updated to Quill X"; a check only starts when
    /// nothing is in flight (`check_for_updates`).
    pub(super) fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        if self.preferences.update_state == UpdateState::Idle {
            self.check_for_updates(cx);
        }
        self.arm_update_recheck(cx);
    }

    /// The next recheck, one timer at a time: the app holds its task, so
    /// replacing it (each recheck arms the next) or dropping the app ends
    /// it.
    fn arm_update_recheck(&mut self, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(RECHECK_EVERY);
        self.preferences.update_recheck = Some(cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                this.recheck_for_updates(cx);
                this.arm_update_recheck(cx);
            });
        }));
    }

    /// The answer of a release check: shown, and a newer release this copy
    /// can install starts downloading at once.
    pub(super) fn apply_check_result(&mut self, state: UpdateState, cx: &mut Context<Self>) {
        self.preferences.update_state = state;
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
            &self.preferences.update_state,
            quill::update_install::self_update_availability(),
        ) {
            self.download_update(release, cx);
        }
    }

    /// The update download ended. One the disk held goes back to
    /// available, and starts again at once when the disk is healthy by now,
    /// else at the disk's return to healthy, from the bytes it kept
    /// (`auto_download_update`).
    pub(super) fn finish_update_download(
        &mut self,
        release: quill::updater::ReleaseInfo,
        progress: &DownloadProgress,
        result: Result<std::path::PathBuf, &'static str>,
        cx: &mut Context<Self>,
    ) {
        let held = result.is_err() && progress.is_held();
        self.preferences.update_state = match result {
            Ok(plan) => UpdateState::Staged(release, plan),
            Err(_) if held => UpdateState::Available(release),
            Err(message) => UpdateState::DownloadFailed(release, message),
        };
        // The disk may have recovered while the download was still noticing
        // the hold: its return to healthy found the download running and
        // started nothing, so this is where it starts again.
        if held {
            self.auto_download_update(cx);
        }
        cx.notify();
    }

    /// The disk went low or full: an update download already running stops,
    /// as one about to start waits (`auto_download_update`).
    pub(super) fn hold_update_download(&mut self) {
        if let UpdateState::Downloading(_, progress, _) = &self.preferences.update_state {
            progress.hold();
        }
    }

    /// The update button for the current state (`UpdateControl::of`).
    pub(super) fn update_control(&self) -> UpdateControl {
        UpdateControl::of(
            &self.preferences.update_state,
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
            | UpdateControl::ChangelogPending
            | UpdateControl::Progress { .. }
            | UpdateControl::Restarting => {}
        }
    }

    /// Downloads, verifies and stages `release` in the background; the
    /// button then offers "Restart to update", and the next start applies it
    /// too (`update_install::apply_on_start`). Quill never restarts itself.
    fn download_update(&mut self, release: quill::updater::ReleaseInfo, cx: &mut Context<Self>) {
        if matches!(
            self.preferences.update_state,
            UpdateState::Downloading(..) | UpdateState::Installing(..) | UpdateState::Staged(..)
        ) {
            return;
        }
        let progress = DownloadProgress::default();
        let download_release = release.clone();
        let download_progress = progress.clone();
        let download = cx.background_executor().spawn(async move {
            quill::update_install::download_and_stage(&download_release, &download_progress)
        });
        let held = progress.clone();
        let stage_release = release.clone();
        let finish = cx.spawn(async move |this, cx| {
            let result = download.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_update_download(release, &held, result, cx);
            });
        });
        self.preferences.update_state =
            UpdateState::Downloading(stage_release, progress, Job::hold(finish));
        cx.notify();
    }

    /// "Restart to update": install the staged update and start the new
    /// version.
    fn restart_to_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Staged(release, plan) = &self.preferences.update_state else {
            return;
        };
        let release = release.clone();
        match quill::update_install::hand_off(plan) {
            Ok(()) => {
                // A quit that never lands must not leave the button hidden
                // for good: the watch ends the stage in InstallFailed.
                let watch = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(RESTART_GIVE_UP).await;
                    let _ = this.update(cx, |this, cx| this.restart_did_not_land(cx));
                });
                self.preferences.update_state = UpdateState::Installing(release, Job::hold(watch));
                cx.quit();
            }
            Err(_) => {
                self.preferences.update_state =
                    UpdateState::InstallFailed(release, tr(keys::Q_OPS_COULD_NOT_START_THE_UPDATE));
            }
        }
        cx.notify();
    }

    /// The quit after the hand-off did not end the process in time.
    pub(super) fn restart_did_not_land(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Installing(release, _) = &self.preferences.update_state else {
            return;
        };
        self.preferences.update_state =
            UpdateState::InstallFailed(release.clone(), tr(keys::Q_OPS_COULD_NOT_START_THE_UPDATE));
        cx.notify();
    }

    /// The text of the update button.
    pub(super) fn update_control_label(control: &UpdateControl) -> SharedString {
        match control {
            UpdateControl::Check { .. } | UpdateControl::ChangelogPending => {
                tr(keys::Q_OPS_CHECK_FOR_UPDATES).into()
            }
            UpdateControl::Install(_) => tr(keys::Q_OPS_UPDATE_QUILL).into(),
            UpdateControl::Progress { progress, total } => tr_with(
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
                    | UpdateControl::ChangelogPending
                    | UpdateControl::Progress { .. }
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
        let status = self.preferences.update_state.label();
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
                self.setting_stand_in(
                    self.known_release().is_none(),
                    &["system.release_notes"],
                    self.setting_spot(
                        &["system.check_updates"],
                        div()
                            .flex()
                            .gap_2()
                            // While it downloads the status line above says so.
                            .when(
                                !matches!(
                                    control,
                                    UpdateControl::Progress { .. } | UpdateControl::Restarting
                                ),
                                |row| {
                                    row.child(self.update_control_button(
                                        "quill-update-button",
                                        &control,
                                        cx,
                                    ))
                                },
                            )
                            .children(self.release_page_button(&control, cx))
                            .into_any_element(),
                        cx,
                    ),
                    cx,
                ),
            )
            .children(
                self.update_release_notes(cx)
                    .map(|notes| self.setting_spot(&["system.release_notes"], notes, cx)),
            );
        if matches!(self.preferences.update_state, UpdateState::Installed(..)) {
            section = section.child(
                div().flex().child(
                    Button::new("acknowledge-update-changelog")
                        .label(tr(keys::Q_OPS_DISMISS_CHANGELOG))
                        .on_click(cx.listener(|this, _, _, cx| {
                            match quill::update_install::acknowledge_changelog() {
                                Ok(()) => this.preferences.update_state = UpdateState::UpToDate,
                                Err(_) => this.set_note(StatusNote::error(tr(
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
        match &self.preferences.update_state {
            UpdateState::Available(release)
            | UpdateState::Downloading(release, ..)
            | UpdateState::DownloadFailed(release, _)
            | UpdateState::InstallFailed(release, _)
            | UpdateState::Staged(release, _)
            | UpdateState::Installing(release, _)
            | UpdateState::Installed(release, _) => Some(release),
            UpdateState::Idle
            | UpdateState::Checking(_)
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
        let notes = if release.notes.is_empty() {
            div()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_OPS_NO_RELEASE_NOTES_WERE_PROVIDED))
                .into_any_element()
        } else {
            super::whats_new::release_notes_markdown(
                "quill-release-notes-text",
                release.notes.clone(),
            )
        };
        // A newer release the button cannot install: the reason.
        let reason = matches!(
            self.preferences.update_state,
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
                        .debug_selector(|| "quill-release-notes".into())
                        .text_sm()
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

    /// "About Quill": the app icon and version, the unofficial-client and
    /// no-warranty notice, then what's new, sharing, the source and the
    /// bundled open-source license notices.
    pub(in crate::ui) fn about_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let version = tr_with(
            keys::Q_OPS_QUILL,
            &[("quill_version_app", (quill::version::APP))],
        );
        let muted = cx.theme().muted_foreground;
        let hero = div()
            .id("about-quill-heading")
            .role(Role::Group)
            .aria_label(tr(keys::Q_OPS_ABOUT_QUILL))
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .pt_2()
            .pb_1()
            .child(img(super::first_run_ui::logo()).size(px(72.)).mb_2())
            .child(
                div()
                    .id("about-quill-version")
                    .role(Role::Label)
                    .aria_label(version.clone())
                    .text_lg()
                    .font_semibold()
                    .child(version),
            )
            .child(
                div()
                    .id("about-quill-disclaimer")
                    .role(Role::Label)
                    .aria_label(quill::about::DISCLAIMER)
                    .max_w(px(460.))
                    .mt_2()
                    .text_xs()
                    .text_center()
                    .text_color(muted)
                    .child(quill::about::DISCLAIMER),
            );
        let hero = self.setting_spot(&["about.version", "about.about_quill"], hero, cx);
        let rows = [
            self.setting_spot(
                &["about.whats_new"],
                icon_row(
                    "about-whats-new",
                    IconName::Sparkles,
                    tr(keys::Q_PREFS_WHATS_NEW),
                    IconName::ChevronRight,
                    cx.listener(|this, _, _, cx| this.open_whats_new(cx)),
                    cx,
                ),
                cx,
            ),
            self.setting_spot(
                &["about.share"],
                icon_row(
                    "about-share-quill",
                    IconName::Share2,
                    tr(keys::Q_SETTINGS_ABOUT_SHARE_QUILL),
                    IconName::Copy,
                    cx.listener(|this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            quill::about::WEBSITE_URL.to_string(),
                        ));
                        this.set_status_note(
                            StatusNote::info(tr(keys::Q_SETTINGS_ABOUT_LINK_COPIED)),
                            cx,
                        );
                    }),
                    cx,
                ),
                cx,
            ),
            self.setting_spot(
                &["about.source_code"],
                icon_row(
                    "about-source-code",
                    IconName::Github,
                    tr(keys::Q_SETTINGS_ABOUT_SOURCE_CODE),
                    IconName::ExternalLink,
                    cx.listener(|this, _, _, cx| this.open_link(quill::about::SOURCE_URL, cx)),
                    cx,
                ),
                cx,
            ),
            self.setting_spot(
                &["about.licenses"],
                icon_row(
                    "open-source-licenses",
                    IconName::FileText,
                    tr(keys::Q_OPS_OPEN_SOURCE_LICENSES),
                    IconName::ExternalLink,
                    cx.listener(|this, _, _, cx| this.open_license_notices(cx)),
                    cx,
                ),
                cx,
            ),
        ];
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(hero)
            .child(group(None, rows))
            .into_any_element()
    }

    fn open_license_notices(&mut self, cx: &mut Context<Self>) {
        match quill::about::bundled_notices() {
            Some(path) => self.open_file_then(&path, cx, published_notices_unless_opened),
            None => self.open_link(quill::about::NOTICES_URL, cx),
        }
    }

    /// "Updated to Quill X" after an update, with its notes; the update
    /// itself is offered in the title bar.
    pub(super) fn update_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        if !matches!(self.preferences.update_state, UpdateState::Installed(..)) {
            return div().into_any_element();
        }
        let label = self.preferences.update_state.label();
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
                                if let UpdateState::Installed(_, banner) =
                                    &mut this.preferences.update_state
                                {
                                    *banner = ChangelogBanner::Dismissed;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}

/// The license notices did not open from the app bundle: the published
/// copy opens in the browser instead.
pub(super) fn published_notices_unless_opened(
    this: &mut QuillApp,
    opened: bool,
    cx: &mut Context<QuillApp>,
) {
    if !opened {
        this.open_link(quill::about::NOTICES_URL, cx);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "updates_tests.rs"]
mod tests;
