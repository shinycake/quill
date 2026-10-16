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
//! The window itself is `group_call_window.rs`; a live stream (RTMP) gets
//! the wide layout of `group_call_stream_layout.rs` instead of this one.
//! The window renders a body the app builds (elements are window-agnostic),
//! so every existing action and helper works from here.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::format_helpers::format_starts_in;
use super::nested_click::SwallowPress;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::ActiveGroupCall;

use std::time::Duration;

/// lib_ui `colors.palette`.
pub(super) const BG: u32 = 0x1a2026;
pub(super) const MEMBERS_BG: u32 = 0x2c333d;
pub(super) const MEMBERS_BG_OVER: u32 = 0x323a45;
const ACTIVE_FG: u32 = 0x4db8ff;
pub(super) const MEMBER_ACTIVE: u32 = 0x8deb90;
pub(super) const MEMBER_INACTIVE_ICON: u32 = 0x84888f;
pub(super) const MEMBER_INACTIVE_STATUS: u32 = 0x61c0ff;
pub(super) const MEMBER_MUTED_ICON: u32 = 0xed7372;
pub(super) const MEMBER_NOT_JOINED: u32 = 0x91979e;
pub(super) const LEAVE_BG: u32 = 0xf75c5c7f;

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
    let Some(me) = me.filter(|_| !call.reconnecting) else {
        return MuteState::Connecting;
    };
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

        let can_invite = call.is_joined && self.group_call_invite_row(&call).is_some();
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
                .on_click(cx.listener(|this, _, _, cx| this.group_call_invite_row_clicked(cx)))
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

        let invite = (self.group_call.invite_open && !self.account.passcode.locked).then(|| {
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
        let rename = self.group_call_rename_overlay("Edit Title".into(), cx);
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
            .children(self.group_call_share_overlay(&call, cx))
            .children(rename)
            .children(self.group_call_settings_overlay(cx))
            .children(self.screen_privacy_overlay(true, cx))
            .into_any_element()
    }

    /// The rename box (`setVideoChatTitle`) over the call window, with
    /// `heading` on top ("Edit live stream title" for a channel).
    pub(super) fn group_call_rename_overlay(
        &self,
        heading: SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.group_call.title_dialog.as_ref()?;
        Some(
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
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(heading))
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
                .into_any_element(),
        )
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

    /// The main window's bar for a joined voice chat: mute, the chat's
    /// title and count, leave; a click brings the window back.
    pub(super) fn group_call_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let call = self.session()?.calls.active_group_call.as_ref()?;
        // A live story plays in the story viewer, without the call bar.
        if !call.is_joined || call.is_live_story {
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
        // A live stream's viewer has no microphone (tdesktop's top bar
        // hides the mute button for RTMP); a spacer keeps the title
        // centered.
        let stream = super::group_call_stream_layout::uses_stream_layout(call);
        let count = if stream {
            let viewers = i64::from(call.viewer_count.max(1));
            quill::lang::trn_with(
                quill::lang::keys::GROUP_CALL_RTMP_VIEWERS,
                viewers,
                &[("count", &quill::premium_hub::group_digits(viewers))],
            )
        } else {
            format!("{} participants", call.participant_count)
        };
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
                .when(stream, |this| this.child(div().w(px(41.)).flex_none()))
                .when(!stream, |this| {
                    this.child(
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
                })
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
                        .child(div().opacity(0.85).child(count)),
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
