//! The chat-row menu entries tdesktop builds in `window_peer_menu.cpp`
//! beyond a plain action: the Mute submenu (`PeerMenuAddMuteSubmenuAction`,
//! `FillMuteMenu`), Add to folder (`Filler::addToggleFolder`,
//! `FillChooseFilterMenu`) and the delete or leave entry that fits the
//! chat kind (`addDeleteChat`, `addLeaveChat`).

use super::*;
use crate::ui::menu_states::{ChatMenuState, ChatMenuSubmenu};
use gpui_kit::TestSupportExt;
use gpui_kit::assets::IconName;
use quill::chatlist_menu::{ExitEntry, exit_entry, mute_offered};
use quill::lang::{keys, tr, tr_with, trn};
use quill::state::ChatSummary;
use quill::telegram::envelope::{
    ChannelMemberStatus, MUTE_FOR_1_HOUR, MUTE_FOR_2_DAYS, MUTE_FOR_8_HOURS, MUTE_FOREVER,
};

/// Width of the menu panel; a submenu opens beside it.
pub(super) const MENU_WIDTH: f32 = 240.;
const PANEL_PADDING: f32 = 4.;

/// "1 hour", "8 hours", "2 days": `Ui::FormatMuteFor`.
fn period_text(seconds: i32) -> String {
    let day = 24 * 3600;
    if seconds % day == 0 {
        trn(keys::DAYS, seconds / day)
    } else {
        trn(keys::HOURS, seconds / 3600)
    }
}

/// One menu row: icon, label, optional trailing marker. Danger rows read
/// in the attention colour.
pub(super) fn menu_row(
    id: impl Into<ElementId>,
    icon: Option<IconName>,
    label: String,
    danger: bool,
    hover: Hsla,
) -> impl ParentElement + IntoElement + StatefulInteractiveElement + Styled {
    div()
        .id(id)
        .test_support()
        .relative()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .text_sm()
        .text_color(if danger { danger_bright() } else { text_menu() })
        .hover(|style| style.bg(hover))
        .role(gpui_kit::Role::MenuItem)
        .aria_label(label.clone())
        .child(match icon {
            Some(icon) => Icon::new(icon).size(px(16.)).into_any_element(),
            None => div().size(px(16.)).into_any_element(),
        })
        .child(div().flex_1().child(label))
}

fn submenu_panel(rows: Vec<AnyElement>) -> AnyElement {
    anchored()
        .offset(point(px(MENU_WIDTH - PANEL_PADDING), px(-PANEL_PADDING)))
        .snap_to_window_with_margin(px(8.))
        .child(
            div()
                .id("chat-menu-submenu")
                .occlude()
                .flex()
                .flex_col()
                .w(px(MENU_WIDTH))
                .p_1()
                .rounded_md()
                .border_1()
                .border_color(accent())
                .bg(bg_canvas())
                .children(rows),
        )
        .into_any_element()
}

impl QuillApp {
    fn open_chat_submenu(&mut self, which: ChatMenuSubmenu, cx: &mut Context<Self>) {
        if let Some(menu) = self.chat_list.menu.as_mut()
            && menu.submenu != Some(which)
        {
            menu.submenu = Some(which);
            cx.notify();
        }
    }

