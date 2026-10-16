//! The controller's pure rules, then UI integration tests that render it
//! (needs the `demo-capture` feature for gpui-kit's test support).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{IconName, PANEL_OPACITY, VideoControlsState, WaitingCheck, WaitingGate, volume_icon};
use std::time::{Duration, Instant};

#[test]
fn the_volume_icon_follows_the_level() {
    assert_eq!(volume_icon(0.0), IconName::VolumeX);
    assert_eq!(volume_icon(-0.2), IconName::VolumeX);
    assert_eq!(volume_icon(f32::NAN), IconName::VolumeX);
    assert_eq!(volume_icon(0.2), IconName::Volume1);
    assert_eq!(volume_icon(0.49), IconName::Volume1);
    assert_eq!(volume_icon(0.5), IconName::Volume2);
    assert_eq!(volume_icon(1.0), IconName::Volume2);
}

#[test]
fn the_waiting_gate_waits_half_a_second() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut gate = WaitingGate::default();
    assert_eq!(
        gate.observe(false, t0),
        WaitingCheck::Settled { shown: false }
    );
    // A stall begins: wait out the rest of the delay.
    assert_eq!(
        gate.observe(true, ms(100)),
        WaitingCheck::Pending(Duration::from_millis(500))
    );
    assert_eq!(
        gate.observe(true, ms(300)),
        WaitingCheck::Pending(Duration::from_millis(300))
    );
    // A short stall that ends never shows anything.
    assert_eq!(
        gate.observe(false, ms(400)),
        WaitingCheck::Settled { shown: false }
    );
    assert_eq!(
        gate.observe(true, ms(1000)),
        WaitingCheck::Pending(Duration::from_millis(500))
    );
    assert_eq!(
        gate.observe(true, ms(1500)),
        WaitingCheck::Settled { shown: true }
    );
    assert_eq!(
        gate.observe(true, ms(4000)),
        WaitingCheck::Settled { shown: true }
    );
}

#[test]
fn a_stopped_clip_drops_its_gestures() {
    let mut state = VideoControlsState::default();
    assert!(!state.dragging());
    state.seek_preview = Some(3.0);
    assert!(state.dragging());
    state.seek_preview = None;
    state.volume_dragging = true;
    assert!(state.dragging());
    state.hover = Some(0.4);
    let t0 = Instant::now();
    state.boost.press(t0);
    assert!(state.boost.tick(t0 + Duration::from_millis(250)));
    assert!(state.clear_clip(), "the boost was on");
    assert!(!state.dragging());
    assert_eq!(state.hover, None);
    assert!(!state.boost.is_active());
}

