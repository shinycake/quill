//! The 1:1 call window (tdesktop `Calls::Panel`): a dark window of its
//! own (720 × 540, at least 380 × 520).
//! - The peer's photo (160 pt) sits on a radial background from their
//!   profile colors, with their name (21 pt semibold) and a gray status
//!   line: "requesting…", "ringing…", "exchanging encryption keys…",
//!   then the call's duration.
//! - The E2E emoji sit at the top.
//! - A row of round buttons (44 pt, 11 pt labels) runs along the bottom.
//! - With video, the peer fills the window and your camera floats in a
//!   corner (`calls::call_video`, which also holds the window's root).
//!
//! Closing the window doesn't end the call: the main window's call bar
//! brings it back.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::call_panel_backdrop::connecting_arc;
use super::call_panel_devices::{self as devices, DeviceMenuTarget};
use super::call_panel_notices as notices;
use super::calls::call_video::{CallPicture, CallWindowRoot};
use super::chat_row::chat_avatar;
use super::nested_click::SwallowPress;
use crate::ui::app_theme::call_chrome;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon, Root, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::TransportState;
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::CallState;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Where the round buttons' row ends, from the window's bottom: 8 px
/// under it, then a 44 px disc, a 6 px gap and the 11 px label's line.
const BUTTONS_TOP: f32 = 8. + 44. + 6. + 15.;

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
    /// The peer's microphone is off (tdesktop `lng_call_microphone_off`).
    remote_muted: bool,
    /// The peer's battery is low (tdesktop `lng_call_battery_level_low`).
    remote_battery_low: bool,
    /// The connection's signal bars, once established (tdesktop
    /// `Calls::SignalBars`).
    signal_bars: Option<u8>,
    /// The peer's picture has a frame: it fills the window.
    video: bool,
    /// Your camera's preview has a frame.
    preview: bool,
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
        CallState::Pending { .. } if !is_outgoing => (tr(keys::CALL_STATUS_INCOMING).into(), false),
        CallState::Pending {
            is_created,
            is_received,
        } => (
            if !is_created {
                tr(keys::CALL_STATUS_REQUESTING)
            } else if *is_received {
                tr(keys::CALL_STATUS_RINGING)
            } else {
                tr(keys::CALL_STATUS_WAITING)
            }
            .into(),
            true,
        ),
        CallState::ExchangingKeys => (tr(keys::CALL_STATUS_EXCHANGING).into(), true),
        CallState::Ready => match transport {
            Some(TransportState::Connected) => (duration_text(connected_secs), false),
            Some(TransportState::Failed) => (tr(keys::CALL_STATUS_FAILED).into(), false),
            _ => (tr(keys::CALL_STATUS_CONNECTING).into(), true),
        },
        CallState::HangingUp => (tr(keys::CALL_STATUS_HANGING).into(), false),
        _ => (tr(keys::CALL_STATUS_ENDED).into(), false),
    }
}

