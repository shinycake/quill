//! The update in the title bar, left of search, while there is one to act
//! on (tdesktop puts "Update Telegram" under the chat list; a pill here
//! takes no row). One pill of one size walks the whole update: Update
//! Quill, then a track that an inset capsule fills with the download under
//! its percentage, then the percentage fades into Restart to update.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::updater::UpdateControl;
use std::time::{Duration, Instant};

/// How long the fill eases to a new download share.
const FILL: Duration = Duration::from_millis(300);
/// How long the percentage takes to fade into Restart to update.
const SWAP: Duration = Duration::from_millis(350);
/// The kit's extra-small button: 20 px high, 4 px side padding. Centered
/// in the 34 px title bar the capsule sits 7 px from the top with a 10 px
/// radius, concentric with a 17 px corner (the macOS window's is 16 px).
const HEIGHT: Pixels = px(20.);
/// The download capsule's inset from the pill's edge; its radius is the
/// pill's less this, so the two are concentric.
const INSET: Pixels = px(2.);

/// The pill's motion: where the fill eases from and to, and when the
/// finished download started fading into the restart label. The control
/// itself stays the source of truth; this only remembers the last target
/// so a change can ease instead of jump.
#[derive(Default)]
pub(super) struct UpdatePill {
    from: f32,
    to: f32,
    since: Option<Instant>,
    downloading: bool,
    /// `Some` once the update is ready: when the swap began, `None` inside
    /// for one that was ready without a download to watch (shown at once).
    ready: Option<Option<Instant>>,
}

impl UpdatePill {
    /// Follows `control` at `now`: a new download share eases the fill
    /// there, and a download that finishes fades into the restart label.
    /// Any other change shows at once.
    pub(super) fn follow(&mut self, control: &UpdateControl, now: Instant) {
        let target = control.fraction().unwrap_or(1.0);
        let downloading = matches!(control, UpdateControl::Downloading { .. });
        let ready = matches!(control, UpdateControl::Restart | UpdateControl::Restarting);
        let eases = self.downloading && (downloading || ready);
        if !eases {
            self.from = target;
            self.to = target;
            self.since = None;
        } else if target != self.to {
            self.from = self.fill(now);
            self.to = target;
            self.since = Some(now);
        }
        self.ready = match (ready, self.ready) {
            (false, _) => None,
            (true, Some(started)) => Some(started),
            (true, None) => Some(self.downloading.then_some(now)),
        };
        self.downloading = downloading;
    }

    /// The share of the pill filled at `now`, 0 to 1.
    pub(super) fn fill(&self, now: Instant) -> f32 {
        let Some(since) = self.since else {
            return self.to;
        };
        let t = progress(now.saturating_duration_since(since), FILL);
        self.from + (self.to - self.from) * super::sidebar_reveal::ease_out_cubic(t)
    }

    /// How far the percentage has faded into the restart label, 0 to 1.
    pub(super) fn ready_mix(&self, now: Instant) -> f32 {
        match self.ready {
            None => 0.0,
            Some(None) => 1.0,
            Some(Some(since)) => progress(now.saturating_duration_since(since), SWAP),
        }
    }

    pub(super) fn animating(&self, now: Instant) -> bool {
        let filling = self
            .since
            .is_some_and(|since| now.saturating_duration_since(since) < FILL);
        let swapping =
            matches!(self.ready, Some(Some(since)) if now.saturating_duration_since(since) < SWAP);
        filling || swapping
    }
}

fn progress(elapsed: Duration, over: Duration) -> f32 {
    (elapsed.as_secs_f32() / over.as_secs_f32()).clamp(0.0, 1.0)
}

/// The pill's text: the percentage in `percent_color` and the action
/// label (icon and text) in `action_color`, stacked and cross-faded by
/// `mix`.
fn pill_text(
    percent_color: Hsla,
    action_color: Hsla,
    percent: Option<SharedString>,
    icon: IconName,
    label: SharedString,
    mix: f32,
) -> Div {
    let layer = || {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
    };
    div()
        .absolute()
        .inset_0()
        .when_some(percent.filter(|_| mix < 1.0), |text, percent| {
            text.child(
                layer()
                    .opacity(1.0 - mix)
                    .text_color(percent_color)
                    .child(percent),
            )
        })
        .when(mix > 0.0, |text| {
            text.child(
                layer()
                    .opacity(mix)
                    .text_color(action_color)
                    .child(Icon::new(icon).size_3().text_color(action_color))
                    .child(label),
            )
        })
}

/// A box exactly the pill's size: the kit's extra-small button height and
/// padding around the widest label, so every state has the same width.
fn pill_box() -> Div {
    let widest = div()
        .flex()
        .flex_col()
        .h_0()
        .overflow_hidden()
        .opacity(0.)
        .children(
            [
                (IconName::Download, tr(keys::Q_OPS_UPDATE_QUILL)),
                (IconName::RotateCw, tr(keys::Q_OPS_RESTART_TO_UPDATE)),
            ]
            .map(|(icon, text)| {
                div()
                    .flex()
                    .gap_1()
                    .child(Icon::new(icon).size_3())
                    .child(text)
            }),
        );
    div()
        .flex()
        .flex_col()
        .justify_center()
        .h(HEIGHT)
        .px_1()
        .child(widest)
}

impl QuillApp {
    pub(in crate::ui) fn sync_update_pill(&mut self) {
        let control = self.update_control();
        self.update_pill.follow(&control, Instant::now());
    }

