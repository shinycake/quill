//! Connect driver: editing an owned bot and creating a managed one
//! (`crate::owned_bots`).
use super::*;
use crate::ids::RequestId;
use crate::owned_bots::{self, EditChanges};
use crate::state::{
    BotEditTexts, BotSave, BotUsernameCheck, BotsPurpose, CreateStage, CreateSubmission,
    ManagedBotShare, RequestPurpose,
};

impl<S: JsonSender> ConnectDriver<S> {
    fn owned_bots_ready(&self, bot_user_id: i64) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || bot_user_id <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(())
    }

    /// Open the edit dialog's state for `bot_user_id` and load its name and
    /// about text in the default language.
    pub fn begin_bot_edit(
        &mut self,
        bot_user_id: i64,
    ) -> Result<(RequestId, RequestId), ConnectSendError> {
        self.owned_bots_ready(bot_user_id)?;
        let mut ask = |about| {
            self.session.request(
                RequestPurpose::Bots(BotsPurpose::GetBotEditText { bot_user_id, about }),
                None,
            )
        };
        let (name, about_extra) = (ask(false), ask(true));
        self.session.bots.owned.edit = Some(BotEditTexts::new(bot_user_id, name, about_extra));
        for (about, extra) in [(false, name), (true, about_extra)] {
            let json = if about {
                owned_bots::get_about_request(&extra.as_extra(), bot_user_id)
            } else {
                owned_bots::get_name_request(&extra.as_extra(), bot_user_id)
            };
            if let Err(err) = self.send_json_request(extra, &json) {
                // Half a load never answers: drop the dialog's state and
                // the load that did go out (the caller says it failed).
                self.session.bots.owned.edit = None;
                self.session.requests.take(name);
                self.session.requests.take(about_extra);
                return Err(err);
            }
        }
        Ok((name, about_extra))
    }

    /// Send the changed fields (`setBotName`, `setBotInfoShortDescription`).
    pub fn save_bot_edit(
        &mut self,
        bot_user_id: i64,
        changes: &EditChanges,
    ) -> Result<(), ConnectSendError> {
        self.owned_bots_ready(bot_user_id)?;
        if let Some(edit) = self.session.bots.owned.edit.as_mut() {
            edit.forget_failed_saves();
        }
        if let Some(name) = &changes.name {
            let extra = self.session.request(
                RequestPurpose::Bots(BotsPurpose::SetBotName { bot_user_id }),
                None,
            );
            self.begin_save(extra, |edit| {
                edit.name_save = BotSave::Saving {
                    request: extra,
                    text: name.clone(),
                }
            });
            let json = owned_bots::set_name_request(&extra.as_extra(), bot_user_id, name);
            self.send_save(extra, &json)?;
        }
        if let Some(about) = &changes.about {
            let extra = self.session.request(
                RequestPurpose::Bots(BotsPurpose::SetBotAbout { bot_user_id }),
                None,
            );
            self.begin_save(extra, |edit| {
                edit.about_save = BotSave::Saving {
                    request: extra,
                    text: about.clone(),
                }
            });
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
        self.begin_save(extra, |edit| {
            edit.photo_save = BotSave::Saving {
                request: extra,
                text: String::new(),
            }
        });
        let json = owned_bots::set_photo_request(&extra.as_extra(), bot_user_id, path);
        self.send_save(extra, &json)
    }

    fn begin_save(&mut self, _extra: RequestId, fill: impl FnOnce(&mut BotEditTexts)) {
        if let Some(edit) = self.session.bots.owned.edit.as_mut() {
            fill(edit);
        }
    }

    /// Send a save; a refused send is no longer waited for.
    fn send_save(&mut self, extra: RequestId, json: &str) -> Result<(), ConnectSendError> {
        let sent = self.send_json_request(extra, json).map(|_| ());
        if sent.is_err()
            && let Some(edit) = self.session.bots.owned.edit.as_mut()
        {
            for save in [
                &mut edit.name_save,
                &mut edit.about_save,
                &mut edit.photo_save,
            ] {
                if matches!(save, BotSave::Saving { request, .. } if *request == extra) {
                    *save = BotSave::Idle;
                }
            }
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
        create.check = BotUsernameCheck::Checking {
            generation,
            username: username.to_string(),
        };
        let extra = self.session.request(
            RequestPurpose::Bots(BotsPurpose::CheckBotUsername { generation }),
            None,
        );
        let json = owned_bots::check_username_request(&extra.as_extra(), username);
        let sent = self.send_json_request(extra, &json);
        if sent.is_err() {
            self.session.bots.owned.create.check = BotUsernameCheck::Idle;
        }
        sent
    }

    /// `createBot`, managed by `manager_bot_user_id`. The new bot's id lands
    /// in `Session::bots.owned.create.stage` as `Created`, with what the
    /// press needs to report it (`share`: the button it answers).
    pub fn create_managed_bot(
        &mut self,
        manager_bot_user_id: i64,
        name: &str,
        username: &str,
        via_link: bool,
        share: Option<ManagedBotShare>,
    ) -> Result<RequestId, ConnectSendError> {
        self.owned_bots_ready(manager_bot_user_id)?;
        let create = &mut self.session.bots.owned.create;
        // One create at a time, owned here and not by the Create button: a
        // second `createBot` would drop the first's stage and its answer,
        // and a second bot could be made unreported.
        if matches!(create.stage, CreateStage::Creating(_)) {
            return Err(ConnectSendError::InvalidRequest);
        }
        create.generation = create.generation.wrapping_add(1);
        let generation = create.generation;
        create.stage = CreateStage::Creating(CreateSubmission {
            generation,
            name: name.to_string(),
            username: username.to_string(),
            manager_id: manager_bot_user_id,
            share,
        });
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
            self.session.bots.owned.create.stage = CreateStage::Editing;
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

    /// The bot a `createBot` made, once: the UI reports it. Any other stage
    /// stays as it is.
    pub fn take_created_bot(&mut self) -> Option<(CreateSubmission, i64)> {
        let create = &mut self.session.bots.owned.create;
        match std::mem::take(&mut create.stage) {
            CreateStage::Created {
                submission,
                user_id,
            } => Some((submission, user_id)),
            other => {
                create.stage = other;
                None
            }
        }
    }
}