impl QuillApp {
    /// What the call window shows now, or `None` when there's no call
    /// (and no ended call still lingering).
    pub(super) fn call_snapshot(&mut self) -> Option<CallSnap> {
        let session = self.session()?;
        let call = session.calls.active_call.clone();
        let summary = session.calls.summary.clone();
        let (user_id, is_video) = match (&call, &summary) {
            (Some(call), _) => (call.user_id, call.is_video),
            (None, Some(summary)) => (summary.user_id, summary.is_video),
            (None, None) => return None,
        };
        let user = session.user(user_id);
        let name = user
            .map(|u| u.display_name())
            .unwrap_or_else(|| tr(keys::Q_OPS_TELEGRAM_USER).into());
        let photo = session.user_photo_path(user_id).and_then(|path| {
            quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
        });
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
                status: if busy {
                    tr(keys::CALL_STATUS_BUSY)
                } else {
                    tr(keys::CALL_STATUS_ENDED)
                }
                .into(),
                connecting: false,
                muted: false,
                camera_on: false,
                screen_sharing: false,
                can_share_screen: false,
                emojis: Vec::new(),
                remote_muted: false,
                remote_battery_low: false,
                signal_bars: None,
                video: false,
                preview: false,
            });
        };
        let (status, connecting) = call_status(
            &call.state,
            call.is_outgoing,
            call.transport,
            call.connected_secs(),
        );
        let ready = matches!(call.state, CallState::Ready);
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
            can_share_screen: ready && quill::calls::capture::screen::can_share(),
            emojis: call
                .ready
                .as_ref()
                .map(|r| r.emojis.clone())
                .unwrap_or_default(),
            remote_muted: ready && call.remote_audio_muted,
            remote_battery_low: ready && call.remote_battery_low,
            signal_bars: notices::shown_signal_bars(&call),
            // The root made the players follow the call before this draw.
            video: self.call_picture_drawn(CallPicture::Remote),
            preview: self.call_picture_drawn(CallPicture::Local),
        })
    }

    /// Open, raise or close the call window to follow the call: it opens
    /// for a new call (and comes to the front when one rings in), stays
    /// shut if you closed it, and lingers briefly on "call ended".
    pub(super) fn sync_call_window(&mut self, cx: &mut Context<Self>) {
        let (call_id, incoming, ended, busy) = match self.session() {
            Some(session) => match (&session.calls.active_call, &session.calls.summary) {
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
            if self
                .calls
                .ended_at
                .is_none_or(|(id, _)| Some(id) != call_id)
            {
                self.calls.ended_at = call_id.map(|id| (id, Instant::now()));
            }
        } else {
            self.calls.ended_at = None;
        }
        let lingering = self
            .calls
            .ended_at
            .is_some_and(|(_, at)| busy || at.elapsed() < ENDED_LINGER);
        let wanted = call_id
            .filter(|id| (!ended || lingering) && self.calls.window_closed_by_user != Some(*id));
        match (wanted, self.calls.window) {
            (Some(_), Some(handle)) => {
                if incoming && !self.calls.window_raised {
                    self.calls.window_raised = true;
                    let _ = handle.update(cx, |_, window, _| window.activate_window());
                }
            }
            (Some(id), None) => self.open_call_window(id, cx),
            (None, Some(handle)) => {
                self.calls.forget_window();
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            (None, None) => {}
        }
        if wanted.is_none() {
            self.release_call_players(cx);
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
        if self.calls.window_opening {
            return;
        }
        self.calls.window_opening = true;
        self.calls.window_raised = false;
        let owner = cx.entity();
        cx.defer(move |cx| {
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let panel_view = owner.read(cx).calls.panel_view.clone();
            let bounds = Bounds::centered(None, size(px(720.), px(540.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(380.), px(520.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some(tr(keys::PROFILE_ACTION_SHORT_CALL).into()),
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
                            app.calls.forget_window();
                            app.calls.window_closed_by_user = Some(call_id);
                            cx.notify();
                        });
                        true
                    });
                    let view = cx.new(|cx| CallWindowRoot::new(&render_owner, &panel_view, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            owner.update(cx, |app, cx| {
                app.calls.window_opening = false;
                match result {
                    Ok(handle) => app.calls.window = Some(handle.into()),
                    Err(_) => app.connection.set_note(StatusNote::error(tr(
                        keys::Q_OPS_COULDNT_OPEN_THE_CALL_WINDOW,
                    ))),
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
        let call = session.calls.active_call.as_ref()?;
        let name = session
            .user(call.user_id)
            .map(|u| u.display_name())
            .unwrap_or_else(|| tr(keys::Q_OPS_TELEGRAM_USER).into());
        let (status, _) = call_status(
            &call.state,
            call.is_outgoing,
            call.transport,
            call.connected_secs(),
        );
        let muted = call.muted;
        let bg: Hsla = if muted {
            call_chrome::muted_fill()
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
                .hover(|style| style.bg(call_chrome::wash()))
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
                        if muted {
                            tr(keys::CALL_UNMUTE_AUDIO)
                        } else {
                            tr(keys::CALL_MUTE_AUDIO)
                        },
                        0.,
                    )
                    .swallow_press()
                    .on_click(cx.listener(|this, _, _, cx| {
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
                .children(
                    notices::shown_signal_bars(call)
                        .map(|bars| notices::signal_bars(bars, notices::BarsStyle::Bar)),
                )
                .child(
                    icon_button(
                        "call-bar-hangup",
                        IconName::Phone,
                        tr(keys::CALL_END_CALL),
                        std::f32::consts::PI * 0.75,
                    )
                    .swallow_press()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.hang_up_call(cx);
                    })),
                )
                .into_any_element(),
        )
    }

    /// Bring the call window back (the call bar's click).
    pub(super) fn show_call_window(&mut self, cx: &mut Context<Self>) {
        self.calls.window_closed_by_user = None;
        if let Some(handle) = self.calls.window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
        cx.notify();
    }
}

/// The call window's panel: everything over the pictures.
pub(crate) struct CallPanel {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
    /// The window stays above the others (tdesktop "pin on top").
    pinned_on_top: bool,
    /// The open device menu (`call_panel_devices`).
    devices_menu: Option<DeviceMenuTarget>,
}

impl CallPanel {
    pub(crate) fn new(owner: &Entity<QuillApp>, cx: &mut Context<Self>) -> Self {
        Self {
            owner: owner.downgrade(),
            _observe: cx.observe(owner, |_, _, cx| cx.notify()),
            pinned_on_top: false,
            devices_menu: demo_devices_menu(),
        }
    }
}

/// `QUILL_DEMO_DEVICES=1` (demo-capture only): the call window opens with
/// the audio device menu showing.
fn demo_devices_menu() -> Option<DeviceMenuTarget> {
    #[cfg(feature = "demo-capture")]
    if std::env::var_os("QUILL_DEMO_DEVICES").is_some() {
        return Some(DeviceMenuTarget::Audio);
    }
    None
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
        ButtonLook::Plain => (call_chrome::wash(), call_chrome::text(), false),
        ButtonLook::Active => (call_chrome::active(), call_chrome::on_active(), false),
        ButtonLook::Answer => (call_chrome::answer_fill(), white(), false),
        ButtonLook::Hangup => (call_chrome::hangup_fill(), white(), true),
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
                    let ring = div()
                        .absolute()
                        .inset(px(-6.))
                        .rounded_full()
                        .border_2()
                        .border_color(call_chrome::answer_ring());
                    // Battery and animations: a still ring.
                    let ring = if quill::power_saving::on(quill::power_saving::Flag::Calls) {
                        ring.opacity(0.6).into_any_element()
                    } else {
                        ring.with_animation(
                            "answer-pulse",
                            Animation::new(Duration::from_millis(1400))
                                .repeat()
                                .with_easing(pulsating_between(0.2, 1.0)),
                            |ring, delta| ring.opacity(delta),
                        )
                        .into_any_element()
                    };
                    this.relative().child(ring)
                }),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(call_chrome::text())
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
        super::frame_clock::trace_slice_render("call-panel");
        let Some(owner) = self.owner.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        let Some(snap) = owner.update(cx, |app, _| app.call_snapshot()) else {
            return div().size_full().into_any_element();
        };
        let app = self.owner.clone();
        let act = move |f: fn(&mut QuillApp, &mut Context<QuillApp>)| {
            let app = app.clone();
            move |cx: &mut App| {
                let _ = app.update(cx, f);
            }
        };
        let video = snap.video;
        // The layout's own size (screen size shrinks by the interface scale).
        let bounds = window.viewport_size();
        let compact = bounds.height < px(480.);
        let photo_size = if snap.preview && !video || compact {
            100.
        } else {
            160.
        };

        let menu_open = self.devices_menu;
        let corner = |button: AnyElement,
                      id: &'static str,
                      target: DeviceMenuTarget,
                      cx: &mut Context<Self>| {
            devices::with_corner(
                button,
                id,
                target,
                menu_open == Some(target),
                cx,
                |this, target, _, cx| {
                    this.devices_menu = devices::toggled(this.devices_menu, target);
                    cx.notify();
                },
            )
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
                        tr(keys::CALL_STOP_VIDEO)
                    } else {
                        tr(keys::CALL_START_VIDEO)
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
                    tr(keys::CALL_DECLINE),
                    ButtonLook::Hangup,
                    act(QuillApp::hang_up_call),
                )
                .into_any_element(),
                call_button(
                    "call-accept",
                    IconName::Phone,
                    tr(keys::CALL_ACCEPT),
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
                            tr(keys::CALL_STOP_VIDEO)
                        } else {
                            tr(keys::CALL_START_VIDEO)
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
                if !row.is_empty() {
                    let first = row.remove(0);
                    row.insert(
                        0,
                        corner(first, "call-video-devices", DeviceMenuTarget::Camera, cx),
                    );
                }
                if snap.can_share_screen {
                    row.push(
                        call_button(
                            "call-screen",
                            IconName::ScreenShare,
                            tr(keys::CALL_SCREENCAST),
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
                        if snap.muted {
                            tr(keys::CALL_UNMUTE_AUDIO)
                        } else {
                            tr(keys::CALL_MUTE_AUDIO)
                        },
                        if snap.muted {
                            ButtonLook::Active
                        } else {
                            ButtonLook::Plain
                        },
                        act(QuillApp::toggle_call_mute),
                    )
                    .into_any_element(),
                );
                let mute = row.pop().unwrap_or_else(|| div().into_any_element());
                row.push(corner(
                    mute,
                    "call-mute-devices",
                    DeviceMenuTarget::Audio,
                    cx,
                ));
                row.push(
                    call_button(
                        "call-end",
                        IconName::Phone,
                        tr(keys::CALL_END_CALL),
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
                        tr(keys::CALL_CANCEL),
                        ButtonLook::Active,
                        act(QuillApp::dismiss_call_summary),
                    )
                    .into_any_element(),
                    call_button(
                        "call-redial",
                        IconName::Phone,
                        tr(keys::CALL_REDIAL),
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
                let ring = div().absolute().inset(px(-8.));
                if quill::power_saving::on(quill::power_saving::Flag::Calls) {
                    // Battery and animations: the arc holds still.
                    this.child(ring.child(connecting_arc(0.)))
                } else {
                    this.child(ring.with_animation(
                        "call-connecting",
                        Animation::new(Duration::from_millis(1400)).repeat(),
                        |ring, delta| ring.child(connecting_arc(delta)),
                    ))
                }
            });

        let name = div()
            .text_size(px(21.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(call_chrome::text())
            .truncate()
            .child(snap.name.clone());
        let status = div()
            .text_size(px(14.))
            .text_color(call_chrome::muted())
            .child(snap.status.clone());
        // tdesktop `callRemoteAudioMute`: a 12 px line under the status
        // while the peer's microphone is off.
        let mic_off = snap.remote_muted.then(|| {
            div()
                .text_size(px(12.))
                .text_color(call_chrome::muted())
                .child(notices::microphone_off_line(&snap.name))
        });

        // tdesktop's low-battery notice over the buttons, while the
        // microphone notice is not showing.
        let battery_low = notices::battery_notice_shown(snap.remote_battery_low, snap.remote_muted)
            .then(|| notices::battery_low_notice(&snap.name, BUTTONS_TOP));

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
                        .bg(call_chrome::pill())
                        .text_size(px(18.))
                        .flex()
                        .gap(px(4.))
                        .children(snap.emojis.iter().map(|emoji| div().child(emoji.clone())))
                        .children(
                            snap.signal_bars
                                .map(|bars| notices::signal_bars(bars, notices::BarsStyle::Panel)),
                        )
                        .tooltip({
                            let tip: SharedString = tr_with(
                                keys::CALL_FINGERPRINT_TOOLTIP,
                                &[("user", &snap.name.to_string())],
                            )
                            .into();
                            move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(tip.clone())
                                    .build(window, cx)
                            }
                        }),
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
                .children(mic_off)
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
                .children(mic_off)
                .into_any_element()
        };

        // The device menu closes with the call's active phase.
        let device_menu = self
            .devices_menu
            .filter(|_| snap.phase == CallPhase::Active)
            .map(|target| {
                let sections = owner.update(cx, |app, _| app.call_device_sections(target));
                let app = self.owner.clone();
                devices::menu_overlay(
                    sections,
                    cx,
                    move |this, kind, id, _, cx| {
                        this.devices_menu = None;
                        let _ = app.update(cx, |app, cx| app.select_call_device(kind, &id, cx));
                        cx.notify();
                    },
                    |this, _, cx| {
                        this.devices_menu = None;
                        cx.notify();
                    },
                )
            });

        let _ = cx.theme();
        div()
            .id("call-panel")
            .size_full()
            .relative()
            .overflow_hidden()
            .font_family(tr(keys::Q_OPS_SYSTEMUIFONT))
            // Shades under the overlays when the peer's video fills it.
            .when(video, |this| {
                this.child(div().absolute().top_0().left_0().right_0().h(px(124.)).bg(
                    linear_gradient(
                        180.,
                        linear_color_stop(call_chrome::video_shade_top(), 0.),
                        linear_color_stop(call_chrome::clear(), 1.),
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
                            linear_color_stop(call_chrome::video_shade_bottom(), 0.),
                            linear_color_stop(call_chrome::clear(), 1.),
                        )),
                )
            })
            .child(body)
            .children(fingerprint)
            .children(battery_low)
            // tdesktop's pin-on-top control, in the corner under the title.
            .child(
                super::window_control::pin_on_top_button(
                    "call-pin-on-top",
                    self.pinned_on_top,
                    cx,
                    |this, pinned, _, cx| {
                        this.pinned_on_top = pinned;
                        cx.notify();
                    },
                )
                .text_color(call_chrome::text())
                .top(px(10.)),
            )
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
            .children(device_menu)
            .children(owner.update(cx, |app, cx| app.screen_privacy_overlay(false, cx)))
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "call_panel_tests.rs"]
mod tests;
