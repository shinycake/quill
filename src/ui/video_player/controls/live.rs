//! The controller's live mode (`VideoController::live`), for pictures
//! with no timeline: a channel's live stream, a live story and, from the
//! next PR, the live PiP (`docs/decisions/video-player-live-ui.md`).
//!
//! There is no scrubber, no time, no speed and no frame step: a live
//! picture has nothing to seek or pause (tdesktop's RTMP panel has none
//! of them either, `calls_group_panel.cpp`). What is left:
//!
//! - the red LIVE pill (tdesktop `groupCallMessageBadge`: 11 px tall,
//!   radius 5, 8 px semibold, uppercase `lng_video_stream_live`)
//!   followed by "{n} viewers" (`lng_group_call_rtmp_viewers`, hidden at
//!   0; `setVideoStreamViewers` in `media_stories_header.cpp`);
//! - mute, and a volume slider only where the engine can set the
//!   picture's volume (ntgcalls cannot for a stream; see the decision
//!   doc), so the slider is absent rather than disabled;
//! - full screen, and Picture-in-Picture when the host has it.
//!
//! Two layouts, because the hosts keep their tdesktop chrome:
//! [`LiveLayout::Bar`] is the stream window's bottom bar (the round
//! full-screen button on a `groupCallBg` backdrop, with the host's own
//! menu and Leave slotted in), [`LiveLayout::Header`] the story header's
//! badge, viewers and sound toggle. The keys are [`live_key_action`]; the
//! full-screen fade is [`super::ControlsFade`].
#![deny(clippy::indexing_slicing)]

use super::{Handler, overlay_button, volume_icon};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, trn_with};
use quill::premium_hub::group_digits;
use std::rc::Rc;

/// `callIconBg`: the bar's discs (settings, menu, mute).
const ICON_BG: u32 = 0xffffff1f;
/// `groupCallMuted1` / `groupCallMuted2`: the round button's gradient.
const ROUND_FROM: u32 = 0x0992ef;
const ROUND_TO: u32 = 0x16ccfb;
/// `groupCallBg` at `kControlsBackgroundOpacity` (0.8): the stream
/// window's control backdrops (the bar, the full-screen title, the
/// empty-stream label).
pub(crate) const CONTROLS_BG: u32 = 0x1a2026cc;
/// The bar's buttons are 68 px tall (`groupCallButtonBottomSkipWide`
/// places them 108 px from the bottom).
pub(crate) const BAR_HEIGHT: f32 = 68.;
/// `groupCallMessageBadge`.
const BADGE_HEIGHT: f32 = 11.;
const BADGE_RADIUS: f32 = 5.;
const BADGE_TEXT: f32 = 8.;
const BADGE_PADDING: f32 = 3.;
/// Between the pill and the viewer count (`groupCallMessageBadge`
/// skip in the story header).
pub(crate) const BADGE_GAP: f32 = 4.;

/// What one draw of the live controller needs.
pub(crate) struct LiveFacts {
    /// Shown after the LIVE pill; 0 hides the count.
    pub(crate) viewers: u32,
    /// The picture's sound is muted for you.
    pub(crate) muted: bool,
    /// The volume slider (0–100), only where the engine can set the
    /// volume; `None` leaves it out.
    pub(crate) volume: Option<Entity<SliderState>>,
    /// The host is full screen (or maximized, for the stream window).
    pub(crate) fullscreen: bool,
}

/// What the live controller's buttons do.
pub(crate) struct LiveActions {
    pub(crate) toggle_mute: Handler,
    /// `None` leaves the button out (a host with no full screen).
    pub(crate) fullscreen: Option<Handler>,
    /// `None` leaves the button out (no live PiP yet).
    pub(crate) pip: Option<Handler>,
    /// The pointer entered or left the controls (the fade's hover).
    pub(crate) hover: Option<Rc<dyn Fn(bool, &mut Window, &mut App)>>,
}

/// How the live controller lays itself out (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveLayout {
    Bar,
    Header,
}

