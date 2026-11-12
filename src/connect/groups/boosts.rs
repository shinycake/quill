//! Connect driver: chat boosts.
use super::*;
use crate::boost_box::{BoostDecision, BoostPhase, BoostWork, ReassignSummary, classify, decide};
use crate::telegram::requests::{get_chat_boost_features, set_supergroup_unrestrict_boost_count};

impl<S: JsonSender> ConnectDriver<S> {
    /// A channel's `boost_level` moved (`updateSupergroup`): ask again for
    /// the boost status of every chat whose cached status is now stale, so an
    /// open boost box, profile row or look dialog shows the new level.
    pub(crate) fn refresh_moved_boost_statuses(&mut self) {
        for chat_id in self.session.outdated_boost_chats() {
            // A status asked before the move would hold the old level: its
            // request is dropped, and the one sent now answers.
            self.session
                .requests
                .take_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id);
            let _ = self.fetch_chat_boost_status(chat_id);
        }
    }

    /// Slice G2: `getChatBoostStatus` (schema 1.8.67, line 13917) —
    /// cached per chat (level + boost count) for the boost dialog.
    pub fn fetch_chat_boost_status(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if !self.session.groups.boost_status_wanted(chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatBoostStatus, Some(chat_id));
        let fetch = self
            .session
            .groups
            .chat_boost_status
            .entry(chat_id.0)
            .or_default();
        fetch.begin_with(extra);
        if let Err(err) = self
            .sender
            .send_json(&get_chat_boost_status(extra, chat_id))
        {
            self.session.requests.take(extra);
            if let Some(fetch) = self.session.groups.chat_boost_status.get_mut(&chat_id.0) {
                fetch.failed(
                    Some(extra),
                    crate::state::send_failed_line("Could not load boost status"),
                    false,
                );
            }
            return Err(err);
        }
        let fetch = self
            .session
            .groups
            .chat_boost_status
            .entry(chat_id.0)
            .or_default();
        fetch.begin_with(extra);
        Ok(Some(extra))
    }

    /// Slice G2: boost the chat. Sends `getAvailableChatBoostSlots`
    /// (schema 1.8.67, line 13914); the driver chains `boostChat` with
    /// the first slot once the answer arrives (`maybe_continue_boost`).
    /// Deduped while an intent or either request is in flight.
    pub fn request_chat_boost(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(ChatKind::Supergroup { is_channel, .. }) =
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind)
        else {
            return Ok(None);
        };
        let group = !*is_channel;
        // The press belongs to the box of this chat, which holds what it
        // waits for (a press with no box for the chat makes one); a press
        // while the box already works does nothing.
        match self.boost_box_for(chat_id) {
            Some(state) if matches!(state.phase, BoostPhase::Working(_)) => return Ok(None),
            Some(_) => {}
            None => {
                self.session.groups.boost_box =
                    Some(crate::boost_box::BoostBoxState::new(chat_id.0, group));
            }
        }
        // A slots read from a press whose box closed is still out: this
        // press waits for that request, and its answer continues this boost.
        if let Some(out) = self
            .session
            .requests
            .id_for_purpose_for_chat(RequestPurpose::GetBoostSlotsForBoost, chat_id)
        {
            if let Some(state) = self.boost_box_for(chat_id) {
                state.reading_slots(out);
            }
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetBoostSlotsForBoost, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_available_chat_boost_slots(extra))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(state) = self.boost_box_for(chat_id) {
            state.reading_slots(extra);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `boostChat` chain — once the slots answer for a pending
    /// boost intent arrives, sort the slots (`crate::boost_box`). A free
    /// slot sends `boostChat` with it; otherwise the open boost box shows
    /// what tdesktop shows (reassign list, "already boosted", Premium
    /// needed). `boostChat` errors are reported by the reducer.
    pub(crate) fn maybe_continue_boost(&mut self) -> Result<(), ConnectSendError> {
        let Some(state) = self.session.groups.boost_box.as_mut() else {
            return Ok(());
        };
        if !matches!(state.phase, BoostPhase::Working(BoostWork::SlotsReady(_))) {
            return Ok(());
        }
        let chat_id = state.chat_id;
        let BoostPhase::Working(BoostWork::SlotsReady(slots)) =
            std::mem::replace(&mut state.phase, BoostPhase::main())
        else {
            return Ok(());
        };
        let now = crate::local_time::now_unix();
        let choice = classify(chat_id, &slots, now);
        let session = &self.session;
        let receive = session.groups.boosts_per_gift.data().copied().unwrap_or(0);
        let decision = decide(&choice, session.my_is_premium(), receive, now, |id| {
            session.chat_is_group(ChatId(id))
        });
        match decision {
            BoostDecision::Boost(slot_id) => {
                self.send_boost_chat(ChatId(chat_id), &[slot_id], None)?;
            }
            other => {
                if let Some(state) = self.boost_box_for(ChatId(chat_id)) {
                    state.apply_decision(other);
                }
            }
        }
        Ok(())
    }

    /// `boostChat` for these slots. Once it is out the box works on that
    /// request, and its answer (or error) returns the box to its counters;
    /// a send that is refused leaves the box as it was.
    pub(super) fn send_boost_chat(
        &mut self,
        chat: ChatId,
        slot_ids: &[i32],
        reassign: Option<ReassignSummary>,
    ) -> Result<(), ConnectSendError> {
        let extra = self.session.request(RequestPurpose::BoostChat, Some(chat));
        if let Err(err) = self.sender.send_json(&boost_chat(extra, chat, slot_ids)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(state) = self.boost_box_for(chat) {
            state.boosting(extra, reassign);
        }
        Ok(())
    }

    /// `getApplicationConfig`, once, for `boosts_per_sent_gift`.
    pub fn fetch_boosts_per_gift(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.groups.boosts_per_gift.can_ask() {
            return Ok(());
        }
        let extra = self.session.request(RequestPurpose::GetBoostsPerGift, None);
        let fetch = &mut self.session.groups.boosts_per_gift;
        fetch.begin_with(extra);
        if let Err(err) = self
            .sender
            .send_json(&crate::telegram::requests::get_application_config(extra))
        {
            self.session.requests.take(extra);
            self.session.groups.boosts_per_gift.failed(
                Some(extra),
                crate::state::send_failed_line("Could not load the boost settings"),
                false,
            );
            return Err(err);
        }
        Ok(())
    }

    /// `getChatBoostFeatures`: the level table for this kind of chat,
    /// cached per chat.
    pub fn fetch_chat_boost_features(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(ChatKind::Supergroup { is_channel, .. }) = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.kind.clone())
        else {
            return Ok(None);
        };
        if self
            .session
            .groups
            .chat_boost_features
            .contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostFeatures, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatBoostFeatures, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_boost_features(extra, is_channel))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `setSupergroupUnrestrictBoostCount` (0 turns the setting off).
    /// Refused as `Ok(None)` without the right to restrict members or
    /// for a value TDLib would reject.
    pub fn set_unrestrict_boost_count(
        &mut self,
        chat_id: ChatId,
        count: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !(0..=crate::state::UNRESTRICT_BOOST_MAX).contains(&count)
            || !self.session.chat_can_edit_unrestrict_boosts(chat_id)
        {
            return Ok(None);
        }
        let Some(supergroup_id) = self.session.chat_supergroup(chat_id) else {
            return Ok(None);
        };
        let purpose = RequestPurpose::SetSupergroupUnrestrictBoostCount;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_supergroup_unrestrict_boost_count(
                extra,
                supergroup_id,
                count,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.groups.unrestrict_boost_saves.insert(
            chat_id.0,
            crate::state::UnrestrictSave {
                request: extra,
                value: count,
            },
        );
        Ok(Some(extra))
    }
}
