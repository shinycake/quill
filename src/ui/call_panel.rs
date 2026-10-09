//! The 1:1 call window (tdesktop `Calls::Panel`): a dark window of its
//! own (720 × 540, at least 380 × 520).
//! - The peer's photo (160 pt) sits on a radial background from their
//!   profile colors, with their name (21 pt semibold) and a gray status
//!   line: "requesting…", "ringing…", "exchanging encryption keys…",
//!   then the call's duration.
//! - The E2E emoji sit at the top.
//! - A row of round buttons (44 pt, 11 pt labels) runs along the bottom.
//! - With video, the peer fills the window and your camera floats in a
//!   corner.
//!
//! Closing the window doesn't end the call: the main window's call bar
//! brings it back.

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon, Root, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::{RemoteVideoState, TransportState};
use quill::telegram::envelope::CallState;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// lib_ui `colors.palette`.
const BG: u32 = 0x1b1f23;
const NAME_FG: u32 = 0xffffff;
const STATUS_FG: u32 = 0xaaabac;
const ANSWER_BG: u32 = 0x66c95b;
const HANGUP_BG: u32 = 0xd75a5a;

/// How long an ended call's window lingers on "call ended".
const ENDED_LINGER: Duration = Duration::from_millis(1600);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CallPhase {
    Incoming,
    Active,
    /// The peer didn't pick up or was busy: Redial or Cancel.
    Busy,
    Ended,
}

/// Everything the call window shows, read from the app each frame.
pub(super) struct CallSnap {
    user_id: i64,
    is_video: bool,
    name: String,
    photo: Option<PathBuf>,
    phase: CallPhase,
    status: String,
    /// Not connected yet: a ring spins around the photo.
    connecting: bool,
    muted: bool,
    camera_on: bool,
    screen_sharing: bool,
    can_share_screen: bool,
    emojis: Vec<String>,
    remote: Option<Arc<RenderImage>>,
    local: Option<Arc<RenderImage>>,
    backdrop: Option<Arc<RenderImage>>,
}

/// tdesktop `Ui::FormatDurationText`: `m:ss`, or `h:mm:ss`.
fn duration_text(secs: i64) -> String {
    let secs = secs.max(0);
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// tdesktop's status line for a live call.
pub(super) fn call_status(
    state: &CallState,
    is_outgoing: bool,
    transport: Option<TransportState>,
    connected_secs: i64,
) -> (String, bool) {
    match state {
        CallState::Pending { .. } if !is_outgoing => ("is calling you...".into(), false),
        CallState::Pending {
            is_created,
            is_received,
        } => (
            if !is_created {
                "requesting..."
            } else if *is_received {
                "ringing..."
            } else {
                "waiting..."
            }
            .into(),
            true,
        ),
        CallState::ExchangingKeys => ("exchanging encryption keys...".into(), true),
        CallState::Ready => match transport {
            Some(TransportState::Connected) => (duration_text(connected_secs), false),
            Some(TransportState::Failed) => ("failed to connect".into(), false),
            _ => ("connecting...".into(), true),
        },
        CallState::HangingUp => ("hanging up...".into(), false),
        _ => ("call ended".into(), false),
    }
}

/// A radial backdrop from the peer's profile colors (tdesktop
/// `PanelBackground`): the first color around the photo, the last at the
/// edges. Cached per palette.
fn backdrop(colors: &[u32]) -> Option<Arc<RenderImage>> {
    use super::lru::Lru;
    use std::cell::RefCell;
    thread_local! {
        static CACHE: RefCell<Lru<Vec<u32>, Arc<RenderImage>>> = RefCell::new(Lru::new(8));
    }
    if colors.len() < 2 {
        return None;
    }
    let key = colors.to_vec();
    if let Some(hit) = CACHE.with(|cache| cache.borrow_mut().get(&key)) {
        return Some(hit);
    }
    let (w, h) = (360u32, 270u32);
    // The photo's center: tdesktop puts it a third of the way down.
    let (cx, cy) = (w as f32 / 2., h as f32 * 0.36);
    let radius = (cx.hypot(cy)).max((w as f32 - cx).hypot(h as f32 - cy));
    let channel = |color: u32, shift: u32| ((color >> shift) & 0xff) as f32;
    let mut image = image::RgbaImage::new(w, h);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let t = ((x as f32 - cx).hypot(y as f32 - cy) / radius).clamp(0., 1.);
        let scaled = t * (colors.len() - 1) as f32;
        let index = (scaled.floor() as usize).min(colors.len() - 2);
        let local = scaled - index as f32;
        let (a, b) = (colors[index], colors[index + 1]);
        let mix = |shift| channel(a, shift) + (channel(b, shift) - channel(a, shift)) * local;
        // BGRA for GPUI.
        *pixel = image::Rgba([mix(0) as u8, mix(8) as u8, mix(16) as u8, 255]);
    }
    let render = Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image),
    ])));
    CACHE.with(|cache| {
        // One per peer palette; a few calls' worth is plenty.
        if let Some(old) = cache.borrow_mut().insert(key, render.clone()) {
            super::image_budget::retire_all([old]);
        }
    });
    Some(render)
}

