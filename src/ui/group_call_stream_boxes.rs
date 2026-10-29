//! The live stream window's menu and boxes, as tdesktop builds them for
//! an RTMP call:
//!
//! - The "…" menu for managers (`FillMenu`, calls_group_menu.cpp): edit
//!   the title (channel wording), start or stop recording, Settings, and
//!   "End Live Stream". Screen sharing is left out: an RTMP connection
//!   cannot capture, which is tdesktop's `videoIsWorking` being false.
//!   Viewers get the Settings button instead.
//! - Settings (`SettingsBox`, calls_group_settings.cpp, RTMP branch):
//!   Speakers, Share Invite Link for a channel with a username, the
//!   Server URL and Stream Key with their copy buttons and the "…" Revoke
//!   for managers once the URL is known, mute for me, and End Live Stream
//!   for managers. tdesktop's volume slider here is the bar's slider in
//!   Quill (`calls::stream_volume`).
//! - The Stream Key is hidden until a click and a warning, as tdesktop's
//!   spoiler (`FillRtmpRows`).
//! - Leave (`LeaveBox`): managers choose between leaving and ending the
//!   stream; viewers leave at once (`Panel::endCall`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::call_panel_devices::DeviceMenuTarget;
use super::dialogs::rtmp_stream::{RtmpRows, rtmp_info_rows};
use super::group_call_stream_layout::{StreamFacts, leave_stream_full};
use super::video_player::controls::bar_disc;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::{ActiveTheme, Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::MediaDeviceKind;
use quill::lang::{keys, tr, tr_with};

/// The box over the stream window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamOverlay {
    Settings,
    /// The Speakers list (`ChoosePlaybackDeviceBox`).
    Speakers,
    /// Leave or end; `end` starts with "End live stream" ticked.
    Leave {
        end: bool,
    },
    RevokeKey,
    KeyWarning,
}

impl QuillApp {
    pub(super) fn open_stream_overlay(&mut self, overlay: StreamOverlay, cx: &mut Context<Self>) {
        self.group_call.stream.overlay = Some(overlay);
        if overlay == StreamOverlay::Speakers
            && let Some(live) = self.live.as_mut()
        {
            live.driver.refresh_call_devices();
        }
        cx.notify();
    }

    pub(super) fn close_stream_overlay(&mut self, cx: &mut Context<Self>) {
        self.group_call.stream.overlay = None;
        cx.notify();
    }

