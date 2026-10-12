//! Connect driver: the boost box (`crate::boost_box`).
use super::*;
use crate::boost_box::{BoostBoxState, BoostPhase, ReassignSummary, sources};

impl<S: JsonSender> ConnectDriver<S> {
    /// The open boost box, if it is for this chat.
    pub(super) fn boost_box_for(&mut self, chat_id: ChatId) -> Option<&mut BoostBoxState> {
        self.session
            .groups
            .boost_box
            .as_mut()
            .filter(|state| state.chat_id == chat_id.0)
    }

    /// Open the boost box for a channel or supergroup: the level now
    /// (`getChatBoostStatus`, as tdesktop's `resolveBoostState`) and the
    /// level table. Nothing is boosted. `Ok(false)` for a chat that cannot be
    /// boosted.
    pub fn open_boost_box(&mut self, chat_id: ChatId) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(ChatKind::Supergroup { is_channel, .. }) = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.kind.clone())
        else {
            return Ok(false);
        };
        self.session.groups.boost_box = Some(BoostBoxState::new(chat_id.0, !is_channel));
        self.session.groups.chat_boost_status.remove(&chat_id.0);
        self.session.groups.chat_boost_details.remove(&chat_id.0);
        self.fetch_chat_boost_status(chat_id)?;
        self.fetch_chat_boost_features(chat_id)?;
        Ok(true)
    }

    /// Close the box. A request in flight still lands, and the reducer
    /// ignores its box.
    pub fn close_boost_box(&mut self) {
        self.session.groups.boost_box = None;
        self.session.groups.boost_intent = None;
    }

    /// Back from the reassign list to the counters.
    pub fn cancel_boost_reassign(&mut self) {
        if let Some(state) = self.session.groups.boost_box.as_mut()
            && matches!(state.phase, BoostPhase::Reassign { .. })
        {
            state.phase = BoostPhase::Main;
        }
    }

    /// The Reassign (or Replace) click: `boostChat` with the picked slots,
    /// which moves those boosts to this chat. Only the box's own list can
    /// be sent, and never a slot that is cooling down.
    pub fn reassign_boosts(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let now = crate::local_time::now_unix();
        let Some(state) = self.session.groups.boost_box.as_ref() else {
            return Ok(false);
        };
        let BoostPhase::Reassign { slots, .. } = &state.phase else {
            return Ok(false);
        };
        let picked = state.reassign_selection(now);
        if picked.is_empty() {
            return Ok(false);
        }
        let session = &self.session;
        let (groups, channels) = sources(slots, &picked, |id| session.chat_is_group(ChatId(id)));
        let chat = ChatId(state.chat_id);
        let summary = ReassignSummary {
            count: picked.len(),
            groups,
            channels,
        };
        if let Some(state) = self.boost_box_for(chat) {
            state.phase = BoostPhase::Working;
            state.note = None;
            state.reassigning = Some(summary);
        }
        self.send_boost_chat(chat, &picked)?;
        Ok(true)
    }
}
