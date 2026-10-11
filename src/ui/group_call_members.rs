//! The member rows of the group-call window (split out of
//! `group_call_panel.rs`): status line, mic icon and the per-member menu.

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::group_call_panel::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};

/// A member's status line and its color (tdesktop's member row).
/// `speaking` is TDLib's flag, or your own level tap for your row.
pub(super) fn member_status(p: &ParsedGroupCallParticipant, speaking: bool) -> (&'static str, u32) {
    if speaking {
        ("speaking", MEMBER_ACTIVE)
    } else if p.is_hand_raised {
        ("wants to speak", MEMBER_INACTIVE_STATUS)
    } else if p.is_muted_for_current_user {
        ("muted for you", MEMBER_MUTED_ICON)
    } else {
        ("listening", MEMBER_INACTIVE_STATUS)
    }
}

impl QuillApp {
    /// The "⋯" menu: the chat's settings and admin tools.
    pub(super) fn group_call_menu(
        &self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let call = call.clone();
        let chat_shown = self.group_call.chat_shown;
        let screen_source = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.group_call_screen_source_available());
        Button::new("group-call-menu")
            .icon(IconName::EllipsisVertical)
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
                        if call.screen_sharing || call.screen_share_pending { "Stop Sharing Screen" } else { "Share Screen" },
                        |this, _, cx| this.toggle_group_call_screen_share(cx),
                    ));
                }
                if call.are_messages_allowed || !call.messages.is_empty() {
                    menu = menu.item(item(
                        if chat_shown { "Hide Chat" } else { "Show Chat" },
                        |this, _, cx| {
                            this.group_call.chat_shown = !this.group_call.chat_shown;
                            cx.notify();
                        },
                    ));
                }
                match &call.invite_link {
                    Some(link) => {
                        let link = link.clone();
                        let owner = owner.clone();
                        menu = menu.item(PopupMenuItem::new("Copy Invite Link").on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| this.copy_video_chat_invite_link(&link, cx));
                        }));
                    }
                    None => {
                        menu = menu.item(item("Share Invite Link", |this, _, cx| this.fetch_group_call_invite_link(cx)));
                    }
                }
                if call.can_be_managed {
                    if call.invite_link.is_some() {
                        menu = menu.item(item("Revoke Invite Link", |this, _, cx| this.revoke_group_call_invite_link(cx)));
                    }
                    menu = menu.item(item("Stream With…", |this, _, cx| this.fetch_video_chat_rtmp_url(cx)));
                    menu = menu
                        .separator()
                        .item(item("Edit Title", |this, window, cx| this.open_group_call_title_dialog(window, cx)))
                        .item(item(
                            if call.record_duration > 0 { "Stop Recording" } else { "Start Recording" },
                            |this, _, cx| this.toggle_group_call_recording(cx),
                        ));
                    if call.can_toggle_mute_new_participants {
                        menu = menu.item(
                            item("Mute New Participants", |this, _, cx| this.toggle_video_chat_mute_new(cx))
                                .checked(call.mute_new_participants),
                        );
                    }
                    if call.can_toggle_are_messages_allowed {
                        menu = menu.item(
                            item("Allow Chat", |this, _, cx| this.toggle_group_call_chat(cx))
                                .checked(call.are_messages_allowed),
                        );
                    }
                    menu = menu
                        .separator()
                        .item(item("End Video Chat", |this, _, cx| this.end_active_group_call(cx)));
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        let speaking = participant.is_speaking || (participant.is_current_user && self_speaking);
        let (status, status_color) = member_status(participant, speaking);
        let force_muted = participant.is_muted_for_all_users && !participant.can_unmute_self;
        let (mic, mic_color) = if participant.is_hand_raised {
            (IconName::Hand, MEMBER_INACTIVE_STATUS)
        } else if force_muted || participant.is_muted_for_current_user {
            (IconName::MicOff, MEMBER_MUTED_ICON)
        } else if participant.is_muted_for_all_users {
            (IconName::MicOff, MEMBER_INACTIVE_ICON)
        } else if speaking {
            (IconName::Mic, MEMBER_ACTIVE)
        } else {
            (IconName::Mic, MEMBER_INACTIVE_ICON)
        };
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
            .h(px(56.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(14.))
            .hover(|style| style.bg(rgb(MEMBERS_BG_OVER)))
            .child(chat_avatar(&name, photo.as_deref(), 40.))
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
                                format!("{name} (you)")
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
                    .text_color(rgb(MEMBER_NOT_JOINED)),
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
                        "Lower Hand".into(),
                        Box::new(|this, cx| this.toggle_group_call_self_hand(false, cx)),
                    ));
                } else if force_muted {
                    menu = menu.item(item(
                        "Raise Hand".into(),
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
                        "Allow to Speak".into(),
                        Box::new(move |this, cx| {
                            this.toggle_group_call_participant_muted(allow, false, cx)
                        }),
                    ))
                    .item(item(
                        "Lower Hand".into(),
                        Box::new(move |this, cx| {
                            this.toggle_group_call_participant_hand(lower, false, cx)
                        }),
                    ));
            } else if p.can_be_muted_for_all_users || p.can_be_muted_for_current_user {
                let sender = sender;
                let label = if p.can_be_muted_for_all_users {
                    "Mute"
                } else {
                    "Mute for Me"
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
                    "Allow to Speak"
                } else {
                    "Unmute for Me"
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
                (format!("Louder ({volume}%)"), 2000),
                (format!("Quieter ({volume}%)"), -2000),
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
                    "Remove".into(),
                    Box::new(move |this, cx| this.ban_group_call_participant(user_id, cx)),
                ));
            }
            menu
        };
        row.context_menu(build.clone())
            .child(
                Button::new(SharedString::from(format!("{row_id}-more")))
                    .icon(IconName::EllipsisVertical)
                    .ghost()
                    .xsmall()
                    .text_color(rgb(MEMBER_NOT_JOINED))
                    .dropdown_menu(build),
            )
            .into_any_element()
    }
}