    /// Hovering a plain menu row folds an open submenu away.
    pub(super) fn chat_menu_row_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if !hovered {
            return;
        }
        if let Some(menu) = self.chat_list.menu.as_mut()
            && menu.submenu.take().is_some()
        {
            cx.notify();
        }
    }

    fn close_chat_menu(&mut self, cx: &mut Context<Self>) {
        self.chat_list.menu = None;
        cx.notify();
    }

    /// "Select tone" and "Mute for..." go through the per-chat notification
    /// panel, which owns the sound list and the custom duration.
    fn open_chat_notification_panel(
        &mut self,
        chat_id: ChatId,
        custom: bool,
        cx: &mut Context<Self>,
    ) {
        self.chat_list.menu = None;
        self.open_mute_menu_for(chat_id, custom, cx);
    }

    /// The Mute row (order 30): a direct "Unmute" while muted, otherwise a
    /// "Mute notifications" row that opens the duration submenu. `None`
    /// where tdesktop offers no mute (Saved Messages, forums).
    pub(super) fn chat_menu_mute_row(
        &self,
        menu: ChatMenuState,
        chat: &ChatSummary,
        hover: Hsla,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let saved = self
            .session()
            .is_some_and(|session| session.is_saved_messages(menu.chat_id));
        if !mute_offered(saved, chat.is_forum_chat()) {
            return None;
        }
        let chat_id = menu.chat_id;
        // tdesktop's `isMuted` counts the scope default too.
        let muted = self
            .session()
            .is_some_and(|session| session.effective_muted(chat));
        if muted {
            return Some(
                menu_row(
                    "chat-menu-mute",
                    Some(IconName::Bell),
                    tr(keys::MUTE_MENU_UNMUTE).to_string(),
                    false,
                    hover,
                )
                .on_hover(cx.listener(|this, hovered, _, cx| {
                    this.chat_menu_row_hovered(*hovered, cx);
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_chat_mute(chat_id, 0, cx);
                    this.close_chat_menu(cx);
                }))
                .into_any_element(),
            );
        }
        let settings = &chat.notification_settings;
        let sound_off = !settings.use_default_sound && settings.sound_id == 0;
        let open = menu.submenu == Some(ChatMenuSubmenu::Mute);
        let sub_item = |id: &'static str, icon: IconName, label: String| {
            menu_row(id, Some(icon), label, false, hover)
        };
        let mut rows: Vec<AnyElement> = vec![
            sub_item(
                "chat-mute-tone",
                IconName::Music,
                tr(keys::MUTE_MENU_SOUND_SELECT).to_string(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_chat_notification_panel(chat_id, false, cx);
            }))
            .into_any_element(),
            sub_item(
                "chat-mute-sound",
                if sound_off {
                    IconName::Volume2
                } else {
                    IconName::VolumeX
                },
                if sound_off {
                    tr(keys::MUTE_MENU_SOUND_ON)
                } else {
                    tr(keys::MUTE_MENU_SOUND_OFF)
                }
                .to_string(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                let (use_default, sound_id) = quill::mute_menu::sound_toggle_target(sound_off);
                this.apply_chat_sound(chat_id, use_default, sound_id, cx);
                this.close_chat_menu(cx);
            }))
            .into_any_element(),
        ];
        for (id, seconds) in [
            ("chat-mute-1h", MUTE_FOR_1_HOUR),
            ("chat-mute-8h", MUTE_FOR_8_HOURS),
            ("chat-mute-2d", MUTE_FOR_2_DAYS),
        ] {
            rows.push(
                sub_item(
                    id,
                    IconName::BellOff,
                    tr_with(
                        keys::MUTE_MENU_DURATION_ANY,
                        &[("duration", period_text(seconds).as_str())],
                    ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_chat_mute(chat_id, seconds, cx);
                    this.close_chat_menu(cx);
                }))
                .into_any_element(),
            );
        }
        rows.push(
            sub_item(
                "chat-mute-custom",
                IconName::Timer,
                tr(keys::MUTE_MENU_DURATION).to_string(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_chat_notification_panel(chat_id, true, cx);
            }))
            .into_any_element(),
        );
        rows.push(
            sub_item(
                "chat-mute-forever",
                IconName::BellOff,
                tr(keys::MUTE_MENU_DURATION_FOREVER).to_string(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apply_chat_mute(chat_id, MUTE_FOREVER, cx);
                this.close_chat_menu(cx);
            }))
            .into_any_element(),
        );
        Some(
            menu_row(
                "chat-menu-mute",
                Some(IconName::BellOff),
                tr(keys::Q_CHAT_MUTE_NOTIFICATIONS).to_string(),
                false,
                hover,
            )
            .child(Icon::new(IconName::ChevronRight).size(px(14.)))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if *hovered {
                    this.open_chat_submenu(ChatMenuSubmenu::Mute, cx);
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.open_chat_submenu(ChatMenuSubmenu::Mute, cx);
            }))
            .when(open, |row| row.child(submenu_panel(rows)))
            .into_any_element(),
        )
    }

    /// The "Add to folder" row (order 45) with a submenu that lists every
    /// folder, a check on the ones holding the chat. `None` without folders.
    pub(super) fn chat_menu_folder_row(
        &self,
        menu: ChatMenuState,
        chat: &ChatSummary,
        hover: Hsla,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let folders = &session.chat_list.chat_folders;
        if folders.is_empty() || !chat.supported() {
            return None;
        }
        let chat_id = menu.chat_id;
        let open = menu.submenu == Some(ChatMenuSubmenu::Folders);
        let rows: Vec<AnyElement> = folders
            .iter()
            .map(|folder| {
                let folder_id = folder.id;
                let contains = chat.folder_positions.contains_key(&folder_id);
                menu_row(
                    format!("chat-menu-folder-{folder_id}"),
                    contains.then_some(IconName::Check),
                    folder.name.clone(),
                    false,
                    hover,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if contains {
                        this.remove_open_chat_from_folder(chat_id, folder_id, cx);
                    } else {
                        this.add_open_chat_to_folder(chat_id, folder_id, cx);
                    }
                    this.close_chat_menu(cx);
                }))
                .into_any_element()
            })
            .collect();
        Some(
            menu_row(
                "chat-menu-folder",
                Some(IconName::Folder),
                tr(keys::FILTERS_MENU_ADD).to_string(),
                false,
                hover,
            )
            .child(Icon::new(IconName::ChevronRight).size(px(14.)))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if *hovered {
                    this.open_chat_submenu(ChatMenuSubmenu::Folders, cx);
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.open_chat_submenu(ChatMenuSubmenu::Folders, cx);
            }))
            .when(open, |row| row.child(submenu_panel(rows)))
            .into_any_element(),
        )
    }

    /// The delete or leave row (order 90): "Delete chat" for a user,
    /// "Delete and leave" for a basic group, "Leave group" / "Leave
    /// channel" for a supergroup or channel the user is still in.
    pub(super) fn chat_menu_exit_row(
        &self,
        chat: &ChatSummary,
        can_delete: bool,
        hover: Hsla,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let still_in = !matches!(
            chat.my_member_status,
            Some(ChannelMemberStatus::Left | ChannelMemberStatus::Banned)
        );
        let entry = exit_entry(&chat.kind, still_in)?;
        let chat_id = chat.id;
        let (label, icon, leave) = match entry {
            ExitEntry::DeleteChat if can_delete => {
                (tr(keys::Q_CHAT_DELETE_CHAT), IconName::Trash, false)
            }
            ExitEntry::DeleteAndLeave if can_delete => {
                (tr(keys::PROFILE_CLEAR_AND_EXIT), IconName::Trash, false)
            }
            ExitEntry::Leave { channel } => (
                if channel {
                    tr(keys::PROFILE_LEAVE_CHANNEL)
                } else {
                    tr(keys::PROFILE_LEAVE_GROUP)
                },
                IconName::LogOut,
                true,
            ),
            _ => return None,
        };
        Some(
            menu_row(
                if leave {
                    "chat-menu-leave"
                } else {
                    "chat-menu-delete"
                },
                Some(icon),
                label.to_string(),
                true,
                hover,
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.chat_menu_row_hovered(*hovered, cx);
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                let action = if leave {
                    GroupConfirmAction::LeaveChat
                } else {
                    GroupConfirmAction::RemoveFromList
                };
                this.open_group_confirm(chat_id, action, cx);
                this.chat_list.menu = None;
                cx.notify();
            }))
            .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "chat_menu_submenus_tests.rs"]
mod tests;
