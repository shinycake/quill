//! The profile's More menu (tdesktop `Filler::fillProfileActions` in
//! `window_peer_menu.cpp`). `quill::profile_actions` picks the entries;
//! this file gathers their facts, labels them and runs Quill's existing
//! flow for each.
#![deny(clippy::indexing_slicing)]

use super::action_row::ProfileTarget;
use crate::ui::GroupSettingsView;
use crate::ui::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::profile_actions::{
    ManageLabel, MenuFacts, ProfileMenuEntry, ProfilePeerKind, profile_menu_entries,
};
use quill::state::InfoPanelTarget;
use quill::suggested_posts::DirectMessagesRole;

/// A folder of the Add to folder submenu: id, name, holds the chat.
type FolderChoice = (i32, String, bool);

/// What the menu shows, gathered when the row renders.
#[derive(Clone, Debug)]
pub(in crate::ui) struct ProfileMenuModel {
    pub(in crate::ui) target: ProfileTarget,
    pub(in crate::ui) entries: Vec<ProfileMenuEntry>,
    folders: Vec<FolderChoice>,
}

impl QuillApp {
    pub(in crate::ui) fn profile_menu_model(&self, target: ProfileTarget) -> ProfileMenuModel {
        let facts = self.profile_menu_facts(target);
        let folders = match (self.session(), target.chat) {
            (Some(session), Some(chat_id)) if facts.can_add_to_folder => {
                let held = session
                    .chats
                    .get(&chat_id.0)
                    .map(|chat| chat.folder_positions.clone())
                    .unwrap_or_default();
                session
                    .chat_list
                    .chat_folders
                    .iter()
                    .map(|folder| {
                        (
                            folder.id,
                            folder.name.clone(),
                            held.contains_key(&folder.id),
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        ProfileMenuModel {
            target,
            entries: profile_menu_entries(&facts),
            folders,
        }
    }

    fn profile_menu_facts(&self, target: ProfileTarget) -> MenuFacts {
        let Some(session) = self.session() else {
            return MenuFacts::default();
        };
        let live = self.live.is_some();
        let chat = target.chat.and_then(|id| session.chats.get(&id.0));
        let user = target.user.and_then(|id| session.user(id));
        let user_info = target.user.and_then(|id| session.user_full_info(id));
        let is_self = target.user.is_some() && target.user == session.my_user_id;
        let joined = self.profile_joined(target);
        let is_owner = target.chat.is_some_and(|id| session.chat_is_owner(id));
        let full_info = target
            .chat
            .and_then(|id| session.chat_supergroup(id))
            .and_then(|id| session.supergroup_full_info(id));
        let reachable = target
            .user
            .is_some_and(|id| Self::can_start_secret_chat_with(session, id));
        let calls_off = user_info.is_some_and(|info| {
            info.extras.calls_blocked
                || info.extras.calls_private
                || info.extras.video_calls_unsupported
        });
        let can_auto_delete = live
            && chat.is_some_and(|chat| {
                chat.supported()
                    && match target.kind {
                        ProfilePeerKind::User { .. } => !is_self,
                        _ => chat.can_change_info() && joined,
                    }
            });
        let is_public = target
            .chat
            .and_then(|id| session.chat_supergroup(id))
            .and_then(|id| session.supergroup_username(id))
            .is_some_and(|name| !name.is_empty());
        let has_personal_channel = session
            .my_user_id
            .and_then(|me| session.user_full_info(me))
            .is_some_and(|info| info.extras.personal_chat_id != 0);
        MenuFacts {
            kind: Some(target.kind),
            is_self,
            is_contact: user.is_some_and(|u| u.is_contact),
            has_phone: user.is_some_and(|u| !u.phone_number.is_empty()),
            can_auto_delete,
            can_video_call: live && reachable && !calls_off,
            can_secret_chat: live && reachable,
            can_add_members: target
                .chat
                .is_some_and(|id| joined && session.chat_can_add_members(id)),
            can_gift: self.profile_can_gift(target),
            can_get_statistics: full_info.is_some_and(|info| info.can_get_statistics),
            is_owner,
            can_view_revenue: full_info.is_some_and(|info| {
                info.admin.can_get_revenue_statistics || info.admin.can_get_star_revenue_statistics
            }),
            can_edit_stories: is_owner,
            can_manage: self.profile_can_manage(target),
            can_set_personal_channel: is_owner && is_public && !has_personal_channel,
            has_discussion: target
                .chat
                .is_some_and(|id| session.discussion_chat_id(id).is_some()),
            direct_messages_admin: self.profile_direct_messages(target).is_some(),
            exportable: live
                && target
                    .chat
                    .is_some_and(|id| !is_self && !session.chat_has_protected_content(id)),
            sharing_disabled: target
                .chat
                .and_then(|id| session.private_sharing_disabled(id)),
            can_add_to_folder: !session.chat_list.chat_folders.is_empty()
                && chat
                    .is_some_and(|chat| chat.supported() && (chat.in_main_list || chat.in_archive)),
            blocked: target.user.filter(|_| !is_self).map(|_| {
                user_info.is_some_and(|info| info.blocked) || chat.is_some_and(|chat| chat.blocked)
            }),
            can_report: chat.is_some_and(|chat| chat.can_be_reported),
            joined,
        }
    }

    /// The channel's direct messages group, when the viewer answers there.
    fn profile_direct_messages(&self, target: ProfileTarget) -> Option<ChatId> {
        if target.kind != ProfilePeerKind::Channel {
            return None;
        }
        let session = self.session()?;
        let info = session
            .chat_supergroup(target.chat?)
            .and_then(|id| session.supergroup_full_info(id))?;
        let direct = ChatId(info.direct_messages_chat_id);
        (info.direct_messages_chat_id != 0
            && session.direct_messages_role(direct) == Some(DirectMessagesRole::Admin))
        .then_some(direct)
    }

    fn run_profile_menu_entry(
        &mut self,
        entry: ProfileMenuEntry,
        target: ProfileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ProfileMenuEntry as E;
        let chat = target.chat;
        let user = target.user;
        match entry {
            E::AutoDelete => self.open_profile_auto_delete(chat, window, cx),
            E::AddContact => {
                if let Some(id) = user {
                    self.open_add_contact_dialog(id, window, cx);
                }
            }
            E::ShareContact => {
                if let Some(id) = user {
                    self.open_share_contact_dialog(id, cx);
                }
            }
            E::EditContact => {
                if let Some(id) = user {
                    self.open_edit_contact_dialog(id, window, cx);
                }
            }
            E::VideoCall => {
                if let Some(id) = user {
                    self.start_call_for_user(id, true, cx);
                }
            }
            E::SecretChat => {
                if let Some(id) = user {
                    self.start_secret_chat_for_user(id, cx);
                }
            }
            E::AddMembers { .. } => {
                if let Some(id) = chat {
                    self.inline_members_add(id, window, cx);
                }
            }
            E::SendGift => {
                if let Some(id) = chat {
                    self.open_send_gift_for(id, window, cx);
                }
            }
            E::Statistics => {
                if let Some(id) = chat {
                    self.open_info_panel_target(InfoPanelTarget::Statistics(id.0), window, cx);
                }
            }
            E::Boosts => {
                if let Some(id) = chat {
                    self.open_group_settings_view(id, GroupSettingsView::Boosts, cx);
                }
            }
            E::Monetization => {
                if let Some(id) = chat {
                    self.open_earn(
                        quill::earn::EarnPanel::Revenue {
                            peer: quill::earn::EarnPeer::Chat(id.0),
                            screen: quill::earn::EarnScreen::Monetization,
                        },
                        window,
                        cx,
                    );
                }
            }
            E::StoryArchive => {
                if let Some(id) = chat {
                    self.open_story_page(id, window, cx);
                }
            }
            E::Manage(_) => self.open_profile_manage(target, window, cx),
            E::SetPersonalChannel => {
                if let Some(id) = chat {
                    self.choose_personal_channel(id.0, cx);
                }
            }
            E::ViewDiscussion => self.open_profile_discussion(chat, window, cx),
            E::DirectMessages => {
                if let Some(direct) = self.profile_direct_messages(target) {
                    self.dismiss_profile_modal();
                    self.select_listed_chat(direct, window, cx);
                }
            }
            E::Export => {
                if let Some(id) = chat {
                    self.start_chat_export(id, cx);
                }
            }
            E::ToggleSharing { disabled } => {
                if let Some(id) = chat {
                    self.toggle_private_sharing(id, disabled, cx);
                }
            }
            E::AddToFolder => {}
            E::Block { bot, blocked } => {
                if let Some(id) = user {
                    if bot {
                        self.toggle_bot_block(id, blocked, window, cx);
                    } else {
                        self.open_group_confirm(
                            ChatId(0),
                            crate::ui::GroupConfirmAction::BlockContact {
                                user_id: id,
                                block: !blocked,
                            },
                            cx,
                        );
                    }
                }
            }
            E::Report => {
                if let Some(id) = chat {
                    self.open_message_report(id, Vec::new(), cx);
                }
            }
            E::Leave { .. } => {
                if let Some(id) = chat {
                    self.leave_from_profile(id, window, cx);
                }
            }
            E::DeleteContact => {
                if let Some(id) = user {
                    self.open_group_confirm(
                        ChatId(0),
                        crate::ui::GroupConfirmAction::DeleteContact { user_id: id },
                        cx,
                    );
                }
            }
        }
        cx.notify();
    }

    /// Auto-Delete: the timer picker lives on the chat's own screen, so the
    /// profile's chat opens first when another one is on screen.
    fn open_profile_auto_delete(
        &mut self,
        chat: Option<ChatId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = chat else {
            return;
        };
        if self.session().and_then(|s| s.open_chat) != Some(chat_id) {
            self.dismiss_profile_modal();
            self.select_listed_chat(chat_id, window, cx);
        }
        if self.session().and_then(|s| s.open_chat) != Some(chat_id) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            )));
            return;
        }
        self.open_ttl_picker(chat_id, cx);
    }
}

/// The menu itself; Add to folder opens a submenu of folders.
pub(in crate::ui) fn fill_profile_menu(
    mut menu: PopupMenu,
    model: &ProfileMenuModel,
    owner: &WeakEntity<QuillApp>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let target = model.target;
    for &entry in &model.entries {
        let (label, icon) = entry_label(entry);
        if entry == ProfileMenuEntry::AddToFolder {
            let folders = model.folders.clone();
            let owner = owner.clone();
            menu = menu.submenu_with_icon(
                Some(icon.into()),
                label,
                window,
                cx,
                move |mut sub, _, _| {
                    for (folder_id, name, held) in &folders {
                        let (folder_id, held) = (*folder_id, *held);
                        let owner = owner.clone();
                        sub = sub.item(PopupMenuItem::new(name.clone()).checked(held).on_click(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    let Some(chat_id) = target.chat else {
                                        return;
                                    };
                                    if held {
                                        this.remove_open_chat_from_folder(chat_id, folder_id, cx);
                                    } else {
                                        this.add_open_chat_to_folder(chat_id, folder_id, cx);
                                    }
                                });
                            },
                        ));
                    }
                    sub
                },
            );
            continue;
        }
        let owner = owner.clone();
        menu = menu.item(
            PopupMenuItem::new(label)
                .icon(icon)
                .on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.run_profile_menu_entry(entry, target, window, cx);
                    });
                }),
        );
    }
    menu
}

