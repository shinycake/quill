//! The buttons under a profile's name (tdesktop `TopBar::setupActions` in
//! `info_profile_top_bar.cpp`): Message or Join, Mute, then at most three
//! of Call, Discuss, Manage, Video chat, Gift, Report and Leave, then
//! More. Which buttons show is `quill::profile_actions`; this file turns
//! the session into its facts and wires each button to Quill's flow.
#![deny(clippy::indexing_slicing)]

use super::more_menu::fill_profile_menu;
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::profile_actions::{ProfilePeerKind, ProfileTile, TileFacts, profile_tiles};
use quill::telegram::envelope::{
    ChatKind, MUTE_FOR_1_HOUR, MUTE_FOR_2_DAYS, MUTE_FOR_8_HOURS, MUTE_FOREVER,
};

const TILE_HEIGHT: f32 = 58.;

/// Whose profile the row belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum ProfilePeer {
    User(i64),
    Chat(ChatId),
}

/// The profile's peer as the row and its More menu see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct ProfileTarget {
    pub(in crate::ui) kind: ProfilePeerKind,
    /// The chat that Mute and the menu's chat entries act on (a user's
    /// private chat once it exists).
    pub(in crate::ui) chat: Option<ChatId>,
    pub(in crate::ui) user: Option<i64>,
}

impl QuillApp {
    pub(in crate::ui) fn profile_target(&self, peer: ProfilePeer) -> Option<ProfileTarget> {
        let session = self.session()?;
        match peer {
            ProfilePeer::User(user_id) => Some(ProfileTarget {
                kind: ProfilePeerKind::User {
                    is_bot: session.is_bot_user(user_id),
                },
                chat: session.private_chat_with(user_id),
                user: Some(user_id),
            }),
            ProfilePeer::Chat(chat_id) => {
                let kind = match session.chats.get(&chat_id.0)?.kind {
                    ChatKind::Private { user_id } => {
                        return self.profile_target(ProfilePeer::User(user_id.0));
                    }
                    ChatKind::BasicGroup { .. } => ProfilePeerKind::BasicGroup,
                    ChatKind::Supergroup {
                        is_channel: false, ..
                    } => ProfilePeerKind::Megagroup,
                    ChatKind::Supergroup {
                        is_channel: true, ..
                    } => ProfilePeerKind::Channel,
                    ChatKind::Secret { .. } | ChatKind::Unknown => return None,
                };
                Some(ProfileTarget {
                    kind,
                    chat: Some(chat_id),
                    user: None,
                })
            }
        }
    }

    /// Whether the viewer is in the group or channel (`amIn`); people are
    /// always "joined".
    pub(in crate::ui) fn profile_joined(&self, target: ProfileTarget) -> bool {
        match (target.user, target.chat, self.session()) {
            (Some(_), ..) => true,
            (None, Some(chat), Some(session)) => !session.chat_not_joined(chat),
            _ => false,
        }
    }

