//! Tells the session which chats' histories the UI still reads while no
//! view shows them, so closed-chat pruning (`state::history_prune`) keeps
//! them whole: the source of a pending forward, the chat a reply bar points
//! at ("Reply in Another Chat" replies to a message of a chat that is no
//! longer shown) and the chat of the track in the player bar. A chat that
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
            .collect();
        let Some(session) = self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .or(self.demo_session.as_mut())
        else {
            return;
        };
        session.set_history_holds(holds);
    }
}