fn entry_label(entry: ProfileMenuEntry) -> (&'static str, IconName) {
    use ProfileMenuEntry as E;
    match entry {
        E::AutoDelete => (tr(keys::MANAGE_MESSAGES_TTL_MENU), IconName::Timer),
        E::AddContact => (tr(keys::INFO_ADD_AS_CONTACT), IconName::UserPlus),
        E::ShareContact => (tr(keys::INFO_SHARE_CONTACT), IconName::Share2),
        E::EditContact => (tr(keys::INFO_EDIT_CONTACT), IconName::Pencil),
        E::VideoCall => (tr(keys::Q_OPS_VIDEO_CALL_3), IconName::Video),
        E::SecretChat => (tr(keys::Q_PEOPLE_SECRET_CHAT), IconName::Lock),
        E::AddMembers { channel: false } => (tr(keys::CHANNEL_ADD_MEMBERS), IconName::UserPlus),
        E::AddMembers { channel: true } => (tr(keys::CHANNEL_ADD_USERS), IconName::UserPlus),
        E::SendGift => (tr(keys::PROFILE_GIFT_PREMIUM), IconName::Gift),
        E::Statistics => (tr(keys::STATS_TITLE), IconName::ChartBar),
        E::Boosts => (tr(keys::BOOSTS_TITLE), IconName::Zap),
        E::Monetization => (tr(keys::CHANNEL_EARN_TITLE), IconName::CircleDollarSign),
        E::StoryArchive => (tr(keys::STORIES_ARCHIVE_BUTTON), IconName::Archive),
        E::Manage(ManageLabel::Bot) => (tr(keys::MANAGE_BOT_TITLE), IconName::Settings),
        E::Manage(ManageLabel::Group) => (tr(keys::MANAGE_GROUP_TITLE), IconName::Settings),
        E::Manage(ManageLabel::Channel) => (tr(keys::MANAGE_CHANNEL_TITLE), IconName::Settings),
        E::SetPersonalChannel => (
            tr(keys::EDIT_CHANNEL_PERSONAL_CHANNEL),
            IconName::CircleUser,
        ),
        E::ViewDiscussion => (tr(keys::PROFILE_VIEW_DISCUSSION), IconName::MessagesSquare),
        E::DirectMessages => (tr(keys::PROFILE_DIRECT_MESSAGES), IconName::MessageCircle),
        E::Export => (tr(keys::PROFILE_EXPORT_CHAT), IconName::Download),
        E::ToggleSharing { disabled: false } => (tr(keys::DISABLE_SHARING), IconName::EyeOff),
        E::ToggleSharing { disabled: true } => (tr(keys::ENABLE_SHARING), IconName::Share2),
        E::AddToFolder => (tr(keys::FILTERS_MENU_ADD), IconName::Folder),
        E::Block { bot, blocked } => block_label(bot, blocked),
        E::Report => (tr(keys::PROFILE_REPORT), IconName::Flag),
        E::Leave { channel: true } => (tr(keys::PROFILE_LEAVE_CHANNEL), IconName::LogOut),
        E::Leave { channel: false } => (tr(keys::PROFILE_LEAVE_GROUP), IconName::LogOut),
        E::DeleteContact => (tr(keys::INFO_DELETE_CONTACT), IconName::Trash),
    }
}

fn block_label(bot: bool, blocked: bool) -> (&'static str, IconName) {
    match (bot, blocked) {
        (true, true) => (tr(keys::PROFILE_RESTART_BOT), IconName::RotateCcw),
        (true, false) => (tr(keys::PROFILE_BLOCK_BOT), IconName::Ban),
        (false, true) => (tr(keys::PROFILE_UNBLOCK_USER), IconName::UserCheck),
        (false, false) => (tr(keys::PROFILE_BLOCK_USER), IconName::Ban),
    }
}
