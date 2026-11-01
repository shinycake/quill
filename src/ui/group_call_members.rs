//! The member rows of the group-call window (split out of
//! `group_call_panel.rs`): status line, mic icon and the per-member menu.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::frame_clock::trace_slice_render;
use super::group_call_members_view::ROW_HEIGHT;
use super::group_call_panel::*;
use super::group_call_speaker::speaker_userpic;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};

/// A member's status line and its color (tdesktop's member row).
/// `speaking` is tgcalls' levels, TDLib's flag, or your own level tap.
pub(super) fn member_status(p: &ParsedGroupCallParticipant, speaking: bool) -> (&'static str, u32) {
    if speaking {
        ("speaking", MEMBER_ACTIVE)
    } else if p.is_hand_raised {
        (
            tr(keys::GROUP_CALL_RAISED_HAND_STATUS),
            MEMBER_INACTIVE_STATUS,
        )
    } else if p.is_muted_for_current_user {
        (tr(keys::GROUP_CALL_MUTED_BY_ME_STATUS), MEMBER_MUTED_ICON)
    } else {
        ("listening", MEMBER_INACTIVE_STATUS)
    }
}

/// What a member row shows: status text and color, mic icon and color.
/// Your own row derives from the call's `is_muted_self` (the one source
/// the call bar and the big button read), not from the participant's
/// server-side fields, which lag the local change.
pub(super) fn member_look(
    p: &ParsedGroupCallParticipant,
    speaking: bool,
    self_muted: bool,
) -> (&'static str, u32, IconName, u32) {
    let force_muted = p.is_muted_for_all_users && !p.can_unmute_self;
    if p.is_current_user {
        return if p.is_hand_raised {
            (
                tr(keys::GROUP_CALL_RAISED_HAND_STATUS),
                MEMBER_INACTIVE_STATUS,
                IconName::Hand,
                MEMBER_INACTIVE_STATUS,
            )
        } else if force_muted {
            (
                tr(keys::GROUP_CALL_FORCE_MUTED),
                MEMBER_MUTED_ICON,
                IconName::MicOff,
                MEMBER_MUTED_ICON,
            )
        } else if self_muted {
            (
                tr(keys::Q_AV_MUTED_STATUS),
                MEMBER_INACTIVE_STATUS,
                IconName::MicOff,
                MEMBER_INACTIVE_ICON,
            )
        } else if speaking {
            ("speaking", MEMBER_ACTIVE, IconName::Mic, MEMBER_ACTIVE)
        } else {
            (
                "listening",
                MEMBER_INACTIVE_STATUS,
                IconName::Mic,
                MEMBER_INACTIVE_ICON,
            )
        };
    }
    let (status, status_color) = member_status(p, speaking);
    let (mic, mic_color) = if p.is_hand_raised {
        (IconName::Hand, MEMBER_INACTIVE_STATUS)
    } else if force_muted || p.is_muted_for_current_user {
        (IconName::MicOff, MEMBER_MUTED_ICON)
    } else if p.is_muted_for_all_users {
        (IconName::MicOff, MEMBER_INACTIVE_ICON)
    } else if speaking {
        (IconName::Mic, MEMBER_ACTIVE)
    } else {
        (IconName::Mic, MEMBER_INACTIVE_ICON)
    };
    (status, status_color, mic, mic_color)
}