    pub(in crate::ui) fn title_bar_update_button(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let control = self.update_control();
        if !control.in_title_bar() {
            return None;
        }
        let now = Instant::now();
        let (fill, mix) = if cx.reduce_motion() {
            (
                self.update_pill.fill(now.checked_add(FILL).unwrap_or(now)),
                self.update_pill
                    .ready_mix(now.checked_add(SWAP).unwrap_or(now)),
            )
        } else {
            if self.update_pill.animating(now) {
                self.request_animation_tick(60, cx);
            }
            (self.update_pill.fill(now), self.update_pill.ready_mix(now))
        };
        let downloading = matches!(control, UpdateControl::Downloading { .. });
        let ready = matches!(control, UpdateControl::Restart | UpdateControl::Restarting);
        let label = Self::update_control_label(&control);
        // A release page this copy cannot install over itself still reads
        // "Update Quill"; the accessible name says where it goes.
        let (icon, action): (IconName, SharedString) = if ready {
            (IconName::RotateCw, tr(keys::Q_OPS_RESTART_TO_UPDATE).into())
        } else {
            (IconName::Download, tr(keys::Q_OPS_UPDATE_QUILL).into())
        };
        let mix = if downloading {
            0.0
        } else if ready {
            mix
        } else {
            1.0
        };
        let percent = (downloading || ready).then(|| {
            let share = control.fraction().unwrap_or(1.0);
            SharedString::from(format!("{:.0}%", share * 100.))
        });
        let theme = cx.theme();
        let accent = theme.button_primary;
        let accent_hover = theme.button_primary_hover;
        let on_accent = theme.button_primary_foreground;
        let text = theme.foreground;
        let track = theme.secondary;
        let ring = theme.ring;
        let pressable = matches!(
            control,
            UpdateControl::Install(_) | UpdateControl::Restart | UpdateControl::OpenRelease(_)
        );
        // The download: an accent capsule inset in the track, concentric
        // with the pill, growing from the left. It carries a copy of the
        // text in the on-accent color laid across the whole pill, so each
        // part of the percentage reads with full contrast on whichever
        // side of the capsule's edge it falls.
        let progress = percent.is_some().then(|| {
            div().absolute().inset(INSET).child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .h_full()
                    .min_w(HEIGHT - INSET * 2.)
                    .w(relative(fill))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(accent)
                    .when(fill <= 0.0, |capsule| capsule.invisible())
                    .child(
                        pill_box()
                            .absolute()
                            .left(-INSET)
                            .top(-INSET)
                            .child(pill_text(
                                on_accent,
                                on_accent,
                                percent.clone(),
                                icon,
                                action.clone(),
                                mix,
                            )),
                    ),
            )
        });
        Some(
            pill_box()
                .id("title-bar-update")
                .test_support()
                .relative()
                .flex_none()
                .rounded_full()
                .bg(track)
                .text_xs()
                .map(|pill| {
                    if downloading {
                        pill.role(Role::ProgressIndicator)
                    } else {
                        pill.role(Role::Button)
                    }
                })
                .aria_label(label)
                // Restart's accent fades in over the track as the
                // percentage fades out.
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .bg(accent)
                        .opacity(mix)
                        .when(pressable, |solid| {
                            solid.group_hover("title-bar-update", |style| style.bg(accent_hover))
                        }),
                )
                .child(pill_text(
                    text,
                    on_accent,
                    percent.clone(),
                    icon,
                    action.clone(),
                    mix,
                ))
                .children(progress)
                .when(pressable, |pill| {
                    pill.group("title-bar-update")
                        .cursor_pointer()
                        .tab_index(0)
                        .focus_visible(move |style| {
                            style.shadow(vec![BoxShadow {
                                color: ring,
                                offset: point(px(0.), px(0.)),
                                blur_radius: px(0.),
                                spread_radius: px(2.),
                                inset: false,
                            }])
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.press_update_control(cx)))
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    // Not a glob: gpui's `test` attribute would shadow the std one.
    use super::{FILL, SWAP, UpdatePill};
    use quill::updater::{DownloadProgress, UpdateControl};
    use std::time::Instant;

    fn downloading(bytes: u64) -> UpdateControl {
        let progress = DownloadProgress::default();
        progress.add(bytes);
        UpdateControl::Downloading {
            progress,
            total: 100,
        }
    }

    #[test]
    fn a_ready_update_seen_at_launch_shows_restart_at_once() {
        let now = Instant::now();
        let mut pill = UpdatePill::default();
        pill.follow(&UpdateControl::Restart, now);
        assert_eq!(pill.fill(now), 1.0);
        assert_eq!(pill.ready_mix(now), 1.0);
        assert!(!pill.animating(now));
    }

    #[test]
    fn the_download_empties_the_pill_at_once_then_eases_the_fill() {
        let now = Instant::now();
        let mut pill = UpdatePill::default();
        pill.follow(&UpdateControl::Check { busy: false }, now);
        pill.follow(&downloading(0), now);
        assert_eq!(pill.fill(now), 0.0);
        pill.follow(&downloading(40), now);
        assert_eq!(pill.fill(now), 0.0);
        assert!(pill.animating(now));
        assert_eq!(pill.fill(now + FILL), 0.4);
        assert_eq!(pill.ready_mix(now + FILL), 0.0);
    }

    #[test]
    fn a_finished_download_fills_up_and_fades_into_restart() {
        let now = Instant::now();
        let mut pill = UpdatePill::default();
        pill.follow(&downloading(90), now);
        pill.follow(&UpdateControl::Restart, now);
        assert_eq!(pill.ready_mix(now), 0.0);
        assert!(pill.fill(now) < 1.0);
        let later = now + SWAP;
        assert_eq!(pill.fill(later), 1.0);
        assert_eq!(pill.ready_mix(later), 1.0);
        assert!(!pill.animating(later));
        // Later frames keep the swap's start.
        pill.follow(&UpdateControl::Restart, later);
        assert_eq!(pill.ready_mix(later), 1.0);
    }
}
