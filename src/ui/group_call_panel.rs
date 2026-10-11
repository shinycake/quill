//! The voice / video chat window (tdesktop `Calls::Group::Panel`): a dark
//! 380 × 520 window of its own (wider when there's video).
//! - The title and participant count sit at the top.
//! - The members list sits on #2c333d: avatars, "speaking" in green,
//!   "listening" in blue, mic icons. Click a member for their actions.
//! - Video tiles go above the list.
//! - Along the bottom: Video, the big mute button and Leave. The mute
//!   button is a gradient disc that breathes while you're live: green
//!   when live, blue when muted, purple-red when an admin muted you.
//!
//! The window renders a body the app builds (elements are window-agnostic),
//! so every existing action and helper works from here.

use super::app::QuillApp;
use super::format_helpers::format_starts_in;
use super::nested_click::SwallowPress;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, Root, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::ActiveGroupCall;

use std::time::Duration;

/// lib_ui `colors.palette`.
const BG: u32 = 0x1a2026;
pub(super) const MEMBERS_BG: u32 = 0x2c333d;
pub(super) const MEMBERS_BG_OVER: u32 = 0x323a45;
const ACTIVE_FG: u32 = 0x4db8ff;
pub(super) const MEMBER_ACTIVE: u32 = 0x8deb90;
pub(super) const MEMBER_INACTIVE_ICON: u32 = 0x84888f;
pub(super) const MEMBER_INACTIVE_STATUS: u32 = 0x61c0ff;
pub(super) const MEMBER_MUTED_ICON: u32 = 0xed7372;
pub(super) const MEMBER_NOT_JOINED: u32 = 0x91979e;
const LEAVE_BG: u32 = 0xf75c5c7f;

/// A member menu entry's effect.
pub(super) type MemberAction = Box<dyn Fn(&mut QuillApp, &mut Context<QuillApp>)>;

/// What the big button says and does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MuteState {
    /// Not in the call yet.
    Join,
    Connecting,
    Live,
    Muted,
    /// An admin muted you; a click raises your hand.
    ForceMuted,
    HandRaised,
    /// A scheduled chat that hasn't started.
    Scheduled,
}

impl MuteState {
    /// tdesktop's button gradients (`groupCallLive*`, `groupCallMuted*`,
    /// `groupCallForceMuted*`).
    fn colors(self) -> (u32, u32) {
        match self {
            MuteState::Live => (0x0dcc39, 0x0bb6bd),
            MuteState::Muted | MuteState::Join | MuteState::Scheduled => (0x0992ef, 0x16ccfb),
            MuteState::ForceMuted | MuteState::HandRaised => (0x9b52e9, 0xeb5353),
            MuteState::Connecting => (0x5b6570, 0x6f7a86),
        }
    }

    fn label(self) -> &'static str {
        match self {
            MuteState::Join => "Join",
            MuteState::Connecting => "Connecting...",
            MuteState::Live => "You are Live",
            MuteState::Muted => "Unmute",
            MuteState::ForceMuted => "Muted by admin",
            MuteState::HandRaised => "You asked to speak",
            MuteState::Scheduled => "Starts soon",
        }
    }

    fn sublabel(self) -> Option<&'static str> {
        match self {
            MuteState::ForceMuted => Some("You are in Listen Only mode"),
            MuteState::HandRaised => Some("We let the speakers know"),
            _ => None,
        }
    }

    fn icon(self) -> IconName {
        match self {
            MuteState::Live => IconName::Mic,
            MuteState::ForceMuted | MuteState::HandRaised => IconName::Hand,
            MuteState::Join | MuteState::Scheduled => IconName::Phone,
            _ => IconName::MicOff,
        }
    }
}

fn mute_state(call: &ActiveGroupCall) -> MuteState {
    if call.scheduled_start_date > 0 && !call.is_joined {
        return MuteState::Scheduled;
    }
    if !call.is_joined {
        return MuteState::Join;
    }
    let me = call.participants.iter().find(|p| p.is_current_user);
    if call.reconnecting || me.is_none() {
        return MuteState::Connecting;
    }
    let me = me.expect("checked");
    if me.is_muted_for_all_users && !me.can_unmute_self {
        return if me.is_hand_raised {
            MuteState::HandRaised
        } else {
            MuteState::ForceMuted
        };
    }
    if call.is_muted_self {
        MuteState::Muted
    } else {
        MuteState::Live
    }
}