    /// tdesktop `EditPeerInfoBox::Available`: a bot the viewer owns, or a
    /// group or channel whose Edit box the viewer may open.
    pub(in crate::ui) fn profile_can_manage(&self, target: ProfileTarget) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        match (target.user, target.chat) {
            (Some(user_id), _) => session
                .user(user_id)
                .is_some_and(|user| user.is_bot && user.can_be_edited),
            (None, Some(chat)) => session.group_edit_box_available(chat),
            _ => false,
        }
    }

    /// tdesktop `CanSendStarGiftTo`: another person while Premium is sold,
    /// or a channel that takes gifts and that the viewer does not own.
    pub(in crate::ui) fn profile_can_gift(&self, target: ProfileTarget) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        match target.kind {
            ProfilePeerKind::User { is_bot: false } => {
                target.chat.is_some()
                    && target.user != session.my_user_id
                    && session.premium_possible()
            }
            ProfilePeerKind::Channel => target.chat.is_some_and(|chat| {
                !session.chat_is_owner(chat)
                    && session
                        .chat_supergroup(chat)
                        .and_then(|id| session.supergroup_full_info(id))
                        .is_some_and(|info| info.profile.can_send_gift)
            }),
            _ => false,
        }
    }

    /// Why the user cannot be called, when the privacy settings forbid it.
    fn profile_call_blocked_reason(&self, user_id: i64) -> Option<&'static str> {
        let extras = &self.session()?.user_full_info(user_id)?.extras;
        extras
            .calls_private
            .then(|| tr(keys::Q_PEOPLE_THIS_USERS_PRIVACY_SETTINGS_DONT))
    }

    fn profile_tile_facts(&self, target: ProfileTarget, has_menu: bool) -> TileFacts {
        let session = self.session();
        let chat = target
            .chat
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
        let can_call = target.user.is_some_and(|user_id| {
            session.is_some_and(|s| {
                Self::can_start_secret_chat_with(s, user_id)
                    && !s
                        .user_full_info(user_id)
                        .is_some_and(|info| info.extras.calls_blocked)
            })
        });
        let video_chat = chat.is_some_and(|chat| {
            chat.video_chat
                .as_ref()
                .is_some_and(|call| call.group_call_id != 0)
                || session.is_some_and(|s| s.chat_can_manage_video_chats(chat.id))
        });
        TileFacts {
            kind: target.kind,
            joined: self.profile_joined(target),
            has_chat: chat.is_some(),
            can_call,
            has_discussion: target
                .chat
                .is_some_and(|id| session.is_some_and(|s| s.discussion_chat_id(id).is_some())),
            can_manage: self.profile_can_manage(target),
            video_chat,
            can_gift: self.profile_can_gift(target),
            owner_or_admin: target
                .chat
                .is_some_and(|id| session.is_some_and(|s| s.chat_owner_or_admin(id))),
            has_menu,
        }
    }

    /// The row under a profile's name, `None` when it has no buttons.
    pub(in crate::ui) fn profile_action_row(
        &self,
        peer: ProfilePeer,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        if let ProfilePeer::User(user_id) = peer
            && session.my_user_id == Some(user_id)
        {
            let edit = click_tile(
                "info-panel-edit-profile",
                IconName::Pencil,
                tr(keys::Q_PEOPLE_EDIT_PROFILE),
                cx,
                |this, window, cx| this.open_edit_profile_dialog(window, cx),
            );
            return Some(tile_row(1).child(edit).into_any_element());
        }
        let target = self.profile_target(peer)?;
        let menu = self.profile_menu_model(target);
        let tiles = profile_tiles(&self.profile_tile_facts(target, !menu.entries.is_empty()));
        if tiles.is_empty() {
            return None;
        }
        let mut row = tile_row(tiles.len());
        for kind in tiles {
            row = row.child(self.profile_tile(kind, target, &menu, cx));
        }
        Some(row.into_any_element())
    }

    fn profile_tile(
        &self,
        kind: ProfileTile,
        target: ProfileTarget,
        menu: &super::more_menu::ProfileMenuModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat = target.chat;
        let user = target.user;
        match kind {
            ProfileTile::Message => click_tile(
                "info-panel-message",
                IconName::MessageSquare,
                tr(keys::PROFILE_ACTION_SHORT_MESSAGE),
                cx,
                move |this, window, cx| {
                    if let Some(user_id) = user {
                        this.dismiss_profile_modal();
                        this.open_user_chat(user_id, window, cx);
                    }
                },
            ),
            ProfileTile::Join => click_tile(
                "info-panel-join",
                IconName::UserPlus,
                tr(keys::PROFILE_ACTION_SHORT_JOIN),
                cx,
                move |this, _, cx| {
                    if let Some(chat_id) = chat {
                        this.join_channel(chat_id, cx);
                    }
                },
            ),
            ProfileTile::Mute => self.profile_mute_tile(chat, cx),
            ProfileTile::Call => self.profile_call_tile(user, cx),
            ProfileTile::Discuss => click_tile(
                "info-panel-discuss",
                IconName::MessagesSquare,
                tr(keys::PROFILE_ACTION_SHORT_DISCUSS),
                cx,
                move |this, window, cx| this.open_profile_discussion(chat, window, cx),
            ),
            ProfileTile::Manage => click_tile(
                "info-panel-manage",
                IconName::Settings,
                tr(keys::PROFILE_ACTION_SHORT_MANAGE),
                cx,
                move |this, window, cx| this.open_profile_manage(target, window, cx),
            ),
            ProfileTile::VideoChat | ProfileTile::LiveStream => click_tile(
                "info-panel-video-chat",
                IconName::Video,
                if kind == ProfileTile::LiveStream {
                    tr(keys::PROFILE_ACTION_SHORT_LIVE_STREAM)
                } else {
                    tr(keys::PROFILE_ACTION_SHORT_VIDEO_CHAT)
                },
                cx,
                move |this, window, cx| {
                    if let Some(chat_id) = chat {
                        this.start_or_join_video_chat(chat_id, window, cx);
                    }
                },
            ),
            ProfileTile::Gift => click_tile(
                "info-panel-gift",
                IconName::Gift,
                tr(keys::PROFILE_ACTION_SHORT_GIFT),
                cx,
                move |this, window, cx| {
                    if let Some(chat_id) = chat {
                        this.open_send_gift_for(chat_id, window, cx);
                    }
                },
            ),
            ProfileTile::Report => click_tile(
                "info-panel-report",
                IconName::Flag,
                tr(keys::PROFILE_ACTION_SHORT_REPORT),
                cx,
                move |this, _, cx| {
                    if let Some(chat_id) = chat {
                        this.open_message_report(chat_id, Vec::new(), cx);
                    }
                },
            ),
            ProfileTile::Leave => click_tile(
                "info-panel-leave",
                IconName::LogOut,
                tr(keys::PROFILE_ACTION_SHORT_LEAVE),
                cx,
                move |this, window, cx| {
                    if let Some(chat_id) = chat {
                        this.leave_from_profile(chat_id, window, cx);
                    }
                },
            ),
            ProfileTile::More => self.profile_more_tile(menu, cx),
        }
    }

    fn profile_call_tile(&self, user: Option<i64>, cx: &mut Context<Self>) -> AnyElement {
        let reason = user.and_then(|id| self.profile_call_blocked_reason(id));
        let call = tile(
            "info-panel-call",
            IconName::Phone,
            tr(keys::PROFILE_ACTION_SHORT_CALL),
            cx,
        );
        match reason {
            Some(reason) => call.disabled(true).tooltip(reason).into_any_element(),
            None => call
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(user_id) = user {
                        this.start_call_for_user(user_id, false, cx);
                    }
                }))
                .into_any_element(),
        }
    }

    fn profile_more_tile(
        &self,
        menu: &super::more_menu::ProfileMenuModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        let menu = menu.clone();
        tile(
            "info-panel-more",
            IconName::Ellipsis,
            tr(keys::PROFILE_ACTION_SHORT_MORE),
            cx,
        )
        .on_click(|event, window, cx| {
            if matches!(event, ClickEvent::Keyboard(_)) {
                window.dispatch_action(
                    Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                    cx,
                );
            }
        })
        .dropdown_menu(move |popup, window, cx| fill_profile_menu(popup, &menu, &owner, window, cx))
        .into_any_element()
    }

    /// Mute: a click mutes forever or unmutes (`ToggleMuteForever`), the
    /// right-click menu is tdesktop's `FillMuteMenu`.
    fn profile_mute_tile(&self, chat: Option<ChatId>, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let summary = chat.and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
        let muted = summary.is_some_and(|c| session.is_some_and(|s| s.effective_muted(c)));
        let sound_off = summary.is_some_and(|c| {
            let settings = &c.notification_settings;
            !settings.use_default_sound && settings.sound_id == 0
        });
        let owner = cx.entity().downgrade();
        tile(
            "info-panel-mute",
            if muted {
                IconName::BellOff
            } else {
                IconName::Bell
            },
            if muted {
                tr(keys::PROFILE_ACTION_SHORT_UNMUTE)
            } else {
                tr(keys::PROFILE_ACTION_SHORT_MUTE)
            },
            cx,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if let Some(chat_id) = chat {
                this.apply_chat_mute(chat_id, if muted { 0 } else { MUTE_FOREVER }, cx);
            }
        }))
        .context_menu(move |menu, _, _| match chat {
            Some(chat_id) => mute_menu(menu, &owner, chat_id, muted, sound_off),
            None => menu,
        })
        .into_any_element()
    }

    /// Discuss and View discussion: the channel's linked group.
    pub(in crate::ui) fn open_profile_discussion(
        &mut self,
        chat: Option<ChatId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let discussion = chat.and_then(|id| self.session()?.discussion_chat_id(id));
        if let Some(discussion) = discussion {
            self.dismiss_profile_modal();
            self.select_listed_chat(ChatId(discussion), window, cx);
        }
    }

    /// Manage: the bot's edit screen or the group or channel's Edit box.
    pub(in crate::ui) fn open_profile_manage(
        &mut self,
        target: ProfileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (target.user, target.chat) {
            (Some(bot_id), _) => self.open_bot_edit(bot_id, window, cx),
            (None, Some(chat_id)) => self.open_group_edit_box(chat_id, window, cx),
            _ => {}
        }
    }

    /// Leave: an owner first picks who inherits, everyone else confirms.
    pub(in crate::ui) fn leave_from_profile(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_owner_leave(chat_id, window, cx) {
            self.open_group_confirm(chat_id, crate::ui::GroupConfirmAction::LeaveChat, cx);
        }
    }
}

