//! The live controller's PiP form (see the module docs of `pip`): the
//! LIVE badge with the viewers in the bottom corner, where a file's
//! progress line is, always shown so the window reads as live; close,
//! enlarge and mute fade in with the pointer like the file PiP's buttons.
//! The whole picture moves the window: there is no line to press.

use super::super::super::{PlayState, VideoPlayer};
use super::super::live::live_badge;
use super::super::volume_icon;
use super::{LivePip, VideoPip};
use gpui_kit::assets::IconName;
use gpui_kit::*;
use quill::calls::live_video::LiveEnd;
use quill::lang::{keys, tr};
use quill::media_viewer::pip::layout::{ControlsLayout, OverState, PLAYBACK_SKIP, PressAction};
use quill::media_viewer::pip::panel::PipPoint;
use std::rc::Rc;

/// `radialBg` (black at 0x56): the badge's backing, so it reads over any
/// picture.
const BADGE_BACKING: f32 = 0.34;

/// What the player's state means for the window, on each draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EndCheck {
    Plays,
    /// A live feed was left, not ended: the connecting state shows until
    /// the owner swaps the player or closes the window.
    Gap,
    /// A clip that failed (`handleStreamingError`), or a live picture
    /// that failed or whose stream is over.
    Close,
}

impl VideoPip {
    /// See [`EndCheck`]. The first draw of a gap tells the owner
    /// (`LivePip::feed_left`), deferred, once per player.
    pub(super) fn end_check(&mut self, player: &VideoPlayer, cx: &mut Context<Self>) -> EndCheck {
        match player.state() {
            PlayState::Failed => EndCheck::Close,
            PlayState::Ended => {
                let Some(live) = self.live.as_ref() else {
                    return EndCheck::Plays;
                };
                let left = player
                    .live_sink()
                    .is_some_and(|sink| sink.snapshot().end == Some(LiveEnd::Left));
                if !left {
                    return EndCheck::Close;
                }
                if !std::mem::replace(&mut self.feed_left_told, true) {
                    let feed_left = live.feed_left.clone();
                    cx.defer(move |cx| feed_left(cx));
                }
                EndCheck::Gap
            }
            _ => EndCheck::Plays,
        }
    }

    /// What the pointer is over at `local` (a live window's layout has
    /// no lines: `ControlsLayout::live`).
    pub(super) fn over_at(&self, window: &Window, local: PipPoint) -> OverState {
        self.layout(window).state_at(local)
    }

    /// M mutes a live picture, as in the stream window. `true` when taken.
    pub(super) fn live_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_ref() else {
            return false;
        };
        let modifiers = &event.keystroke.modifiers;
        let plain = !(modifiers.control
            || modifiers.alt
            || modifiers.platform
            || modifiers.function
            || modifiers.shift);
        if event.keystroke.key != "m" || !plain || event.is_held {
            return false;
        }
        cx.stop_propagation();
        self.toggle_live_mute(live.toggle_mute.clone(), cx);
        true
    }

    /// Mute or unmute through the owner, then redraw with its new state.
    pub(super) fn toggle_live_mute(
        &mut self,
        toggle: Rc<dyn Fn(&mut App)>,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        cx.defer(move |cx| {
            toggle(cx);
            let _ = view.update(cx, |_, cx| cx.notify());
        });
    }

    /// The live controls: the badge (always), and close, enlarge and
    /// mute at `shown` opacity.
    pub(super) fn live_controls(
        &self,
        live: &LivePip,
        layout: &ControlsLayout,
        shown: f32,
        active: (f32, f32, f32),
        cx: &mut Context<Self>,
    ) -> (Div, Div) {
        let facts = (live.facts)(cx);
        let (close_active, enlarge_active, volume_active) = active;
        let (mute_icon, mute_label) = if facts.muted {
            (IconName::VolumeX, tr(keys::Q_AV_UNMUTE))
        } else {
            (volume_icon(1.0), tr(keys::GROUP_CALL_CONTEXT_MUTE))
        };
        let buttons = div()
            .absolute()
            .inset_0()
            .opacity(shown)
            .child(self.icon_button(
                "pip-close",
                layout.close,
                IconName::X,
                tr(keys::CLOSE),
                PressAction::Close,
                close_active,
                cx,
            ))
            .child(self.icon_button(
                "pip-enlarge",
                layout.enlarge,
                IconName::Maximize2,
                live.back_label,
                PressAction::Enlarge,
                enlarge_active,
                cx,
            ))
            .child(
                self.icon_button(
                    "pip-volume",
                    layout.volume_toggle,
                    mute_icon,
                    mute_label,
                    PressAction::ToggleVolume,
                    volume_active,
                    cx,
                )
                .debug_selector(|| "live-pip-mute".into()),
            );
        let skip = px(PLAYBACK_SKIP as f32);
        let badge = div()
            .debug_selector(|| "live-pip-badge".into())
            .absolute()
            .left(skip)
            .bottom(skip)
            .max_w(px((layout.inner.w - 2 * PLAYBACK_SKIP).max(0) as f32))
            .px(px(5.))
            .py(px(3.))
            .rounded(px(6.))
            .bg(gpui_kit::black().opacity(BADGE_BACKING))
            .text_size(px(11.))
            .text_color(gpui_kit::white())
            .child(live_badge(facts.viewers));
        (buttons, badge)
    }
}