    /// The bar's left button: the managers' "…" menu, or Settings.
    pub(super) fn group_call_stream_menu(
        &self,
        facts: &StreamFacts,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disc = bar_disc;
        if !facts.manager {
            return Button::new("group-call-stream-settings")
                .custom(disc(cx))
                .icon(Icon::new(IconName::Settings).with_size(px(22.)))
                .rounded(px(22.))
                .size(px(44.))
                .tooltip(tr(keys::GROUP_CALL_SETTINGS))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_stream_overlay(StreamOverlay::Settings, cx)
                }))
                .into_any_element();
        }
        let owner = cx.entity().downgrade();
        let recording = facts.recording;
        Button::new("group-call-stream-menu")
            .custom(disc(cx))
            .icon(Icon::new(IconName::EllipsisVertical).with_size(px(22.)))
            .rounded(px(22.))
            .size(px(44.))
            .tooltip(tr(keys::PROFILE_ACTION_SHORT_MORE))
            .accessibility_label(tr(keys::PROFILE_ACTION_SHORT_MORE))
            .dropdown_menu(move |menu, _, _| {
                let item = |label: &str,
                            action: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
                    let owner = owner.clone();
                    PopupMenuItem::new(label.to_string()).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| action(this, window, cx));
                    })
                };
                menu.item(item(
                    tr(keys::GROUP_CALL_EDIT_TITLE_CHANNEL),
                    |this, window, cx| this.open_group_call_title_dialog(window, cx),
                ))
                .item(item(
                    if recording {
                        tr(keys::GROUP_CALL_RECORDING_STOP)
                    } else {
                        tr(keys::GROUP_CALL_RECORDING_START)
                    },
                    |this, _, cx| this.toggle_group_call_recording(cx),
                ))
                .item(item(tr(keys::GROUP_CALL_SETTINGS), |this, _, cx| {
                    this.open_stream_overlay(StreamOverlay::Settings, cx)
                }))
                .item(item(tr(keys::GROUP_CALL_END_CHANNEL), |this, _, cx| {
                    this.open_stream_overlay(StreamOverlay::Leave { end: true }, cx)
                }))
            })
            .into_any_element()
    }

    /// Leave (`Panel::endCall`): a viewer leaves, a manager is asked
    /// whether to end the stream too, and an unjoined window just closes.
    pub(super) fn stream_leave_clicked(
        &mut self,
        facts: &StreamFacts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !facts.joined {
            leave_stream_full(window);
            self.close_group_call_window(cx);
        } else if facts.manager {
            self.open_stream_overlay(StreamOverlay::Leave { end: false }, cx);
        } else {
            leave_stream_full(window);
            self.leave_active_group_call(cx);
        }
    }

    /// The open box, drawn over the window.
    pub(super) fn group_call_stream_overlay(
        &mut self,
        facts: &StreamFacts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let overlay = self.group_call.stream.overlay?;
        let content = match overlay {
            StreamOverlay::Settings => self.stream_settings_box(facts, cx),
            StreamOverlay::Speakers => self.stream_speakers_box(cx),
            StreamOverlay::Leave { end } => self.stream_leave_box(facts, end, cx),
            StreamOverlay::RevokeKey => confirm_box(
                tr(keys::GROUP_CALL_RTMP_REVOKE_SURE).into(),
                tr(keys::GROUP_INVITE_CONTEXT_REVOKE),
                false,
                cx,
                |this, cx| {
                    this.replace_video_chat_rtmp_url(cx);
                    this.open_stream_overlay(StreamOverlay::Settings, cx);
                },
            ),
            StreamOverlay::KeyWarning => confirm_box(
                bold(tr(keys::GROUP_CALL_RTMP_KEY_WARNING)),
                tr(keys::FROM_REQUEST_UNDERSTAND),
                true,
                cx,
                |this, cx| {
                    this.group_call.stream.key_shown = true;
                    this.open_stream_overlay(StreamOverlay::Settings, cx);
                },
            ),
        };
        Some(
            div()
                .id("group-call-stream-overlay")
                .absolute()
                .inset_0()
                .bg(hsla(0., 0., 0., 0.5))
                .flex()
                .items_center()
                .justify_center()
                .on_click(cx.listener(|this, _, _, cx| this.close_stream_overlay(cx)))
                .child(
                    div()
                        .id("group-call-stream-box")
                        .debug_selector(|| "group-call-stream-box".into())
                        .w(px(340.))
                        .max_h(relative(0.9))
                        .overflow_y_scroll()
                        .rounded(px(12.))
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .p(px(16.))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        // Clicks inside the box stay in it.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(content),
                )
                .into_any_element(),
        )
    }

    fn stream_settings_box(&mut self, facts: &StreamFacts, cx: &mut Context<Self>) -> AnyElement {
        let speaker = self
            .call_device_sections(DeviceMenuTarget::Audio)
            .into_iter()
            .find(|section| section.kind == MediaDeviceKind::Speaker)
            .and_then(|section| section.items.into_iter().find(|item| item.selected))
            .map(|item| item.name)
            .unwrap_or_else(|| tr(keys::SETTINGS_CALL_DEVICE_DEFAULT).to_string());
        let muted = self.stream_muted();
        let row = |id: &'static str, label: SharedString| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .px(px(8.))
                .py(px(8.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|style| style.bg(hsla(0., 0., 0.5, 0.12)))
                .child(div().child(label))
        };
        let rtmp = (facts.manager).then(|| self.stream_rtmp_rows(cx)).flatten();
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(heading(tr(keys::GROUP_CALL_SETTINGS_TITLE)))
            .child(
                row(
                    "group-call-stream-speakers",
                    tr(keys::GROUP_CALL_SPEAKERS).into(),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .child(speaker),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_stream_overlay(StreamOverlay::Speakers, cx)
                })),
            )
            .child(
                row(
                    "group-call-stream-mute-for-me",
                    if muted {
                        tr(keys::GROUP_CALL_CONTEXT_UNMUTE_FOR_ME)
                    } else {
                        tr(keys::GROUP_CALL_CONTEXT_MUTE_FOR_ME)
                    }
                    .into(),
                )
                .child(
                    Icon::new(if muted {
                        IconName::VolumeX
                    } else {
                        IconName::Volume2
                    })
                    .with_size(px(18.)),
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.toggle_stream_playback_muted(!muted, cx)
                    }),
                ),
            )
            .when(facts.share_link, |this| {
                this.child(
                    row("group-call-stream-share", tr(keys::GROUP_CALL_SHARE).into()).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.group_call.stream.overlay = None;
                            this.open_group_call_share_links(cx);
                        }),
                    ),
                )
            })
            .children(rtmp)
            .when(facts.manager, |this| {
                this.child(
                    row(
                        "group-call-stream-end",
                        tr(keys::GROUP_CALL_END_CHANNEL).into(),
                    )
                    .text_color(cx.theme().danger)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_stream_overlay(StreamOverlay::Leave { end: true }, cx)
                    })),
                )
            })
            .child(
                div().flex().justify_end().pt(px(6.)).child(
                    Button::new("group-call-stream-settings-done")
                        .label(tr(keys::BOX_DONE))
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.close_stream_overlay(cx))),
                ),
            )
            .into_any_element()
    }

    /// Server URL and Stream Key for managers, once the URL is known.
    fn stream_rtmp_rows(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let call = self.session()?.calls.active_group_call.as_ref()?;
        let url = call.rtmp_url.clone().filter(|url| !url.is_empty())?;
        let key = call.rtmp_stream_key.clone().unwrap_or_default();
        let shown = self.group_call.stream.key_shown;
        Some(rtmp_info_rows(
            RtmpRows {
                prefix: "group-call-stream",
                url,
                key,
                key_shown: shown,
                revoke_label: tr(keys::GROUP_CALL_RTMP_REVOKE),
            },
            cx,
            |this, cx| this.open_stream_overlay(StreamOverlay::KeyWarning, cx),
            |this, cx| this.open_stream_overlay(StreamOverlay::RevokeKey, cx),
        ))
    }

    fn stream_speakers_box(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let speakers = self
            .call_device_sections(DeviceMenuTarget::Audio)
            .into_iter()
            .find(|section| section.kind == MediaDeviceKind::Speaker)
            .map(|section| section.items)
            .unwrap_or_default();
        let any_selected = speakers.iter().any(|item| item.selected);
        let mut list = div().flex().flex_col().gap(px(2.));
        let choice = |id: SharedString, name: String, checked: bool| {
            div()
                .id(id.clone())
                .flex()
                .items_center()
                .gap(px(10.))
                .py(px(6.))
                .cursor_pointer()
                .child(
                    Radio::new(id)
                        .checked(checked)
                        .tab_stop(false)
                        .accessibility_label(name.clone()),
                )
                .child(div().min_w_0().truncate().child(name))
        };
        list = list.child(
            choice(
                "group-call-stream-speaker-default".into(),
                tr(keys::SETTINGS_CALL_DEVICE_DEFAULT).to_string(),
                !any_selected,
            )
            .on_click(cx.listener(|this, _, _, cx| this.pick_stream_speaker(None, cx))),
        );
        for (index, item) in speakers.into_iter().enumerate() {
            let id = item.id.clone();
            list =
                list.child(
                    choice(
                        format!("group-call-stream-speaker-{index}").into(),
                        item.name,
                        item.selected,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_stream_speaker(Some(id.clone()), cx)
                    })),
                );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(heading(tr(keys::GROUP_CALL_SPEAKERS)))
            .child(list)
            .child(
                div().flex().justify_end().child(
                    Button::new("group-call-stream-speakers-back")
                        .label(tr(keys::BOX_DONE))
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_stream_overlay(StreamOverlay::Settings, cx)
                        })),
                ),
            )
            .into_any_element()
    }

    /// A Speakers pick (`setCallPlaybackDeviceId`): the call plays there
    /// at once. `None` is the system default.
    fn pick_stream_speaker(&mut self, device: Option<String>, cx: &mut Context<Self>) {
        match (device, self.live.as_mut()) {
            (Some(id), _) => self.select_call_device(MediaDeviceKind::Speaker, &id, cx),
            (None, Some(live)) => {
                let microphone = live.driver.selected_call_devices().0.map(str::to_owned);
                if let Err(err) = live.driver.select_call_devices(microphone, None) {
                    self.connection.set_note(StatusNote::error(tr_with(
                        keys::Q_OPS_COULD_NOT_SELECT_AUDIO_DEVICE,
                        &[("err", &err.to_string())],
                    )));
                }
            }
            (None, None) => self.demo_ui.selected_devices.1 = None,
        }
        cx.notify();
    }

    fn toggle_stream_playback_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Some(call_id) = live
                .driver
                .session
                .calls
                .active_group_call
                .as_ref()
                .map(|c| c.id)
            && let Err(err) = live.driver.set_group_call_playback_muted(call_id, muted)
        {
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_OPS_COULD_NOT_SELECT_AUDIO_DEVICE,
                &[("err", &err.to_string())],
            )));
        }
        cx.notify();
    }

    fn stream_leave_box(
        &mut self,
        facts: &StreamFacts,
        end: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading(tr(keys::GROUP_CALL_LEAVE_TITLE_CHANNEL)))
            .child(
                div()
                    .text_sm()
                    .child(tr(keys::GROUP_CALL_LEAVE_SURE_CHANNEL)),
            )
            .when(facts.manager, |this| {
                this.child(
                    Checkbox::new("group-call-stream-also-end")
                        .label(tr(keys::GROUP_CALL_ALSO_END_CHANNEL))
                        .checked(end)
                        .on_click(cx.listener(|this, &on: &bool, _, cx| {
                            this.group_call.stream.overlay = Some(StreamOverlay::Leave { end: on });
                            cx.notify();
                        })),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("group-call-stream-leave-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.close_stream_overlay(cx))),
                    )
                    .child(
                        Button::new("group-call-stream-leave-confirm")
                            .label(tr(keys::GROUP_CALL_LEAVE))
                            .danger()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.group_call.stream.overlay = None;
                                leave_stream_full(window);
                                // tdesktop `discard()`: ending the call
                                // leaves it too.
                                if end {
                                    this.end_active_group_call(cx);
                                }
                                this.leave_active_group_call(cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}

fn heading(text: &'static str) -> Div {
    div()
        .pb(px(4.))
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

/// `**text**` from a tdesktop string, drawn bold.
fn bold(text: &str) -> SharedString {
    text.trim_start_matches("**")
        .trim_end_matches("**")
        .to_string()
        .into()
}

/// tdesktop's `MakeConfirmBox`: the text, Cancel and the confirm button.
/// `attention`: the Stream Key warning (`FillRtmpRows`), bold (its `**`
/// markup) with the attention (danger) button; otherwise plain text and
/// the default button.
fn confirm_box(
    text: SharedString,
    confirm: &'static str,
    attention: bool,
    cx: &mut Context<QuillApp>,
    on_confirm: impl Fn(&mut QuillApp, &mut Context<QuillApp>) + 'static,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(
            div()
                .id("group-call-stream-confirm-text")
                .text_sm()
                .when(attention, |this| this.font_weight(FontWeight::SEMIBOLD))
                .child(text),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(
                    Button::new("group-call-stream-confirm-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_stream_overlay(StreamOverlay::Settings, cx)
                        })),
                )
                .child(
                    Button::new("group-call-stream-confirm")
                        .label(confirm)
                        .when(attention, |button| button.danger())
                        .when(!attention, |button| button.primary())
                        .on_click(cx.listener(move |this, _, _, cx| on_confirm(this, cx))),
                ),
        )
        .into_any_element()
}
