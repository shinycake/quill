//! Member context menu in the member list: Mention, Search messages,
//! member tag, Promote / Edit admin rights, Restrict, Ban, Remove, Unban.
//! Mirrors tdesktop's `ParticipantsBoxController::rowContextMenu`
//! (`boxes/peers/edit_participants_box.cpp`) and `FillSenderUserpicMenu`
//! (`window/window_peer_menu.cpp`). What the menu offers comes from
//! [`quill::moderation::member_menu_actions`], which is unit tested.

use super::app::QuillApp;
use super::dialogs::{AdminDialog, AdminDialogKind, GroupConfirmAction};
use super::shell::DialogKind;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::moderation::{
    GroupFlavor, MemberAction, MemberMenuContext, append_mention, member_menu_actions, mention_text,
};
use quill::telegram::envelope::{ChatKind, MessageSender, ParsedChatMember};
use std::rc::Rc;

/// A built menu body, shared by the row's right-click menu and its "more"
/// button.
pub(super) type MenuBuild =
    Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// tdesktop's wording for each item (`lng_context_*`, `lng_profile_kick`).
fn action_label(
    action: MemberAction,
    tag: &str,
    is_self: bool,
    flavor: GroupFlavor,
) -> &'static str {
    match action {
        MemberAction::Mention => "Mention",
        MemberAction::SearchMessages => "Search messages",
        MemberAction::EditTag => match (is_self, tag.is_empty()) {
            (true, true) => "Add my tag",
            (true, false) => "Edit my tag",
            (false, true) => "Add member tag",
            (false, false) => "Edit member tag",
        },
        MemberAction::Promote => "Promote to admin",
        MemberAction::EditAdminRights => "Edit admin rights",
        MemberAction::Restrict => "Restrict user",
        MemberAction::Ban => "Ban",
        MemberAction::Remove => {
            if flavor == GroupFlavor::Channel {
                "Remove from channel"
            } else {
                "Remove from group"
            }
        }
        MemberAction::Unban => "Unban",
        MemberAction::Unrestrict => "Unrestrict",
    }
}

impl QuillApp {
    pub(super) fn group_flavor(&self, chat_id: ChatId) -> Option<GroupFlavor> {
        match self.session()?.chats.get(&chat_id.0)?.kind {
            ChatKind::BasicGroup { .. } => Some(GroupFlavor::BasicGroup),
            ChatKind::Supergroup {
                is_channel: true, ..
            } => Some(GroupFlavor::Channel),
            ChatKind::Supergroup { .. } => Some(GroupFlavor::Supergroup),
            _ => None,
        }
    }

    /// Everything the menu rules need about this member and the viewer.
    pub(super) fn member_menu_context(
        &self,
        chat_id: ChatId,
        member: &ParsedChatMember,
    ) -> Option<MemberMenuContext> {
        let session = self.session()?;
        let flavor = self.group_flavor(chat_id)?;
        let (target_is_user, user_id) = match member.member_id {
            MessageSender::User { user_id } => (true, user_id),
            MessageSender::Chat { .. } => (false, 0),
        };
        Some(MemberMenuContext {
            flavor,
            target_is_user,
            target_is_self: session.my_user_id == Some(user_id),
            target_status: member.status,
            target_can_be_edited: member.can_be_edited,
            viewer_can_promote: session.chat_can_manage_admins(chat_id),
            viewer_can_restrict: session.chat_can_restrict_members(chat_id),
            viewer_can_manage_tags: session.chat_can_manage_tags(chat_id),
            chat_is_open: session.open_chat == Some(chat_id),
        })
    }