impl QuillApp {
    /// What the call window shows now, or `None` when there's no call
    /// (and no ended call still lingering).
    pub(super) fn call_snapshot(&mut self) -> Option<CallSnap> {
        let session = self.session()?;
        let call = session.active_call.clone();
        let summary = session.call_summary.clone();
        let (user_id, is_video) = match (&call, &summary) {
            (Some(call), _) => (call.user_id, call.is_video),
            (None, Some(summary)) => (summary.user_id, summary.is_video),
            (None, None) => return None,
        };
        let user = session.user(user_id);
        let name = user
            .map(|u| u.display_name())
            .unwrap_or_else(|| "Telegram user".into());
        let photo = session.user_photo_path(user_id).and_then(|path| {
            quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
        });
        let colors: Vec<u32> = user
            .filter(|u| u.profile_accent_color_id >= 0)
            .and_then(|u| {
                session
                    .profile_accent_colors
                    .iter()
                    .find(|color| color.id == u.profile_accent_color_id)
            })
            .map(|color| color.dark_colors.clone())
            .unwrap_or_default();
        let backdrop = backdrop(&colors);
        let Some(call) = call else {
            let summary = summary?;
            let busy = summary.busy;
            return Some(CallSnap {
                user_id,
                is_video,
                name,
                photo,
                phase: if busy {
                    CallPhase::Busy
                } else {
                    CallPhase::Ended
                },
                status: if busy { "line busy" } else { "call ended" }.into(),
                connecting: false,
                muted: false,
                camera_on: false,
                screen_sharing: false,
                can_share_screen: false,
                emojis: Vec::new(),
                remote: None,
                local: None,
                backdrop,
            });
        };
        let (status, connecting) = call_status(
            &call.state,
            call.is_outgoing,
            call.transport,
            call.connected_secs(),
        );
        let ready = matches!(call.state, CallState::Ready);
        let (remote_frame, local_frame) = match self.live.as_ref() {
            Some(live) => (
                live.driver.latest_video_frame(call.id, false),
                live.driver.latest_video_frame(call.id, true),
            ),
            None => (
                self.demo_remote_frame.clone(),
                self.demo_local_frame.clone(),
            ),
        };
        let screen_frame = if call.remote_screen == RemoteVideoState::Inactive {
            None
        } else if let Some(live) = self.live.as_ref() {
            live.driver.latest_screen_frame(call.id)
        } else {
            self.demo_screen_frame.clone()
        };
        let remote = if !ready {
            None
        } else if let Some(frame) =
            screen_frame.filter(|_| call.remote_screen == RemoteVideoState::Active)
        {
            self.cached_video_image(false, &frame)
        } else if call.remote_video == RemoteVideoState::Active {
            remote_frame.and_then(|frame| self.cached_video_image(false, &frame))
        } else {
            None
        };
        let camera_on = self.call_camera_effective(&call);
        let local = camera_on
            .then(|| local_frame.and_then(|frame| self.cached_video_image(true, &frame)))
            .flatten();
        Some(CallSnap {
            user_id,
            is_video,
            name,
            photo,
            phase: if matches!(call.state, CallState::Pending { .. }) && !call.is_outgoing {
                CallPhase::Incoming
            } else {
                CallPhase::Active
            },
            status,
            connecting,
            muted: call.muted,
            camera_on: call.camera_on,
            screen_sharing: call.screen_sharing,
            can_share_screen: ready && cfg!(target_os = "macos"),
            emojis: call
                .ready
                .as_ref()
                .map(|r| r.emojis.clone())
                .unwrap_or_default(),
            remote,
            local,
            backdrop,
        })
    }

