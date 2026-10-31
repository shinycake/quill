//! Tells the session which chats' histories the UI still reads while no
//! view shows them, so closed-chat pruning (`state::history_prune`) keeps
//! them whole: the source of a pending forward, the chat a reply bar points
//! at ("Reply in Another Chat" replies to a message of a chat that is no
//! longer shown), the chat of the track in the player bar, and the chats
//! whose side tables something open reads: the group the info panel shows,
//! the chat of an ad being reported, and the chat whose member lists a
//! management box reads (they go with the chat's history otherwise, and
//! these stay open when the main chat changes under them). A chat that
//! closes while the claim is not synced yet waits in the session's release
//! queue until this runs, so a claim and a chat switch in one event cannot
//! trim the source.
use super::*;
use quill::ids::ChatId;

impl QuillApp {
    pub(super) fn sync_history_holds(&mut self) {
        let holds: Vec<ChatId> = self
            .share
            .pending_forward
            .as_ref()
            .map(|draft| draft.from_chat_id)
            .into_iter()
            .chain(self.playback.player.chat)
            .chain(self.composer_ui.reply().map(|reply| reply.chat_id))
            .chain(self.member_list_readers())
            .chain(self.session().into_iter().flat_map(|session| {
                [session.info_panel_chat(), session.sponsored_report_chat()]
                    .into_iter()
                    .flatten()
            }))
            .collect();
        let Some(session) = self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .or(self.demo_session.as_deref_mut())
        else {
            return;
        };
        session.set_history_holds(holds);
    }

    /// The chats whose member lists an open box reads: the member dialog,
    /// the admin and exception pickers, the restrict and ownership
    /// dialogs, and the Edit box's Manage rows.
    fn member_list_readers(&self) -> impl Iterator<Item = ChatId> {
        let admin = &self.admin;
        [
            admin.member_dialog.as_ref().map(|dialog| dialog.chat_id),
            admin.add_special.as_ref().map(|dialog| dialog.chat_id),
            admin.admin_dialog.as_ref().map(|dialog| dialog.chat_id),
            admin.restrict_dialog.as_ref().map(|dialog| dialog.chat_id),
            admin.ownership_dialog.as_ref().map(|dialog| dialog.chat_id),
            self.group_edit_box().map(|edit| edit.chat_id),
        ]
        .into_iter()
        .flatten()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "history_holds_tests.rs"]
mod tests;
