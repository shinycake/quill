//! Tells the session which chats' histories the UI still reads while no
//! view shows them, so closed-chat pruning (`state::history_prune`) keeps
//! them whole: the source of a pending forward or selection, and the chat
//! of the track in the player bar.
use super::*;

impl QuillApp {
    pub(super) fn sync_history_holds(&mut self) {
        let holds: Vec<ChatId> = self
            .share
            .pending_forward
            .as_ref()
            .map(|draft| draft.from_chat_id)
            .into_iter()
            .chain(self.playback.player.chat)
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