impl QuillApp {
    /// The group call window's content, built with the app's context.
    pub(super) fn group_call_panel_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(call) = self
            .session()
            .and_then(|s| s.calls.active_group_call.clone())
        else {
            return div().size_full().bg(rgb(BG)).into_any_element();
        };
        let state = mute_state(&call);
        // Your own microphone: the halo follows the level while live.
        let (self_level, self_speaking) = self.group_call_self_level();
        let level_reach = (state == MuteState::Live).then(|| {
            let now = self.group_call_now_ms();
            if (self_level - self.group_call.level_seen).abs() > f32::EPSILON {
                self.group_call.level_seen = self_level;
                self.group_call
                    .level_anim
                    .retarget(self_level.clamp(0.0, 1.0), now);
            }
            let value = self.group_call.level_anim.value(now);
            if self.group_call.level_anim.is_running(now) || value > 0.0 {
                self.request_animation_tick(30, cx);
            }
            quill::calls::audio_level::halo_reach(value)
        });
        let title = if call.title.is_empty() {
            if call.is_video_chat {
                "Video Chat"
            } else {
                "Voice Chat"
            }
            .to_string()
        } else {
            call.title.clone()
        };
        let mut subtitle = match call.participant_count {
            1 => "1 participant".to_string(),
            n => format!("{n} participants"),
        };
        if call.record_duration > 0 {
            subtitle.push_str(&format!(
                " · Recording {}",
                super::format_helpers::format_record_duration(call.record_duration)
            ));
        }
        if state == MuteState::Scheduled {
            subtitle = format!(
                "Starts {}",
                format_starts_in(call.scheduled_start_date as i64)
            );
        }

        // Video tiles: every stream, yours included; one can be pinned.
        let tiles = self.group_call_tiles(&call, cx);