/// The live controller (see the module docs).
#[derive(IntoElement)]
pub(crate) struct LiveController {
    row_id: u64,
    facts: LiveFacts,
    actions: LiveActions,
    layout: LiveLayout,
    leading: Vec<AnyElement>,
    trailing: Vec<AnyElement>,
    variant: Option<ButtonCustomVariant>,
}

impl super::VideoController {
    /// The controller's live mode: no timeline, the LIVE badge, mute and
    /// full screen. `row_id` keys the element ids.
    pub(crate) fn live(row_id: u64, facts: LiveFacts, actions: LiveActions) -> LiveController {
        LiveController {
            row_id,
            facts,
            actions,
            layout: LiveLayout::Bar,
            leading: Vec::new(),
            trailing: Vec::new(),
            variant: None,
        }
    }
}

impl LiveController {
    /// The story header's layout: badge, viewers and the sound toggle.
    pub(crate) fn header(mut self) -> Self {
        self.layout = LiveLayout::Header;
        self
    }

    /// A host control before the controller's own (the bar's menu).
    pub(crate) fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading.push(element.into_any_element());
        self
    }

    /// A host control after the controller's own (the bar's Leave).
    pub(crate) fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing.push(element.into_any_element());
        self
    }

    /// The header's buttons take the host's variant (the story chrome).
    pub(crate) fn button_variant(mut self, variant: ButtonCustomVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}

/// The red LIVE pill alone (`groupCallMessageBadge`).
pub(crate) fn live_pill(cx: &App) -> Div {
    div()
        .debug_selector(|| "live-pill".into())
        .flex_none()
        .h(px(BADGE_HEIGHT))
        .px(px(BADGE_PADDING))
        .rounded(px(BADGE_RADIUS))
        .bg(cx.theme().danger)
        .flex()
        .items_center()
        .text_size(px(BADGE_TEXT))
        .line_height(px(BADGE_HEIGHT))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(gpui_kit::white())
        .child(live_label())
}

/// The pill's text, uppercase as tdesktop draws it.
pub(crate) fn live_label() -> String {
    tr(keys::VIDEO_STREAM_LIVE).to_uppercase()
}

/// The pill's width for `text_width`, the label's natural width at the
/// pill's 8 px.
pub(crate) fn live_pill_width(text_width: f32) -> f32 {
    text_width + 2. * BADGE_PADDING
}

/// The pill's font size, for measuring its label.
pub(crate) fn live_pill_text_size() -> Pixels {
    px(BADGE_TEXT)
}

/// "{n} viewers", or `None` at 0 (the badge then shows the pill alone).
pub(crate) fn viewers_label(viewers: u32) -> Option<String> {
    (viewers > 0).then(|| {
        let count = i64::from(viewers);
        trn_with(
            keys::GROUP_CALL_RTMP_VIEWERS,
            count,
            &[("count", &group_digits(count))],
        )
    })
}

/// The pill followed by the viewer count, in the parent's text style.
pub(crate) fn live_badge(viewers: u32, cx: &App) -> Div {
    div()
        .debug_selector(|| "live-badge".into())
        .flex()
        .items_center()
        .gap(px(BADGE_GAP))
        .min_w_0()
        .child(live_pill(cx))
        .children(viewers_label(viewers).map(|text| div().truncate().child(text)))
}

/// The mute button's icon and label.
fn mute_look(muted: bool) -> (IconName, &'static str) {
    if muted {
        (IconName::VolumeX, tr(keys::Q_AV_UNMUTE))
    } else {
        (volume_icon(1.0), tr(keys::GROUP_CALL_CONTEXT_MUTE))
    }
}

impl RenderOnce for LiveController {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        match self.layout {
            LiveLayout::Bar => render_bar(self, cx),
            LiveLayout::Header => render_header(self, cx),
        }
    }
}

