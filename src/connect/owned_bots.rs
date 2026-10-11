//! Connect driver: editing an owned bot and creating a managed one
//! (`crate::owned_bots`).
use super::*;
use crate::ids::RequestId;
use crate::owned_bots::{self, EditChanges};
use crate::state::{BotEditTexts, BotsPurpose, RequestPurpose};

impl<S: JsonSender> ConnectDriver<S> {
    fn owned_bots_ready(&self, bot_user_id: i64) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || bot_user_id <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(())
    }

    /// Open the edit dialog's state for `bot_user_id` and load its name and
    /// about text in the default language.
    pub fn begin_bot_edit(&mut self, bot_user_id: i64) -> Result<(), ConnectSendError> {
        self.owned_bots_ready(bot_user_id)?;
        self.session.bots.owned.edit = Some(BotEditTexts {
            bot_id: bot_user_id,
            ..BotEditTexts::default()
        });
        for about in [false, true] {
            let extra = self.session.request(
                RequestPurpose::Bots(BotsPurpose::GetBotEditText { bot_user_id, about }),
                None,
            );
            let json = if about {
                owned_bots::get_about_request(&extra.as_extra(), bot_user_id)
            } else {
                owned_bots::get_name_request(&extra.as_extra(), bot_user_id)
            };
            self.send_json_request(extra, &json)?;
        }
        Ok(())
    }

    /// Send the changed fields (`setBotName`, `setBotInfoShortDescription`).
    pub fn save_bot_edit(
        &mut self,
        bot_user_id: i64,
        changes: &EditChanges,
    ) -> Result<(), ConnectSendError> {
        self.owned_bots_ready(bot_user_id)?;
        if let Some(edit) = self.session.bots.owned.edit.as_mut() {
            edit.error = None;
        }
        if let Some(name) = &changes.name {
            let extra = self.session.request(
                RequestPurpose::Bots(BotsPurpose::SetBotName { bot_user_id }),
                None,
            );
            self.begin_save(|edit| edit.pending_name = Some(name.clone()));
            let json = owned_bots::set_name_request(&extra.as_extra(), bot_user_id, name);
            self.send_save(extra, &json)?;
        }
        if let Some(about) = &changes.about {
            let extra = self.session.request(
                RequestPurpose::Bots(BotsPurpose::SetBotAbout { bot_user_id }),
                None,
            );
            self.begin_save(|edit| edit.pending_about = Some(about.clone()));
            let json = owned_bots::set_about_request(&extra.as_extra(), bot_user_id, about);
            self.send_save(extra, &json)?;
        }
        Ok(())
    }

    /// `setBotProfilePhoto` from a local image.
    pub fn set_bot_photo(&mut self, bot_user_id: i64, path: &str) -> Result<(), ConnectSendError> {
        self.owned_bots_ready(bot_user_id)?;
        let extra = self.session.request(
            RequestPurpose::Bots(BotsPurpose::SetBotPhoto { bot_user_id }),
            None,
        );
        self.begin_save(|_| {});
        let json = owned_bots::set_photo_request(&extra.as_extra(), bot_user_id, path);
        self.send_save(extra, &json)
    }

    fn begin_save(&mut self, fill: impl FnOnce(&mut BotEditTexts)) {
        if let Some(edit) = self.session.bots.owned.edit.as_mut() {
            edit.saving = edit.saving.saturating_add(1);
            fill(edit);
        }
    }

    /// Send a save; a refused send takes its count back.
    fn send_save(&mut self, extra: RequestId, json: &str) -> Result<(), ConnectSendError> {
        let sent = self.send_json_request(extra, json).map(|_| ());
        if sent.is_err()
            && let Some(edit) = self.session.bots.owned.edit.as_mut()
        {
            edit.saving = edit.saving.saturating_sub(1);
        }
        sent
    }

    /// Forget the edit dialog's state.
    pub fn end_bot_edit(&mut self) {
        self.session.bots.owned.edit = None;
    }

    /// `checkBotUsername` for the full username; the verdict lands in
    /// `Session::bots.owned.create.check`.
    pub fn check_new_bot_username(
        &mut self,
        username: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let create = &mut self.session.bots.owned.create;
        create.generation = create.generation.wrapping_add(1);
        let generation = create.generation;
        create.check_pending = Some(generation);
        create.check_username = username.to_string();
        let extra = self.session.request(
            RequestPurpose::Bots(BotsPurpose::CheckBotUsername { generation }),
            None,
        );
        let json = owned_bots::check_username_request(&extra.as_extra(), username);
        let sent = self.send_json_request(extra, &json);
        if sent.is_err() {
            self.session.bots.owned.create.check_pending = None;
        }
        sent
    }

    /// `createBot`, managed by `manager_bot_user_id`. The new bot's id lands
    /// in `Session::bots.owned.create.created`.
    pub fn create_managed_bot(
        &mut self,
        manager_bot_user_id: i64,
        name: &str,
        username: &str,
        via_link: bool,
    ) -> Result<RequestId, ConnectSendError> {
        self.owned_bots_ready(manager_bot_user_id)?;
        let create = &mut self.session.bots.owned.create;
        create.generation = create.generation.wrapping_add(1);
        let generation = create.generation;
        create.creating = Some(generation);
        create.created = None;
        create.error = None;
        let extra = self.session.request(
            RequestPurpose::Bots(BotsPurpose::CreateBot { generation }),
            None,
        );
        let json = owned_bots::create_request(
            &extra.as_extra(),
            manager_bot_user_id,
            name,
            username,
            via_link,
        );
        let sent = self.send_json_request(extra, &json);
        if sent.is_err() {
            self.session.bots.owned.create.creating = None;
        }
        sent
    }

    /// Reset the create dialog's state.
    pub fn reset_create_bot(&mut self) {
        let generation = self.session.bots.owned.create.generation;
        self.session.bots.owned.create = crate::state::CreateBotState {
            generation: generation.wrapping_add(1),
            ..Default::default()
        };
    }
}