        let can_invite = call.is_joined;
        let invite_row = can_invite.then(|| {
            div()
                .id("group-call-invite")
                .h(px(48.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(14.))
                .cursor_pointer()
                .text_color(rgb(ACTIVE_FG))
                .hover(|style| style.bg(rgb(MEMBERS_BG_OVER)))
                .on_click(cx.listener(|this, _, _, cx| this.open_group_call_invite(cx)))
                .child(
                    div()
                        .size(px(40.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::UserPlus).with_size(px(20.))),
                )
                .child(div().text_size(px(14.)).child("Invite Members"))
        });

        let rows: Vec<AnyElement> = call
            .participants
            .iter()
            .map(|participant| self.group_member_row(&call, participant, self_speaking, cx))
            .collect();

        let members = div()
            .id("group-call-members")
            .flex_1()
            .min_h_0()
            .mx(px(16.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(tiles)
            .child(
                div()
                    .rounded(px(12.))
                    .bg(rgb(MEMBERS_BG))
                    .overflow_hidden()
                    .py(px(4.))
                    .children(invite_row)
                    .children(rows)
                    .when(
                        !call.loaded_all_participants && call.participants.len() >= 100,
                        |this| {
                            this.child(
                                div()
                                    .id("group-call-more")
                                    .h(px(40.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .text_color(rgb(ACTIVE_FG))
                                    .text_size(px(13.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.load_more_group_call_participants(cx)
                                    }))
                                    .child("Show more"),
                            )
                        },
                    ),
            )
            .when(self.group_call.chat_shown, |this| {
                this.child(
                    self.group_call_messages_section(div().id("group-call-chat"), &call, cx)
                        .rounded(px(12.))
                        .bg(rgb(MEMBERS_BG))
                        .p(px(10.)),
                )
            });

        let header = div()
            .relative()
            .pt(px(10.))
            .pb(px(10.))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(white())
                    .max_w(px(260.))
                    .truncate()
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(rgb(MEMBER_NOT_JOINED))
                    .child(subtitle),
            )
            .child(
                div()
                    .absolute()
                    .right(px(10.))
                    .top(px(10.))
                    .child(self.group_call_menu(&call, cx)),
            );

        // A request that failed (invite, ban, volume, rejoin…).
        let error = self
            .session()
            .and_then(|s| s.calls.group_call_error.clone())
            .map(|error| {
                div()
                    .id("group-call-error")
                    .mx(px(16.))
                    .mb(px(8.))
                    .p(px(10.))
                    .rounded(px(10.))
                    .bg(rgba(0xeb535333))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_group_call_error(cx)))
                    .child(error)
            });
        let banner = call.reconnecting.then(|| {
            div()
                .id("group-call-rejoin")
                .mx(px(16.))
                .mb(px(8.))
                .p(px(10.))
                .rounded(px(10.))
                .bg(rgba(0xeb535333))
                .text_size(px(13.))
                .text_color(white())
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.rejoin_active_group_call(cx)))
                .child("Connection lost. Click to rejoin.")
        });

        let invite = self.group_call.invite_open.then(|| {
            div()
                .absolute()
                .inset_0()
                .bg(hsla(0., 0., 0., 0.5))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(340.))
                        .max_h(relative(0.85))
                        .rounded(px(12.))
                        .bg(cx.theme().background)
                        .p(px(12.))
                        .child(self.group_call_invite_panel(&call, cx)),
                )
        });

        // Rename (`setVideoChatTitle`).
        let rename = self.group_call.title_dialog.as_ref().map(|dialog| {
            div()
                .absolute()
                .inset_0()
                .bg(hsla(0., 0., 0., 0.5))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(320.))
                        .rounded(px(12.))
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .p(px(16.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Edit Title"))
                        .child(gpui_kit::component::input::Textarea::new(
                            &dialog.title_input,
                        ))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    Button::new("group-call-title-cancel")
                                        .label("Cancel")
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_group_call_title_dialog(cx)
                                        })),
                                )
                                .child(
                                    Button::new("group-call-title-save")
                                        .label("Save")
                                        .primary()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.save_group_call_title(cx)
                                        })),
                                ),
                        ),
                )
        });
        // Streaming with an external app: the server URL and key.
        let stream = call.rtmp_url.clone().map(|url| {
            let key = call.rtmp_stream_key.clone().unwrap_or_default();
            let copy = |id: &'static str, label: &'static str, value: String| {
                Button::new(id)
                    .label(label)
                    .ghost()
                    .xsmall()
                    .text_color(rgb(ACTIVE_FG))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.copy_rtmp_value(label, &value, cx)),
                    )
            };
            div()
                .mx(px(16.))
                .mb(px(8.))
                .p(px(10.))
                .rounded(px(10.))
                .bg(rgb(MEMBERS_BG))
                .text_size(px(12.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Stream with another app"),
                )
                .child(
                    div()
                        .text_color(rgb(MEMBER_NOT_JOINED))
                        .truncate()
                        .child(url.clone()),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(copy("group-call-rtmp-url", "Copy Server URL", url))
                        .child(copy("group-call-rtmp-key", "Copy Stream Key", key))
                        .child(
                            Button::new("group-call-rtmp-revoke")
                                .label("Revoke Key")
                                .ghost()
                                .xsmall()
                                .text_color(rgb(MEMBER_MUTED_ICON))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.replace_video_chat_rtmp_url(cx)
                                })),
                        ),
                )
        });
        div()
            .id("group-call-panel")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(white())
            .child(header)
            .children(banner)
            .children(error)
            .children(stream)
            .children(self.group_call_join_as_row(&call, cx))
            .child(members)
            .child(self.group_call_controls(&call, state, level_reach, cx))
            .children(invite)
            .children(rename)
            .into_any_element()
    }

    /// Video · the big mute button · Leave.
    fn group_call_controls(
        &self,
        call: &ActiveGroupCall,
        state: MuteState,
        level_reach: Option<f32>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (from, to) = state.colors();
        let joined = call.is_joined;
        let side = |id: &'static str, icon: IconName, label: &'static str, bg: Hsla| {
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
                .child(
                    div()
                        .size(px(44.))
                        .rounded_full()
                        .bg(bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|style| style.opacity(0.85))
                        .child(Icon::new(icon).with_size(px(22.)).text_color(white())),
                )
                .child(div().text_size(px(11.)).child(label))
        };
        let video_on = call.is_my_video_enabled;
        let left = (joined && call.can_enable_video).then(|| {
            side(
                "group-call-video",
                if video_on {
                    IconName::Video
                } else {
                    IconName::VideoOff
                },
                "Video",
                if video_on {
                    hsla(0., 0., 1., 0.12)
                } else {
                    hsla(0., 0., 1., 0.9)
                },
            )
            .on_click(cx.listener(|this, _, _, cx| this.toggle_group_call_video(cx)))
        });
        let leave = side(
            "group-call-leave",
            IconName::Close,
            if joined { "Leave" } else { "Close" },
            rgba(LEAVE_BG).into(),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if joined {
                this.leave_active_group_call(cx);
            } else {
                this.close_group_call_window(cx);
            }
        }));
        let scheduled_admin = state == MuteState::Scheduled && call.can_be_managed;
        let notify_on = call.enabled_start_notification;
        let mute = div()
            .id("group-call-mute")
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .role(Role::Button)
            .aria_label(state.label())
            .on_click(cx.listener(move |this, _, _, cx| match state {
                MuteState::Join => this.join_active_group_call(cx),
                MuteState::Live | MuteState::Muted => this.toggle_group_call_self_mute(cx),
                MuteState::ForceMuted => this.toggle_group_call_self_hand(true, cx),
                MuteState::Scheduled if scheduled_admin => this.start_scheduled_video_chat(cx),
                MuteState::Scheduled => this.toggle_video_chat_start_notification(cx),
                _ => {}
            }))
            .child(
                div()
                    .relative()
                    .size(px(104.))
                    .flex()
                    .items_center()
                    .justify_center()
                    // Your voice: a ring that grows with the level
                    // (tdesktop's blobs follow `setLevel`).
                    .children(level_reach.map(|reach| {
                        div()
                            .absolute()
                            .inset(px(-reach))
                            .rounded_full()
                            .bg(rgb(to))
                            .opacity(0.22)
                    }))
                    // The breathing halo (tdesktop's blobs).
                    .child({
                        let halo = div().absolute().inset_0().rounded_full().bg(rgb(to));
                        // Battery and animations: a still halo.
                        if quill::power_saving::on(quill::power_saving::Flag::Calls) {
                            halo.opacity(0.24).into_any_element()
                        } else {
                            halo.with_animation(
                                "group-mute-halo",
                                Animation::new(Duration::from_millis(
                                    if state == MuteState::Live { 1100 } else { 2600 },
                                ))
                                .repeat()
                                .with_easing(pulsating_between(0.0, 1.0)),
                                move |halo, delta| {
                                    let reach = if state == MuteState::Live { 14. } else { 6. };
                                    halo.opacity(0.18 + 0.12 * delta).inset(px(-reach * delta))
                                },
                            )
                            .into_any_element()
                        }
                    })
                    .child(
                        div()
                            .size(px(84.))
                            .rounded_full()
                            .bg(linear_gradient(
                                135.,
                                linear_color_stop(rgb(from), 0.),
                                linear_color_stop(rgb(to), 1.),
                            ))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(state.icon())
                                    .with_size(px(36.))
                                    .text_color(white()),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match state {
                        MuteState::Scheduled if scheduled_admin => "Start Now",
                        MuteState::Scheduled if notify_on => "Cancel Reminder",
                        MuteState::Scheduled => "Notify Me",
                        _ => state.label(),
                    }),
            )
            .children(state.sublabel().map(|sub| {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MEMBER_NOT_JOINED))
                    .child(sub)
            }))
            .children(
                matches!(state, MuteState::Live | MuteState::Muted)
                    .then(|| self.ptt_hint())
                    .flatten()
                    .map(|hint| {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MEMBER_NOT_JOINED))
                            .child(hint)
                    }),
            );
        div()
            .flex_none()
            .h(px(190.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(36.))
            .child(div().w(px(68.)).children(left))
            .child(mute)
            .child(leave)
    }

    /// Close the window of a call you haven't joined.
    pub(super) fn close_group_call_window(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map(|c| c.id)
        {
            self.group_call.window_closed_by_user = Some(id);
        }
        cx.notify();
    }

    /// Open or close the group call window with the tracked group call.
    pub(super) fn sync_group_call_window(&mut self, cx: &mut Context<Self>) {
        self.sync_global_ptt(cx);
        let wanted = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map(|call| call.id)
            .filter(|id| self.group_call.window_closed_by_user != Some(*id));
        self.prune_group_video_images(wanted);
        match (wanted, self.group_call.window) {
            (Some(_), None) => self.open_group_call_window(cx),
            (None, Some(handle)) => {
                self.group_call.window = None;
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            _ => {}
        }
    }

    fn open_group_call_window(&mut self, cx: &mut Context<Self>) {
        if self.group_call.window_opening {
            return;
        }
        self.group_call.window_opening = true;
        let owner = cx.entity();
        cx.defer(move |cx| {
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let bounds = Bounds::centered(None, size(px(420.), px(640.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(380.), px(520.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Video Chat".into()),
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
                            app.group_call.window = None;
                            app.group_call.window_closed_by_user = app
                                .session()
                                .and_then(|s| s.calls.active_group_call.as_ref())
                                .map(|c| c.id);
                            cx.notify();
                        });
                        true
                    });
                    let view = cx.new(|cx| {
                        let activation_owner = render_owner.downgrade();
                        GroupCallPanel {
                            owner: render_owner.downgrade(),
                            _observe: cx.observe(&render_owner, |_, _, cx| cx.notify()),
                            focus: cx.focus_handle(),
                            // A key-up can't arrive once the window is in
                            // the background: close the push-to-talk mic.
                            _activation: cx.observe_window_activation(
                                window,
                                move |_, window, cx| {
                                    if !window.is_window_active() {
                                        let _ = activation_owner.update(cx, |app, cx| {
                                            app.group_call_ptt_release_all(cx)
                                        });
                                    }
                                },
                            ),
                            pinned_on_top: false,
                        }
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            owner.update(cx, |app, cx| {
                app.group_call.window_opening = false;
                match result {
                    Ok(handle) => app.group_call.window = Some(handle.into()),
                    Err(_) => {
                        app.connection.status_note = "Couldn't open the video chat window".into()
                    }
                }
                cx.notify();
            });
        });
    }

    /// Bring the group call window back (the call bar).
    pub(super) fn show_group_call_window(&mut self, cx: &mut Context<Self>) {
        self.group_call.window_closed_by_user = None;
        if let Some(handle) = self.group_call.window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
        cx.notify();
    }

    /// The main window's bar for a joined voice chat: mute, the chat's
    /// title and count, leave; a click brings the window back.
    pub(super) fn group_call_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let call = self.session()?.calls.active_group_call.as_ref()?;
        if !call.is_joined {
            return None;
        }
        let state = mute_state(call);
        let (from, to) = state.colors();
        let title = if call.title.is_empty() {
            "Video Chat".to_string()
        } else {
            call.title.clone()
        };
        let muted = call.is_muted_self;
        Some(
            div()
                .id("group-call-bar")
                .h(px(38.))
                .flex_none()
                .flex()
                .items_center()
                .text_color(white())
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(rgb(from), 0.),
                    linear_color_stop(rgb(to), 1.),
                ))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.show_group_call_window(cx)))
                .child(
                    div()
                        .id("group-call-bar-mute")
                        .w(px(41.))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|style| style.bg(hsla(0., 0., 1., 0.12)))
                        .swallow_press()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_group_call_self_mute(cx);
                        }))
                        .child(
                            Icon::new(if muted {
                                IconName::MicOff
                            } else {
                                IconName::Mic
                            })
                            .with_size(px(18.)),
                        ),
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
                                .child(title),
                        )
                        .child(
                            div()
                                .opacity(0.85)
                                .child(format!("{} participants", call.participant_count)),
                        ),
                )
                .child(
                    div()
                        .id("group-call-bar-leave")
                        .w(px(41.))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|style| style.bg(hsla(0., 0., 1., 0.12)))
                        .swallow_press()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.leave_active_group_call(cx);
                        }))
                        .child(Icon::new(IconName::Close).with_size(px(18.))),
                )
                .into_any_element(),
        )
    }
}