fn tile_row(count: usize) -> Div {
    div()
        .grid()
        .grid_cols(u16::try_from(count).unwrap_or(1))
        .gap_1p5()
        .w_full()
}

/// A tile whose click runs `run` on the app.
fn click_tile(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    cx: &mut Context<QuillApp>,
    run: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> AnyElement {
    tile(id, icon, label, cx)
        .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx)))
        .into_any_element()
}

/// One labeled button of the row.
fn tile(id: &'static str, icon: IconName, label: &'static str, cx: &App) -> Button {
    Button::new(id)
        .secondary()
        .rounded(ButtonRounded::Large)
        .w_full()
        .px_1()
        .h(px(TILE_HEIGHT))
        .accessibility_label(label)
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(Icon::new(icon).size(px(18.)).text_color(cx.theme().primary))
                .child(div().text_xs().child(label)),
        )
}

/// tdesktop `MuteMenu::FillMuteMenu`: Select tone, sound on or off, the
/// preset durations, Mute for..., then Mute forever or Unmute.
fn mute_menu(
    mut menu: PopupMenu,
    owner: &WeakEntity<QuillApp>,
    chat_id: ChatId,
    muted: bool,
    sound_off: bool,
) -> PopupMenu {
    let act = |owner: &WeakEntity<QuillApp>,
               f: fn(&mut QuillApp, ChatId, &mut Context<QuillApp>)| {
        let owner = owner.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = owner.update(cx, |this, cx| f(this, chat_id, cx));
        }
    };
    menu = menu
        .item(
            PopupMenuItem::new(tr(keys::MUTE_MENU_SOUND_SELECT))
                .icon(IconName::Music)
                .on_click(act(owner, |this, chat_id, cx| {
                    this.dismiss_profile_modal();
                    this.open_mute_menu_for(chat_id, false, cx);
                })),
        )
        .item(
            PopupMenuItem::new(if sound_off {
                tr(keys::MUTE_MENU_SOUND_ON)
            } else {
                tr(keys::MUTE_MENU_SOUND_OFF)
            })
            .icon(if sound_off {
                IconName::Volume2
            } else {
                IconName::VolumeX
            })
            .on_click({
                let owner = owner.clone();
                move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        let (use_default, sound_id) =
                            quill::mute_menu::sound_toggle_target(sound_off);
                        this.apply_chat_sound(chat_id, use_default, sound_id, cx);
                    });
                }
            }),
        );
    for seconds in [MUTE_FOR_1_HOUR, MUTE_FOR_8_HOURS, MUTE_FOR_2_DAYS] {
        let owner = owner.clone();
        menu = menu.item(
            PopupMenuItem::new(tr_with(
                keys::MUTE_MENU_DURATION_ANY,
                &[(
                    "duration",
                    quill::mute_menu::mute_period_text(seconds).as_str(),
                )],
            ))
            .icon(IconName::BellOff)
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |this, cx| this.apply_chat_mute(chat_id, seconds, cx));
            }),
        );
    }
    menu.item(
        PopupMenuItem::new(tr(keys::MUTE_MENU_DURATION))
            .icon(IconName::Timer)
            .on_click(act(owner, |this, chat_id, cx| {
                this.dismiss_profile_modal();
                this.open_mute_menu_for(chat_id, true, cx);
            })),
    )
    .item(
        PopupMenuItem::new(if muted {
            tr(keys::MUTE_MENU_UNMUTE)
        } else {
            tr(keys::MUTE_MENU_DURATION_FOREVER)
        })
        .icon(if muted {
            IconName::Bell
        } else {
            IconName::BellOff
        })
        .on_click({
            let owner = owner.clone();
            move |_, _, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.apply_chat_mute(chat_id, if muted { 0 } else { MUTE_FOREVER }, cx);
                });
            }
        }),
    )
}