/// A disc in the bar (`CallButton` 44 px in a 60 px slot).
fn bar_slot(width: f32) -> Div {
    div()
        .w(px(width))
        .h(px(BAR_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
}

/// The bar's discs: white on `groupCallMembersBgOver`-like white wash.
pub(crate) fn bar_disc(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(rgba(ICON_BG).into())
        .hover(hsla(0., 0., 1., 0.2))
        .active(hsla(0., 0., 1., 0.25))
        .foreground(gpui_kit::white())
}

fn render_bar(this: LiveController, cx: &mut App) -> AnyElement {
    let row_id = this.row_id;
    let (icon, label) = mute_look(this.facts.muted);
    let toggle_mute = this.actions.toggle_mute.clone();
    let mute = bar_slot(60.).child(
        Button::new(("live-mute", row_id))
            .custom(bar_disc(cx))
            .icon(Icon::new(icon).with_size(px(22.)))
            .rounded(px(22.))
            .size(px(44.))
            .tooltip(label)
            .accessibility_label(label)
            .on_click(move |_, window, cx| toggle_mute(window, cx)),
    );
    let volume = this.facts.volume.map(|slider| {
        div().w(super::metrics::VOLUME_WIDTH).child(
            Slider::new(&slider)
                .bg(crate::ui::chat_theme::accent())
                .text_color(crate::ui::chat_theme::text_on_fill()),
        )
    });
    let full = this.facts.fullscreen;
    // A gradient disc: the kit's custom button takes one solid color.
    let fullscreen = this.actions.fullscreen.map(|toggle| {
        let label = if full {
            tr(keys::Q_AV_EXIT_FULL_SCREEN)
        } else {
            tr(keys::Q_AV_FULL_SCREEN)
        };
        bar_slot(68.).child(
            div()
                .id(("live-fullscreen", row_id))
                .debug_selector(|| "group-call-stream-fullscreen".into())
                .size(px(42.))
                .tab_index(0)
                .rounded_full()
                .bg(linear_gradient(
                    135.,
                    linear_color_stop(rgb(ROUND_FROM), 0.),
                    linear_color_stop(rgb(ROUND_TO), 1.),
                ))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(SharedString::from(label))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx)
                })
                .hover(|style| style.opacity(0.85))
                .on_click(move |_, window, cx| toggle(window, cx))
                .child(
                    Icon::new(if full {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .with_size(px(20.))
                    .text_color(gpui_kit::white()),
                ),
        )
    });
    let pip = this.actions.pip.map(|pip| {
        bar_slot(60.).child(
            Button::new(("live-pip", row_id))
                .custom(bar_disc(cx))
                .icon(Icon::new(IconName::PictureInPicture2).with_size(px(22.)))
                .rounded(px(22.))
                .size(px(44.))
                .tooltip(tr(keys::Q_AV_PICTURE_IN_PICTURE))
                .accessibility_label(tr(keys::Q_AV_PICTURE_IN_PICTURE))
                .on_click(move |_, window, cx| pip(window, cx)),
        )
    });
    let hover = this.actions.hover;
    div()
        .id(("live-controller", row_id))
        .debug_selector(|| "group-call-stream-bar".into())
        .role(Role::Group)
        .aria_label(tr(keys::Q_AV_VIDEO_CONTROLS))
        .h(px(BAR_HEIGHT))
        .px(px(10.))
        .rounded(px(12.))
        .bg(rgba(CONTROLS_BG))
        .flex()
        .items_center()
        .gap(px(5.))
        .when_some(hover, |bar, hover| {
            bar.on_hover(move |inside: &bool, window, cx| hover(*inside, window, cx))
        })
        .children(this.leading)
        .child(mute)
        .children(volume)
        .children(pip)
        .children(fullscreen)
        .children(this.trailing)
        .into_any_element()
}

fn render_header(this: LiveController, cx: &mut App) -> AnyElement {
    let row_id = this.row_id;
    let (icon, label) = mute_look(this.facts.muted);
    let variant = this.variant.unwrap_or_else(|| overlay_button(cx));
    let toggle_mute = this.actions.toggle_mute.clone();
    div()
        .id(("live-controller", row_id))
        .flex()
        .items_center()
        .gap(px(BADGE_GAP))
        .child(live_badge(this.facts.viewers, cx))
        .children(this.leading)
        .child(
            Button::new(("live-mute", row_id))
                .icon(icon)
                .custom(variant)
                .small()
                .tab_index(0)
                .tooltip(label)
                .accessibility_label(label)
                .on_click(move |_, window, cx| toggle_mute(window, cx)),
        )
        .when_some(this.facts.volume, |row, slider| {
            row.child(
                div().w(super::metrics::VOLUME_WIDTH).child(
                    Slider::new(&slider)
                        .bg(crate::ui::chat_theme::accent())
                        .text_color(crate::ui::chat_theme::text_on_fill()),
                ),
            )
        })
        .children(this.trailing)
        .into_any_element()
}

/// What a key does in live mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveKeyAction {
    /// Taken and ignored: Space pauses nothing on a live picture.
    Nothing,
    ToggleMute,
    ToggleFullscreen,
    LeaveFullscreen,
}