    /// Open, raise or close the call window to follow the call: it opens
    /// for a new call (and comes to the front when one rings in), stays
    /// shut if you closed it, and lingers briefly on "call ended".
    pub(super) fn sync_call_window(&mut self, cx: &mut Context<Self>) {
        let (call_id, incoming, ended, busy) = match self.session() {
            Some(session) => match (&session.active_call, &session.call_summary) {
                (Some(call), _) => (
                    Some(call.id),
                    matches!(call.state, CallState::Pending { .. }) && !call.is_outgoing,
                    false,
                    false,
                ),
                (None, Some(summary)) => (Some(summary.call_id), false, true, summary.busy),
                _ => (None, false, false, false),
            },
            None => (None, false, false, false),
        };
        // The window's call, and when it ended.
        if ended {
            if self.call_ended_at.is_none_or(|(id, _)| Some(id) != call_id) {
                self.call_ended_at = call_id.map(|id| (id, Instant::now()));
            }
        } else {
            self.call_ended_at = None;
        }
        let lingering = self
            .call_ended_at
            .is_some_and(|(_, at)| busy || at.elapsed() < ENDED_LINGER);
        let wanted = call_id
            .filter(|id| (!ended || lingering) && self.call_window_closed_by_user != Some(*id));
        match (wanted, self.call_window) {
            (Some(_), Some(handle)) => {
                if incoming && !self.call_window_raised {
                    self.call_window_raised = true;
                    let _ = handle.update(cx, |_, window, _| window.activate_window());
                }
            }
            (Some(id), None) => self.open_call_window(id, cx),
            (None, Some(handle)) => {
                self.call_window = None;
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            (None, None) => {}
        }
        if ended && lingering && !busy {
            // Close once the linger is over.
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(ENDED_LINGER).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
    }

    fn open_call_window(&mut self, call_id: i32, cx: &mut Context<Self>) {
        if self.call_window_opening {
            return;
        }
        self.call_window_opening = true;
        self.call_window_raised = false;
        let owner = cx.entity();
        cx.defer(move |cx| {
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let bounds = Bounds::centered(None, size(px(720.), px(540.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(380.), px(520.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Call".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(12.))),
                    }),
                    focus: true,
                    show: true,
                    ..Default::default()
                },
                move |window, cx| {
                    window.activate_window();
                    window.on_window_should_close(cx, move |_, cx| {
                        let _ = weak.update(cx, |app, cx| {
                            app.call_window = None;
                            app.call_window_closed_by_user = Some(call_id);
                            cx.notify();
                        });
                        true
                    });
                    let view = cx.new(|cx| CallPanel {
                        owner: render_owner.downgrade(),
                        _observe: cx.observe(&render_owner, |_, _, cx| cx.notify()),
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            owner.update(cx, |app, cx| {
                app.call_window_opening = false;
                match result {
                    Ok(handle) => app.call_window = Some(handle.into()),
                    Err(_) => app.status_note = "Couldn't open the call window".into(),
                }
                cx.notify();
            });
        });
    }

    /// tdesktop `Calls::TopBar`: while a call runs, a 38 pt bar over the
    /// chat (the accent color, gray when muted): mute, who and how long,
    /// hang up. A click anywhere else brings the call window back.
    pub(super) fn call_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let call = session.active_call.as_ref()?;
        let name = session
            .user(call.user_id)
            .map(|u| u.display_name())
            .unwrap_or_else(|| "Telegram user".into());
        let (status, _) = call_status(
            &call.state,
            call.is_outgoing,
            call.transport,
            call.connected_secs(),
        );
        let muted = call.muted;
        let bg: Hsla = if muted {
            rgb(0x8f8f8f).into()
        } else {
            cx.theme().primary
        };
        let icon_button = |id: &'static str, icon: IconName, label: &'static str, turn: f32| {
            div()
                .id(id)
                .w(px(41.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(label)
                .hover(|style| style.bg(hsla(0., 0., 1., 0.12)))
                .child(
                    Icon::new(icon)
                        .with_size(px(18.))
                        .text_color(white())
                        .rotate(radians(turn)),
                )
        };
        Some(
            div()
                .id("call-bar")
                .h(px(38.))
                .flex_none()
                .flex()
                .items_center()
                .bg(bg)
                .text_color(white())
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.show_call_window(cx)))
                .child(
                    icon_button(
                        "call-bar-mute",
                        if muted {
                            IconName::MicOff
                        } else {
                            IconName::Mic
                        },
                        if muted { "Unmute" } else { "Mute" },
                        0.,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_call_mute(cx);
                    })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .justify_center()
                        .gap(px(8.))
                        .text_size(px(13.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(name),
                        )
                        .child(div().opacity(0.85).child(status)),
                )
                .child(
                    icon_button(
                        "call-bar-hangup",
                        IconName::Phone,
                        "End Call",
                        std::f32::consts::PI * 0.75,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.hang_up_call(cx);
                    })),
                )
                .into_any_element(),
        )
    }

    /// Bring the call window back (the call bar's click).
    pub(super) fn show_call_window(&mut self, cx: &mut Context<Self>) {
        self.call_window_closed_by_user = None;
        if let Some(handle) = self.call_window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
        cx.notify();
    }
}

/// A quarter-ish arc turning around the photo, `turn` of the way round.
fn connecting_arc(turn: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = bounds.size.width / px(1.) / 2. - 2.;
            let start = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let sweep = std::f32::consts::FRAC_PI_2 * 1.2;
            let points: Vec<_> = (0..=40)
                .map(|step| {
                    let angle = start + sweep * step as f32 / 40.;
                    point(
                        center.x + px(radius * angle.cos()),
                        center.y + px(radius * angle.sin()),
                    )
                })
                .collect();
            let mut path = PathBuilder::stroke(px(4.));
            path.add_polygon(&points, false);
            if let Ok(path) = path.build() {
                window.paint_path(path, hsla(0., 0., 1., 0.85));
            }
        },
    )
    .size_full()
}

/// The call window's view.
struct CallPanel {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
}

/// One round control: a 44 pt disc and an 11 pt label (tdesktop
/// `CallButton`, 68 × 79).
fn call_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    style: ButtonLook,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    let (bg, fg, rotate) = match style {
        ButtonLook::Plain => (hsla(0., 0., 1., 0.12), white(), false),
        ButtonLook::Active => (hsla(0., 0., 1., 0.9), rgb(0x222222).into(), false),
        ButtonLook::Answer => (rgb(ANSWER_BG).into(), white(), false),
        ButtonLook::Hangup => (rgb(HANGUP_BG).into(), white(), true),
    };
    let mut icon = Icon::new(icon).with_size(px(22.)).text_color(fg);
    if rotate {
        icon = icon.rotate(radians(std::f32::consts::PI * 0.75));
    }
    div()
        .id(id)
        .w(px(68.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.))
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(move |_, _, cx| on_click(cx))
        .child(
            div()
                .size(px(44.))
                .rounded_full()
                .bg(bg)
                .flex()
                .items_center()
                .justify_center()
                .hover(|style| style.opacity(0.88))
                .child(icon)
                // The answer button breathes while it rings (tdesktop's
                // outer ring).
                .when(style == ButtonLook::Answer, |this| {
                    this.relative().child(
                        div()
                            .absolute()
                            .inset(px(-6.))
                            .rounded_full()
                            .border_2()
                            .border_color(rgba(0x50eb4140))
                            .with_animation(
                                "answer-pulse",
                                Animation::new(Duration::from_millis(1400))
                                    .repeat()
                                    .with_easing(pulsating_between(0.2, 1.0)),
                                |ring, delta| ring.opacity(delta),
                            ),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(NAME_FG))
                .child(label),
        )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonLook {
    Plain,
    /// Switched off (muted mic, camera off look in tdesktop is the
    /// inverse: white disc, dark icon).
    Active,
    Answer,
    Hangup,
}

impl Render for CallPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        let Some(snap) = owner.update(cx, |app, _| app.call_snapshot()) else {
            return div().size_full().bg(rgb(BG)).into_any_element();
        };
        let app = self.owner.clone();
        let act = move |f: fn(&mut QuillApp, &mut Context<QuillApp>)| {
            let app = app.clone();
            move |cx: &mut App| {
                let _ = app.update(cx, f);
            }
        };
        let video = snap.remote.is_some();
        // The layout's own size (screen size shrinks by the interface scale).
        let bounds = window.viewport_size();
        let compact = bounds.height < px(480.);
        let photo_size = if snap.local.is_some() && !video || compact {
            100.
        } else {
            160.
        };

        // The buttons for this moment of the call.
        let buttons: Vec<AnyElement> = match snap.phase {
            CallPhase::Incoming => vec![
                call_button(
                    "call-video",
                    if snap.camera_on {
                        IconName::Video
                    } else {
                        IconName::VideoOff
                    },
                    if snap.camera_on {
                        "Stop Video"
                    } else {
                        "Start Video"
                    },
                    if snap.camera_on {
                        ButtonLook::Plain
                    } else {
                        ButtonLook::Active
                    },
                    act(QuillApp::toggle_call_camera),
                )
                .into_any_element(),
                call_button(
                    "call-decline",
                    IconName::Phone,
                    "Decline",
                    ButtonLook::Hangup,
                    act(QuillApp::hang_up_call),
                )
                .into_any_element(),
                call_button(
                    "call-accept",
                    IconName::Phone,
                    "Accept",
                    ButtonLook::Answer,
                    act(QuillApp::accept_incoming_call),
                )
                .into_any_element(),
            ],
            CallPhase::Active => {
                let mut row = vec![
                    call_button(
                        "call-video",
                        if snap.camera_on {
                            IconName::Video
                        } else {
                            IconName::VideoOff
                        },
                        if snap.camera_on {
                            "Stop Video"
                        } else {
                            "Start Video"
                        },
                        if snap.camera_on {
                            ButtonLook::Plain
                        } else {
                            ButtonLook::Active
                        },
                        act(QuillApp::toggle_call_camera),
                    )
                    .into_any_element(),
                ];
                if snap.can_share_screen {
                    row.push(
                        call_button(
                            "call-screen",
                            IconName::ScreenShare,
                            "Screencast",
                            if snap.screen_sharing {
                                ButtonLook::Plain
                            } else {
                                ButtonLook::Active
                            },
                            act(QuillApp::toggle_call_screen_share),
                        )
                        .into_any_element(),
                    );
                }
                row.push(
                    call_button(
                        "call-mute",
                        if snap.muted {
                            IconName::MicOff
                        } else {
                            IconName::Mic
                        },
                        if snap.muted { "Unmute" } else { "Mute" },
                        if snap.muted {
                            ButtonLook::Active
                        } else {
                            ButtonLook::Plain
                        },
                        act(QuillApp::toggle_call_mute),
                    )
                    .into_any_element(),
                );
                row.push(
                    call_button(
                        "call-end",
                        IconName::Phone,
                        "End Call",
                        ButtonLook::Hangup,
                        act(QuillApp::hang_up_call),
                    )
                    .into_any_element(),
                );
                row
            }
            CallPhase::Busy => {
                let (user_id, is_video) = (snap.user_id, snap.is_video);
                let redial_app = self.owner.clone();
                vec![
                    call_button(
                        "call-cancel",
                        IconName::Close,
                        "Cancel",
                        ButtonLook::Active,
                        act(QuillApp::dismiss_call_summary),
                    )
                    .into_any_element(),
                    call_button(
                        "call-redial",
                        IconName::Phone,
                        "Redial",
                        ButtonLook::Answer,
                        move |cx| {
                            let _ = redial_app.update(cx, |app, cx| {
                                app.dismiss_call_summary(cx);
                                app.call_again(user_id, is_video, cx);
                            });
                        },
                    )
                    .into_any_element(),
                ]
            }
            CallPhase::Ended => Vec::new(),
        };

        let photo = div()
            .relative()
            .size(px(photo_size))
            .child(chat_avatar(&snap.name, snap.photo.as_deref(), photo_size))
            // A ring turns around the photo until the call connects
            // (tdesktop `callConnectingRadial`).
            .when(snap.connecting, |this| {
                this.child(div().absolute().inset(px(-8.)).with_animation(
                    "call-connecting",
                    Animation::new(Duration::from_millis(1400)).repeat(),
                    |ring, delta| ring.child(connecting_arc(delta)),
                ))
            });

        let name = div()
            .text_size(px(21.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(NAME_FG))
            .truncate()
            .child(snap.name.clone());
        let status = div()
            .text_size(px(14.))
            .text_color(rgb(STATUS_FG))
            .child(snap.status.clone());

        // The 4 emoji everyone in the call should see the same.
        let fingerprint = (!snap.emojis.is_empty()).then(|| {
            div()
                .id("call-fingerprint")
                .absolute()
                .top(px(10.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("call-fingerprint-pill")
                        .px(px(10.))
                        .py(px(4.))
                        .rounded_full()
                        .bg(hsla(0., 0., 0., 0.3))
                        .text_size(px(18.))
                        .flex()
                        .gap(px(4.))
                        .children(snap.emojis.iter().map(|emoji| div().child(emoji.clone())))
                        .tooltip({
                            let tip: SharedString = format!(
                                "If the emoji on {}'s screen are the same, this call is 100% secure",
                                snap.name
                            )
                            .into();
                            move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        }),
                )
        });

        let backdrop = match (&snap.remote, &snap.backdrop) {
            (Some(remote), _) => img(ImageSource::Render(remote.clone()))
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            (None, Some(image)) => img(ImageSource::Render(image.clone()))
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            (None, None) => div().into_any_element(),
        };

        // Your camera, in a corner above the buttons.
        let preview = snap.local.clone().map(|local| {
            div()
                .absolute()
                .right(px(12.))
                .bottom(px(99.))
                .w(px(160.))
                .h(px(110.))
                .rounded(px(10.))
                .shadow_lg()
                // GPUI clips to rectangles: the image rounds itself.
                .child(
                    img(ImageSource::Render(local))
                        .size_full()
                        .rounded(px(10.))
                        .object_fit(ObjectFit::Cover),
                )
        });

        let body = if video {
            // Over the peer's video: name and status up top, legible.
            div()
                .absolute()
                .top(px(54.))
                .left_0()
                .right_0()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .child(name.text_size(px(17.)))
                .child(status)
                .into_any_element()
        } else {
            div()
                .absolute()
                .inset_0()
                .pb(px(87.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .child(photo)
                .child(div().mt(px(16.)).max_w(px(420.)).child(name))
                .child(status)
                .into_any_element()
        };

        let _ = cx.theme();
        div()
            .id("call-panel")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(BG))
            .font_family(".SystemUIFont")
            .child(backdrop)
            // Shades under the overlays when the peer's video fills it.
            .when(video, |this| {
                this.child(div().absolute().top_0().left_0().right_0().h(px(124.)).bg(
                    linear_gradient(
                        180.,
                        linear_color_stop(hsla(0., 0., 0., 0.45), 0.),
                        linear_color_stop(hsla(0., 0., 0., 0.), 1.),
                    ),
                ))
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(124.))
                        .bg(linear_gradient(
                            0.,
                            linear_color_stop(hsla(0., 0., 0., 0.5), 0.),
                            linear_color_stop(hsla(0., 0., 0., 0.), 1.),
                        )),
                )
            })
            .child(body)
            .children(fingerprint)
            .children(preview)
            .child(
                div()
                    .absolute()
                    .bottom(px(8.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .gap(px(8.))
                    .children(buttons),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{call_status, duration_text};
    use quill::calls::engine::TransportState;
    use quill::telegram::envelope::CallState;

    #[test]
    fn statuses_follow_tdesktop() {
        let pending = |is_created, is_received| CallState::Pending {
            is_created,
            is_received,
        };
        assert_eq!(
            call_status(&pending(false, false), true, None, 0).0,
            "requesting..."
        );
        assert_eq!(
            call_status(&pending(true, false), true, None, 0).0,
            "waiting..."
        );
        assert_eq!(
            call_status(&pending(true, true), true, None, 0).0,
            "ringing..."
        );
        assert_eq!(
            call_status(&pending(true, true), false, None, 0).0,
            "is calling you..."
        );
        assert_eq!(
            call_status(&CallState::ExchangingKeys, true, None, 0).0,
            "exchanging encryption keys..."
        );
        let ready = |t| call_status(&CallState::Ready, true, t, 75);
        assert_eq!(
            ready(Some(TransportState::Connected)),
            ("1:15".to_string(), false)
        );
        assert_eq!(ready(None), ("connecting...".to_string(), true));
        assert_eq!(ready(Some(TransportState::Failed)).0, "failed to connect");
    }

    #[test]
    fn durations_read_like_tdesktop() {
        assert_eq!(duration_text(0), "0:00");
        assert_eq!(duration_text(605), "10:05");
        assert_eq!(duration_text(3725), "1:02:05");
    }
}
