//! Methods moved out of `message_menu_ui.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// "Delete" on a row of the who-reacted list: the admin removes that
    /// member's reaction (`deleteMessageReactionsFromSender`), then the
    /// list is fetched again.
    pub(in crate::ui) fn delete_member_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        sender: MessageSender,
        cx: &mut Context<Self>,
    ) {
        self.connection.status_note = match self.live.as_mut() {
            Some(live) => match live
                .driver
                .delete_message_reactions_from(chat_id, message_id, sender)
            {
                Ok(Some(_)) => "deleting reaction…".into(),
                _ => "could not delete the reaction".into(),
            },
            None => "reaction deletion needs a live connection (demo)".into(),
        };
        cx.notify();
    }

    /// The admin checkboxes the delete box adds for `message_id`
    /// (`boxes/moderate_messages_box.cpp`): Report Spam, Delete all from
    /// the user, Ban the user. `None` when nothing applies.
    pub(in crate::ui) fn moderation_offer(
        &self,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
        actions: Option<MessageActions>,
    ) -> Option<ModerationOffer> {
        use quill::moderation::{GroupFlavor, ModerateInput, moderate_options};
        let session = self.session()?;
        let flavor = self.group_flavor(chat_id)?;
        let MessageSender::User { user_id } = message.sender? else {
            return None;
        };
        let actions = actions?;
        // The sender's standing, when the admin list is loaded; a plain
        // member otherwise (TDLib rejects a ban it does not allow).
        let (sender_status, sender_can_be_edited) = match session.groups.admin_lists.get(&chat_id.0)
        {
            Some(quill::state::AdminListFetch::Loaded(list)) => list
                .iter()
                .find(|entry| entry.user_id == user_id)
                .map_or((ChannelMemberStatus::Member, false), |entry| {
                    if entry.is_owner {
                        (ChannelMemberStatus::Creator, false)
                    } else {
                        (ChannelMemberStatus::Administrator, entry.can_be_edited)
                    }
                }),
            _ => (ChannelMemberStatus::Member, false),
        };
        let options = moderate_options(&ModerateInput {
            flavor,
            sender_is_user: true,
            sender_is_self: message.is_outgoing || session.my_user_id == Some(user_id),
            can_report_spam: actions.can_report_supergroup_spam,
            can_delete_for_all: actions.can_be_deleted_for_all_users,
            // Reactions are removed from the who-reacted list, not here.
            can_delete_reactions: false,
            viewer_can_restrict: session.chat_can_restrict_members(chat_id),
            sender_status,
            sender_can_be_edited,
        });
        options.any().then(|| ModerationOffer {
            chat_id,
            user_id,
            user_name: session
                .user(user_id)
                .map(|u| u.display_name())
                .unwrap_or_else(|| "this user".into()),
            options,
            can_restrict_instead: flavor == GroupFlavor::Supergroup && options.ban_or_restrict,
        })
    }
}