struct GroupCallPanel {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
    /// Takes keyboard focus so push-to-talk key events reach the window.
    focus: FocusHandle,
    _activation: Subscription,
    /// The window stays above the others (tdesktop "pin on top").
    pinned_on_top: bool,
}

impl Render for GroupCallPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        if self.focus.is_focused(window) || window.focused(cx).is_none() {
            self.focus.focus(window, cx);
        }
        let fullscreen = window.is_fullscreen();
        let body = owner.update(cx, |app, cx| {
            let wants_stage = fullscreen || app.demo_ui.group_stage;
            let call = app
                .session()
                .and_then(|s| s.calls.active_group_call.clone());
            let stage = call
                .filter(|_| wants_stage)
                .and_then(|call| app.group_call_stage(&call, cx));
            match stage {
                Some(stage) => stage,
                None => app.group_call_panel_body(cx),
            }
        });
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let key = event.keystroke.key.clone();
                if quill::calls::tile_pin::exits_fullscreen(&key) && window.is_fullscreen() {
                    owner.update(cx, |app, cx| app.leave_group_call_stage(window, cx));
                    cx.stop_propagation();
                    return;
                }
                if owner.update(cx, |app, cx| app.group_call_key_down(&key, cx)) {
                    cx.stop_propagation();
                }
            }))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| {
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let key = event.keystroke.key.clone();
                if owner.update(cx, |app, cx| app.group_call_key_up(&key, cx)) {
                    cx.stop_propagation();
                }
            }))
            .child(body)
            // tdesktop's pin-on-top control; the full-screen stage has no
            // window chrome to pin.
            .when(!fullscreen, |this| {
                this.relative().child(
                    super::window_control::pin_on_top_button(
                        "group-call-pin-on-top",
                        self.pinned_on_top,
                        cx,
                        |this, pinned, _, cx| {
                            this.pinned_on_top = pinned;
                            cx.notify();
                        },
                    )
                    .text_color(white()),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{MuteState, mute_state};
    use crate::ui::group_call_members::member_status;
    use quill::state::ActiveGroupCall;
    use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};

    fn me(muted_by_admin: bool, can_unmute: bool, hand: bool) -> ParsedGroupCallParticipant {
        ParsedGroupCallParticipant {
            participant_id: MessageSender::User { user_id: 1 },
            audio_source_id: 0,
            is_current_user: true,
            is_speaking: false,
            is_hand_raised: hand,
            can_be_muted_for_all_users: false,
            can_be_unmuted_for_all_users: false,
            can_be_muted_for_current_user: false,
            can_be_unmuted_for_current_user: false,
            is_muted_for_all_users: muted_by_admin,
            is_muted_for_current_user: false,
            can_unmute_self: can_unmute,
            volume_level: 10000,
            order: String::new(),
            video_enabled: false,
            screen_sharing_enabled: false,
            video_info: None,
            screen_sharing_video_info: None,
        }
    }

    #[test]
    fn the_big_button_follows_tdesktop_states() {
        let mut call = ActiveGroupCall::fresh(1);
        assert!(mute_state(&call) == MuteState::Join);
        call.is_joined = true;
        assert!(mute_state(&call) == MuteState::Connecting);
        call.participants.push(me(false, true, false));
        assert!(mute_state(&call) == MuteState::Live);
        call.is_muted_self = true;
        assert!(mute_state(&call) == MuteState::Muted);
        call.participants[0] = me(true, false, false);
        assert!(mute_state(&call) == MuteState::ForceMuted);
        call.participants[0] = me(true, false, true);
        assert!(mute_state(&call) == MuteState::HandRaised);
        call.is_joined = false;
        call.scheduled_start_date = 2_000_000_000;
        assert!(mute_state(&call) == MuteState::Scheduled);
    }

    #[test]
    fn member_statuses_read_like_tdesktop() {
        let mut p = me(false, true, false);
        assert_eq!(member_status(&p, p.is_speaking).0, "listening");
        p.is_hand_raised = true;
        assert_eq!(member_status(&p, p.is_speaking).0, "wants to speak");
        p.is_speaking = true;
        assert_eq!(member_status(&p, p.is_speaking).0, "speaking");
    }
}