    /// The menu of one member row; `None` when nothing applies.
    pub(super) fn member_menu_build(
        &self,
        chat_id: ChatId,
        member: &ParsedChatMember,
        cx: &mut Context<Self>,
    ) -> Option<MenuBuild> {
        let MessageSender::User { user_id } = member.member_id else {
            return None;
        };
        let ctx = self.member_menu_context(chat_id, member)?;
        let actions = member_menu_actions(&ctx);
        if actions.is_empty() {
            return None;
        }
        let (tag, is_self, flavor) = (member.tag.clone(), ctx.target_is_self, ctx.flavor);
        let owner = cx.entity().downgrade();
        Some(Rc::new(move |mut menu, _, _| {
            let mut separated = false;
            for (index, action) in actions.iter().copied().enumerate() {
                // Info and tag items first, then a rule before the
                // admin tools (tdesktop groups them the same way).
                let admin_tool = matches!(
                    action,
                    MemberAction::Promote
                        | MemberAction::EditAdminRights
                        | MemberAction::Restrict
                        | MemberAction::Unrestrict
                        | MemberAction::Ban
                        | MemberAction::Remove
                        | MemberAction::Unban
                );
                if admin_tool && !separated && index > 0 {
                    menu = menu.separator();
                    separated = true;
                }
                let (owner, tag) = (owner.clone(), tag.clone());
                menu = menu.item(
                    PopupMenuItem::new(action_label(action, &tag, is_self, flavor)).on_click(
                        move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.run_member_action(chat_id, user_id, action, &tag, window, cx);
                            });
                        },
                    ),
                );
            }
            menu
        }))
    }

    /// Run one menu item. Tools that open a panel behind the member
    /// dialog close the dialog first.
    pub(super) fn run_member_action(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        action: MemberAction,
        tag: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            MemberAction::Mention => {
                self.leave_member_dialog(window, cx);
                self.mention_member(user_id, window, cx);
            }
            MemberAction::SearchMessages => {
                self.leave_member_dialog(window, cx);
                self.search_messages_from(user_id, window, cx);
            }
            MemberAction::EditTag => {
                self.open_custom_title_dialog(chat_id, user_id, tag, window, cx)
            }
            MemberAction::Promote => {
                self.leave_member_dialog(window, cx);
                self.open_promote_picker(chat_id, window, cx);
                if let Some(AdminDialog {
                    kind: AdminDialogKind::Promote { selected_user, .. },
                    ..
                }) = self.admin.admin_dialog.as_mut()
                {
                    *selected_user = Some(user_id);
                }
            }
            MemberAction::EditAdminRights => {
                self.leave_member_dialog(window, cx);
                self.open_rights_editor(chat_id, user_id, cx);
            }
            MemberAction::Restrict => {
                self.open_restrict_dialog(chat_id, user_id, false, window, cx)
            }
            MemberAction::Ban => self.open_restrict_dialog(chat_id, user_id, true, window, cx),
            MemberAction::Remove => {
                self.open_group_confirm(chat_id, GroupConfirmAction::RemoveMember { user_id }, cx)
            }
            MemberAction::Unban | MemberAction::Unrestrict => {
                self.unban_member(chat_id, user_id, cx)
            }
        }
    }

    /// Close the member dialog so what the action opens is not hidden
    /// behind it.
    fn leave_member_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_member_dialog(cx);
        self.close_kit_dialog_if_done(DialogKind::Member, window, cx);
    }

    /// "Mention": `@username` (or a text mention) goes into the composer.
    fn mention_member(&mut self, user_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some((username, name)) = self
            .session()
            .and_then(|s| s.user(user_id))
            .map(|user| (user.username.clone(), user.display_name()))
        else {
            self.connection.status_note = "can't mention this member yet".into();
            cx.notify();
            return;
        };
        let draft = self.composer_markup(cx);
        let next = append_mention(&draft, &mention_text(user_id, &username, &name));
        self.set_composer_markup(&next, window, cx);
        self.composer.update(cx, |input, cx| {
            let end = input.value().len();
            input.set_selected_range(end..end, cx);
            input.focus(window, cx);
        });
        let next = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&next);
        cx.notify();
    }

    /// "Search messages": open the in-chat search with this member as the
    /// sender filter (the same "From:" filter the search bar offers).
    fn search_messages_from(&mut self, user_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.open_chat_search_ui(window, cx);
        let open = self.session().is_some_and(|s| s.chat_search.open);
        if open {
            self.chat_search_pick_sender(Some(MessageSender::User { user_id }), window, cx);
        }
    }
}