/// The live keys: Space does nothing (there is nothing to pause), M
/// mutes, F toggles full screen, Esc leaves full screen. The seek, speed
/// and frame-step keys of the file viewer mean nothing here and pass on.
/// `None`: not a live key (Esc outside full screen closes what it would
/// otherwise close).
pub(crate) fn live_key_action(
    key: &str,
    modifiers: &Modifiers,
    fullscreen: bool,
) -> Option<LiveKeyAction> {
    let plain = !(modifiers.control || modifiers.alt || modifiers.platform || modifiers.function);
    match key {
        "space" if plain => Some(LiveKeyAction::Nothing),
        "m" if plain && !modifiers.shift => Some(LiveKeyAction::ToggleMute),
        "f" if plain && !modifiers.shift => Some(LiveKeyAction::ToggleFullscreen),
        "escape" if fullscreen => Some(LiveKeyAction::LeaveFullscreen),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{LiveKeyAction, live_key_action, live_pill_width, viewers_label};
    use gpui_kit::Modifiers;

    #[test]
    fn the_live_keys_mute_and_toggle_full_screen_and_space_does_nothing() {
        let none = Modifiers::default();
        assert_eq!(
            live_key_action("space", &none, false),
            Some(LiveKeyAction::Nothing)
        );
        assert_eq!(
            live_key_action("m", &none, false),
            Some(LiveKeyAction::ToggleMute)
        );
        assert_eq!(
            live_key_action("f", &none, true),
            Some(LiveKeyAction::ToggleFullscreen)
        );
        assert_eq!(
            live_key_action("escape", &none, true),
            Some(LiveKeyAction::LeaveFullscreen)
        );
        // Outside full screen Esc is the host's (close a box, the window).
        assert_eq!(live_key_action("escape", &none, false), None);
        // No seek, frame step, pause or speed keys on a live picture.
        for key in ["left", "right", "j", "k", "l", ",", ".", "0", "5", "enter"] {
            assert_eq!(live_key_action(key, &none, true), None, "{key}");
        }
        // Shortcuts with modifiers stay the app's (Cmd+M minimizes).
        let cmd = Modifiers {
            platform: true,
            ..Modifiers::default()
        };
        assert_eq!(live_key_action("m", &cmd, false), None);
        let ctrl = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        assert_eq!(live_key_action("f", &ctrl, false), None);
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(live_key_action("m", &shift, false), None);
    }

    #[test]
    fn the_viewer_count_hides_at_zero() {
        assert_eq!(viewers_label(0), None);
        let one = viewers_label(1).expect("one viewer");
        assert!(one.contains('1'), "{one}");
        let many = viewers_label(12_480).expect("many");
        assert!(many.contains("12"), "{many}");
    }

    #[test]
    fn the_pill_is_its_label_and_three_pixels_each_side() {
        assert!((live_pill_width(20.) - 26.).abs() < f32::EPSILON);
    }
}