impl QuillApp {
    /// The messages sit below every member row, so showing them scrolls there.
    pub(super) fn toggle_group_call_messages(&mut self, cx: &mut Context<Self>) {
        self.group_call.chat_shown = !self.group_call.chat_shown;
        if self.group_call.chat_shown {
            self.group_call.members_scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// The "⋯" menu: the chat's settings and admin tools.
    pub(super) fn group_call_menu(
        &self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let call = call.clone();
        let chat_shown = self.group_call.chat_shown;
        let share_link = self.group_call_can_share_link(&call);
        let screen_source = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.group_call_screen_source_available())
            || self.uses_screen_chooser();
        Button::new("group-call-menu")
            .icon(IconName::EllipsisVertical)
            .tooltip(tr(keys::PROFILE_ACTION_SHORT_MORE))
            .accessibility_label(tr(keys::PROFILE_ACTION_SHORT_MORE))
            .ghost()
            .small()
            .text_color(white())
            .dropdown_menu(move |mut menu, _, _| {
                let item = |label: &str, action: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
                    let owner = owner.clone();
                    PopupMenuItem::new(label.to_string()).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| action(this, window, cx));
                    })
                };
                if call.is_joined && screen_source {
                    menu = menu.item(item(
                        if call.screen_sharing || call.screen_share_pending { tr(keys::Q_AV_STOP_SHARING_SCREEN) } else { tr(keys::GROUP_CALL_TOOLTIP_SCREEN) },
                        |this, _, cx| this.toggle_group_call_screen_share(cx),
                    ));
                }
                if call.are_messages_allowed || !call.messages.is_empty() {
                    menu = menu.item(item(
                        if chat_shown { tr(keys::Q_AV_HIDE_CHAT) } else { tr(keys::Q_AV_SHOW_CHAT) },
                        |this, _, cx| this.toggle_group_call_messages(cx),
                    ));
                }
                if share_link {
                    menu = menu.item(item(tr(keys::GROUP_CALL_SHARE), |this, _, cx| this.open_group_call_share_links(cx)));
                }
                // tdesktop's menu: "Settings" opens the call settings box.
                menu = menu.item(item(tr(keys::GROUP_CALL_SETTINGS), |this, _, cx| this.open_group_call_settings(cx)));
                if call.can_be_managed {
                    if call.share_links.listener.is_some() {
                        menu = menu.item(item(tr(keys::Q_AV_REVOKE_INVITE_LINK), |this, _, cx| this.revoke_group_call_invite_link(cx)));
                    }
                    menu = menu.item(item(tr(keys::MENU_START_GROUP_CALL_WITH), |this, _, cx| this.fetch_video_chat_rtmp_url(cx)));
                    menu = menu
                        .separator()
                        .item(item(tr(keys::Q_AV_EDIT_TITLE), |this, window, cx| this.open_group_call_title_dialog(window, cx)))
                        .item(item(
                            if call.record_duration > 0 { tr(keys::GROUP_CALL_PTT_RECORDING) } else { tr(keys::GROUP_CALL_RECORDING_START) },
                            |this, _, cx| this.toggle_group_call_recording(cx),
                        ));
                    if call.can_toggle_mute_new_participants {
                        menu = menu.item(
                            item(tr(keys::GROUP_CALL_NEW_MUTED), |this, _, cx| this.toggle_video_chat_mute_new(cx))
                                .checked(call.mute_new_participants),
                        );
                    }
                    if call.can_toggle_are_messages_allowed {
                        menu = menu.item(
                            item(tr(keys::Q_AV_ALLOW_CHAT), |this, _, cx| this.toggle_group_call_chat(cx))
                                .checked(call.are_messages_allowed),
                        );
                    }
                    menu = menu
                        .separator()
                        .item(item(tr(keys::GROUP_CALL_END), |this, _, cx| this.end_active_group_call(cx)));
                }
                menu
            })
    }

    /// One member row: avatar, name, status, mic; a click opens what you
    /// can do with them.
    pub(super) fn group_member_row(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        self_speaking: bool,
        now: std::time::Instant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        trace_slice_render("group-member-row");
        let name = self.group_call_participant_name(&participant.participant_id);
        let photo = match &participant.participant_id {
            MessageSender::User { user_id } => {
                self.session().and_then(|s| s.user_photo_path(*user_id))
            }
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|s| s.chat_photo_path(quill::ids::ChatId(*chat_id))),
        }
        .and_then(|path| {
            quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
        });
        // tgcalls' levels (tdesktop's), else TDLib's flag and your own tap.
        let speaker = self.group_member_speaker(participant, now);
        let speaking = speaker.look.speaking
            || participant.is_speaking
            || (participant.is_current_user && self_speaking);
        let (status, status_color, mic, mic_color) =
            member_look(participant, speaking, call.is_muted_self);
        let force_muted = participant.is_muted_for_all_users && !participant.can_unmute_self;
        let me = participant.is_current_user;
        let sender = participant.participant_id;
        let user_id = match sender {
            MessageSender::User { user_id } => Some(user_id),
            MessageSender::Chat { .. } => None,
        };
        let p = participant.clone();
        let owned = call.is_owned;
        let row_id = SharedString::from(format!("group-member-{:?}", participant.participant_id));
        let owner = cx.entity().downgrade();
        let row = div()
            .id(row_id.clone())
            .h(px(ROW_HEIGHT))
            .debug_selector(|| format!("group-member-row-{}", participant.audio_source_id))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(14.))
            .hover(|style| style.bg(rgb(MEMBERS_BG_OVER)))
            .child(speaker_userpic(
                &name,
                photo.as_deref(),
                speaker,
                participant.is_muted_for_current_user,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(if me {
                                tr_with(keys::Q_AV_YOU, &[("name", &name.to_string())])
                            } else {
                                name.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(status_color))
                            .child(status),
                    ),
            )
            .when(p.video_enabled || p.screen_sharing_enabled, |this| {
                this.child(
                    Icon::new(if p.screen_sharing_enabled {
                        IconName::ScreenShare
                    } else {
                        IconName::Video
                    })
                    .with_size(px(16.))
                    .text_color(crate::ui::app_theme::call_chrome::group_muted()),
                )
            })
            .child(Icon::new(mic).with_size(px(20.)).text_color(rgb(mic_color)));
        let build = move |mut menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
            let item = |label: String, action: MemberAction| {
                let owner = owner.clone();
                PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| action(this, cx));
                })
            };
            if me {
                if p.is_hand_raised {
                    menu = menu.item(item(
                        tr(keys::Q_AV_LOWER_HAND).into(),
                        Box::new(|this, cx| this.toggle_group_call_self_hand(false, cx)),
                    ));
                } else if force_muted {
                    menu = menu.item(item(
                        tr(keys::Q_AV_RAISE_HAND).into(),
                        Box::new(|this, cx| this.toggle_group_call_self_hand(true, cx)),
                    ));
                }
                return menu;
            }
            if p.is_hand_raised && p.can_be_unmuted_for_all_users {
                let allow = sender;
                let lower = sender;
                menu = menu
                    .item(item(
                        tr(keys::GROUP_CALL_CONTEXT_UNMUTE).into(),
                        Box::new(move |this, cx| {
                            this.toggle_group_call_participant_muted(allow, false, cx)
                        }),
                    ))
                    .item(item(
                        tr(keys::Q_AV_LOWER_HAND).into(),
                        Box::new(move |this, cx| {
                            this.toggle_group_call_participant_hand(lower, false, cx)
                        }),
                    ));
            } else if p.can_be_muted_for_all_users || p.can_be_muted_for_current_user {
                let sender = sender;
                let label = if p.can_be_muted_for_all_users {
                    tr(keys::GROUP_CALL_CONTEXT_MUTE)
                } else {
                    tr(keys::GROUP_CALL_CONTEXT_MUTE_FOR_ME)
                };
                menu = menu.item(item(
                    label.into(),
                    Box::new(move |this, cx| {
                        this.toggle_group_call_participant_muted(sender, true, cx)
                    }),
                ));
            } else if p.can_be_unmuted_for_all_users || p.can_be_unmuted_for_current_user {
                let sender = sender;
                let label = if p.can_be_unmuted_for_all_users {
                    tr(keys::GROUP_CALL_CONTEXT_UNMUTE)
                } else {
                    tr(keys::GROUP_CALL_CONTEXT_UNMUTE_FOR_ME)
                };
                menu = menu.item(item(
                    label.into(),
                    Box::new(move |this, cx| {
                        this.toggle_group_call_participant_muted(sender, false, cx)
                    }),
                ));
            }
            let volume = p.volume_level / 100;
            for (label, delta) in [
                (
                    tr_with(keys::Q_AV_LOUDER, &[("volume", &volume.to_string())]),
                    2000,
                ),
                (
                    tr_with(keys::Q_AV_QUIETER, &[("volume", &volume.to_string())]),
                    -2000,
                ),
            ] {
                let sender = sender;
                menu = menu.item(item(
                    label,
                    Box::new(move |this, cx| {
                        this.adjust_group_call_participant_volume(sender, delta, cx)
                    }),
                ));
            }
            if let Some(user_id) = user_id.filter(|_| owned) {
                menu = menu.separator().item(item(
                    tr(keys::GROUP_CALL_CONTEXT_REMOVE).into(),
                    Box::new(move |this, cx| this.ban_group_call_participant(user_id, cx)),
                ));
            }
            menu
        };
        row.context_menu(build.clone())
            .child(
                Button::new(SharedString::from(format!("{row_id}-more")))
                    .icon(IconName::EllipsisVertical)
                    .tooltip(tr(keys::PROFILE_ACTION_SHORT_MORE))
                    .accessibility_label(tr(keys::PROFILE_ACTION_SHORT_MORE))
                    .ghost()
                    .xsmall()
                    .text_color(crate::ui::app_theme::call_chrome::group_muted())
                    .dropdown_menu(build),
            )
            .into_any_element()
    }
}