/// WCAG relative luminance of an sRGB channel triple.
fn luminance(rgb: [f32; 3]) -> f32 {
    let channel = |c: f32| {
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

fn over(top: f32, alpha: f32, below: f32) -> f32 {
    top * alpha + below * (1.0 - alpha)
}

#[test]
fn the_time_labels_pass_aa_on_any_picture() {
    // Worst case: the panel over a white frame (the backdrop is black 0.92,
    // but the panel may sit over a bright picture edge).
    let panel = over(0.0, PANEL_OPACITY, 1.0);
    let label = over(1.0, 0.9, panel);
    let ratio = (luminance([label; 3]) + 0.05) / (luminance([panel; 3]) + 0.05);
    assert!(ratio >= 4.5, "contrast {ratio}");
}

#[cfg(feature = "demo-capture")]
mod ui {
    use super::super::{
        ControllerActions, PictureOverlay, PictureState, VideoController, VideoControlsState,
        WaitingCheck, WaitingGate, speed_trigger,
    };
    use crate::ui::chat_theme::set_theme_mode;
    use crate::ui::video_player::PlayState;
    use crate::ui::video_player::Transport;
    use gpui_kit::component::slider::SliderState;
    use gpui_kit::component::{Root, ThemeMode};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        Window, div, px, size,
    };
    use gpui_kit::{TestAppContext, VisualTestContext};
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    fn transport(state: PlayState, volume: f32) -> Transport {
        Transport {
            state,
            position: 3.0,
            duration: Some(12.0),
            buffered: vec![0.0..6.0],
            rate: 1.0,
            volume,
            has_sound: true,
            live: false,
        }
    }

    /// A window drawing the controller (and the picture overlay) from a
    /// transport the test sets.
    struct Host {
        transport: Transport,
        controls: VideoControlsState,
        speed: f64,
        gate: WaitingGate,
        failed: bool,
        presses: Rc<Cell<u32>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let presses = self.presses.clone();
            let actions = ControllerActions {
                toggle_play: Rc::new(move |_, _| presses.set(presses.get() + 1)),
                toggle_mute: Rc::new(|_, _| {}),
                fullscreen: Rc::new(|_, _| {}),
                pip: Some(Rc::new(|_, _| {})),
                hover: Rc::new(|_, _, _| {}),
            };
            let boosted = self.controls.boost.is_active();
            let waiting = matches!(
                self.transport.state,
                PlayState::Opening | PlayState::Buffering
            );
            let now = cx.background_executor().now();
            let shown = match self.gate.observe(waiting, now) {
                WaitingCheck::Settled { shown } => shown,
                WaitingCheck::Pending(wait) => {
                    let timer = cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(wait).await;
                        let _ = this.update(cx, |_, cx| cx.notify());
                    });
                    self.gate.arm(timer);
                    false
                }
            };
            let picture = if self.failed {
                Some(PictureState::Failed {
                    detail: "video format not supported".into(),
                    open: Some(Rc::new(|_, _| {})),
                })
            } else {
                shown.then_some(PictureState::Waiting)
            };
            div()
                .size_full()
                .bg(gpui_kit::black())
                .flex()
                .flex_col()
                .items_center()
                .justify_end()
                .child(
                    div()
                        .relative()
                        .w(px(480.))
                        .h(px(270.))
                        .children(picture.map(|state| PictureOverlay::new(1, state))),
                )
                .child(
                    VideoController::new(1, &self.transport, &self.controls, actions)
                        .level(self.transport.volume)
                        .speed_dial(
                            speed_trigger(("media-viewer-speed", 1u64), self.speed, boosted, cx)
                                .into_any_element(),
                        ),
                )
        }
    }

    fn open(cx: &mut TestAppContext, mode: ThemeMode) -> (Entity<Host>, VisualTestContext) {
        cx.update(|cx| set_theme_mode(mode, None, cx));
        let slot = Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(800.), px(500.)), move |window, cx| {
            let host = cx.new(|cx| {
                let seek = cx.new(|_| SliderState::new().min(0.0).max(12.0).default_value(3.0));
                let volume = cx.new(|_| SliderState::new().min(0.0).max(100.0).default_value(80.0));
                Host {
                    transport: transport(PlayState::Playing, 0.8),
                    controls: VideoControlsState {
                        seek: Some(seek),
                        volume: Some(volume),
                        ..Default::default()
                    },
                    speed: 1.0,
                    gate: WaitingGate::default(),
                    failed: false,
                    presses: Rc::new(Cell::new(0)),
                }
            });
            *slot_in.borrow_mut() = Some(host.clone());
            Root::new(host, window, cx)
        });
        let host = slot.borrow().clone().expect("host view");
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        (host, vcx)
    }

    fn label(vcx: &mut VisualTestContext, id: (&'static str, u64)) -> Option<String> {
        vcx.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).label().map(str::to_string)
        })
    }

    fn present(vcx: &mut VisualTestContext, id: (&'static str, u64)) -> bool {
        vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(id).is_some()
        })
    }

    const THEMES: [ThemeMode; 2] = [ThemeMode::Light, ThemeMode::Dark];

    #[gpui_kit::test]
    fn play_and_pause_name_the_next_press(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for mode in THEMES {
            let (host, mut vcx) = open(cx, mode);
            let vcx = &mut vcx;
            assert!(present(vcx, ("media-viewer-player", 1)));
            assert_eq!(
                label(vcx, ("media-viewer-play", 1)).as_deref(),
                Some("Pause")
            );
            vcx.update(|window, cx| window.click(("media-viewer-play", 1u64), cx));
            host.read_with(vcx, |host, _| assert_eq!(host.presses.get(), 1));
            host.update(vcx, |host, cx| {
                host.transport = transport(PlayState::Paused, 0.8);
                cx.notify();
            });
            assert_eq!(
                label(vcx, ("media-viewer-play", 1)).as_deref(),
                Some("Play")
            );
            // Every button the panel draws is named.
            for id in [
                "media-viewer-mute",
                "media-viewer-pip",
                "media-viewer-fullscreen",
            ] {
                assert!(label(vcx, (id, 1)).is_some(), "{id} has a label");
            }
            assert_eq!(
                label(vcx, ("media-viewer-fullscreen", 1)).as_deref(),
                Some("Full screen")
            );
        }
    }

    #[gpui_kit::test]
    fn mute_names_the_level(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for mode in THEMES {
            let (host, mut vcx) = open(cx, mode);
            let vcx = &mut vcx;
            assert_eq!(
                label(vcx, ("media-viewer-mute", 1)).as_deref(),
                Some("Mute")
            );
            host.update(vcx, |host, cx| {
                host.transport = transport(PlayState::Playing, 0.0);
                cx.notify();
            });
            assert_eq!(
                label(vcx, ("media-viewer-mute", 1)).as_deref(),
                Some("Unmute")
            );
        }
    }

    #[gpui_kit::test]
    fn a_held_space_shows_the_boost_rate(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for mode in THEMES {
            let (host, mut vcx) = open(cx, mode);
            let vcx = &mut vcx;
            assert_eq!(
                label(vcx, ("media-viewer-speed", 1)).as_deref(),
                Some("Playback speed 1×")
            );
            host.update(vcx, |host, cx| {
                let t0 = Instant::now();
                host.controls.boost.press(t0);
                assert!(host.controls.boost.tick(t0 + Duration::from_millis(200)));
                cx.notify();
            });
            assert_eq!(
                label(vcx, ("media-viewer-speed", 1)).as_deref(),
                Some("Playback speed 2×")
            );
        }
    }

    #[gpui_kit::test]
    fn buffering_shows_the_spinner_only_after_the_delay(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for mode in THEMES {
            let (host, mut vcx) = open(cx, mode);
            let vcx = &mut vcx;
            host.update(vcx, |host, cx| {
                host.transport = transport(PlayState::Buffering, 0.8);
                cx.notify();
            });
            assert!(!present(vcx, ("viewer-waiting", 1)));
            vcx.executor().advance_clock(Duration::from_millis(300));
            vcx.run_until_parked();
            assert!(!present(vcx, ("viewer-waiting", 1)), "not on a short stall");
            vcx.executor().advance_clock(Duration::from_millis(250));
            vcx.run_until_parked();
            assert!(present(vcx, ("viewer-waiting", 1)), "shown past 500 ms");
            host.update(vcx, |host, cx| {
                host.transport = transport(PlayState::Playing, 0.8);
                cx.notify();
            });
            assert!(!present(vcx, ("viewer-waiting", 1)));
        }
    }

    #[gpui_kit::test]
    fn a_failed_clip_says_so_and_offers_the_system_player(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for mode in THEMES {
            let (host, mut vcx) = open(cx, mode);
            let vcx = &mut vcx;
            host.update(vcx, |host, cx| {
                host.transport = transport(PlayState::Failed, 0.8);
                host.failed = true;
                cx.notify();
            });
            assert!(present(vcx, ("viewer-failed", 1)));
            assert!(present(vcx, ("media-viewer-open-externally", 1)));
            assert!(!present(vcx, ("viewer-waiting", 1)));
        }
    }
}
