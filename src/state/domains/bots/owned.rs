//! Editing a bot the viewer owns and creating a managed one (see
//! `crate::owned_bots` for what tdesktop does).
use crate::state::*;

/// One of the bot's two texts as the server holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotText {
    /// Its `get…` request is out as this request; an answer for another
    /// dialog drops.
    Loading(RequestId),
    Ready(String),
    /// The load failed; the dialog says so instead of waiting forever.
    Failed,
}

impl BotText {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Ready(text) => Some(text),
            Self::Loading(_) | Self::Failed => None,
        }
    }
}

/// One save of the edit dialog (the name, the about text or the photo).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum BotSave {
    #[default]
    Idle,
    /// Sent as `request` and not yet confirmed by `ok`; `text` is what the
    /// server holds once it is (empty for the photo).
    Saving { request: RequestId, text: String },
    /// Why the last try failed.
    Failed(String),
}

impl BotSave {
    pub fn is_saving(&self) -> bool {
        matches!(self, Self::Saving { .. })
    }

    fn waits_for(&self, id: RequestId) -> bool {
        matches!(self, Self::Saving { request, .. } if *request == id)
    }
}

/// The texts of the bot being edited, as the server holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotEditTexts {
    pub bot_id: i64,
    pub name: BotText,
    pub about: BotText,
    pub name_save: BotSave,
    pub about_save: BotSave,
    pub photo_save: BotSave,
    /// Confirmed saves since the dialog opened.
    pub saved: u8,
}

/// Where the typed username's availability check stands. The answer does
/// not echo the username, so it rides in the stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum BotUsernameCheck {
    #[default]
    Idle,
    /// `checkBotUsername` for `username` is out as `generation`.
    Checking { generation: u64, username: String },
    /// The latest answered check.
    Known {
        username: String,
        result: UsernameCheckResult,
    },
}

/// What a pressed Create needs to report when the bot exists: fixed at the
/// press, so a closed box still reports with its own name and manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSubmission {
    /// Bumped per username check and per create, so late answers drop.
    pub generation: u64,
    pub name: String,
    pub username: String,
    pub manager_id: i64,
    /// The `keyboardButtonTypeRequestManagedBot` press to answer with the
    /// new bot, if the create came from one.
    pub share: Option<ManagedBotShare>,
}

/// The managed-bot button a create answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedBotShare {
    pub chat_id: crate::ids::ChatId,
    pub message_id: crate::ids::MessageId,
    pub button_id: i32,
}

/// Where the Create Bot dialog's `createBot` stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CreateStage {
    #[default]
    Editing,
    Creating(CreateSubmission),
    /// The new bot exists; the UI reports it once.
    Created {
        submission: CreateSubmission,
        user_id: i64,
    },
    /// The dialog shows the reason and takes another try.
    Failed(String),
}

/// The "Create Bot" dialog's server state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateBotState {
    /// Bumped per username check and per create, so late answers drop.
    pub generation: u64,
    pub check: BotUsernameCheck,
    pub stage: CreateStage,
}

impl CreateBotState {
    pub fn creating(&self) -> bool {
        matches!(self.stage, CreateStage::Creating(_))
    }

    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            CreateStage::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The verdict for `username`, if that is the one checked.
    pub fn verdict(&self, username: &str) -> Option<UsernameCheckResult> {
        match &self.check {
            BotUsernameCheck::Known {
                username: checked,
                result,
            } if checked == username => Some(*result),
            _ => None,
        }
    }

    pub fn checking(&self) -> bool {
        matches!(self.check, BotUsernameCheck::Checking { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnedBotsState {
    pub edit: Option<BotEditTexts>,
    pub create: CreateBotState,
}

impl BotEditTexts {
    /// A dialog for `bot_id` whose two loads are out as these requests.
    pub fn new(bot_id: i64, name: RequestId, about: RequestId) -> Self {
        Self {
            bot_id,
            name: BotText::Loading(name),
            about: BotText::Loading(about),
            name_save: BotSave::Idle,
            about_save: BotSave::Idle,
            photo_save: BotSave::Idle,
            saved: 0,
        }
    }

    /// Saves sent and not yet answered.
    pub fn saving(&self) -> usize {
        [&self.name_save, &self.about_save, &self.photo_save]
            .into_iter()
            .filter(|save| save.is_saving())
            .count()
    }

    /// A load failed.
    pub fn load_failed(&self) -> bool {
        self.name == BotText::Failed || self.about == BotText::Failed
    }

    /// The reason of a failed save, if one stands.
    pub fn error(&self) -> Option<&str> {
        [&self.name_save, &self.about_save, &self.photo_save]
            .into_iter()
            .find_map(|save| match save {
                BotSave::Failed(error) => Some(error.as_str()),
                _ => None,
            })
    }

    /// Failed saves are forgotten (a new try starts).
    pub fn forget_failed_saves(&mut self) {
        for save in [
            &mut self.name_save,
            &mut self.about_save,
            &mut self.photo_save,
        ] {
            if matches!(save, BotSave::Failed(_)) {
                *save = BotSave::Idle;
            }
        }
    }

    /// The save `pending` is the request of, if this dialog waits for it.
    fn save_of(
        &mut self,
        purpose: BotsPurpose,
        pending: Option<&PendingRequest>,
    ) -> Option<&mut BotSave> {
        let id = pending?.id;
        let save = match purpose {
            BotsPurpose::SetBotName { .. } => &mut self.name_save,
            BotsPurpose::SetBotAbout { .. } => &mut self.about_save,
            BotsPurpose::SetBotPhoto { .. } => &mut self.photo_save,
            _ => return None,
        };
        save.waits_for(id).then_some(save)
    }
}

impl Session {
    /// A `text` answer for the edit dialog's two loads.
    pub(crate) fn apply_owned_bot_text(
        &mut self,
        purpose: BotsPurpose,
        text: String,
        pending: Option<&PendingRequest>,
    ) {
        let BotsPurpose::GetBotEditText { about, .. } = purpose else {
            return;
        };
        let Some(id) = pending.map(|p| p.id) else {
            return;
        };
        if let Some(edit) = self.bots.owned.edit.as_mut() {
            let slot = if about {
                &mut edit.about
            } else {
                &mut edit.name
            };
            if *slot == BotText::Loading(id) {
                *slot = BotText::Ready(text);
            }
        }
    }

    /// `ok` for a save of the name, the about text or the photo.
    pub(crate) fn apply_owned_bots_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::Bots(purpose)) = pending.map(|p| p.purpose) else {
            return;
        };
        let Some(edit) = self.bots.owned.edit.as_mut() else {
            return;
        };
        let Some(save) = edit.save_of(purpose, pending) else {
            return;
        };
        let BotSave::Saving { text, .. } = std::mem::take(save) else {
            return;
        };
        match purpose {
            BotsPurpose::SetBotName { .. } => edit.name = BotText::Ready(text),
            BotsPurpose::SetBotAbout { .. } => edit.about = BotText::Ready(text),
            _ => {}
        }
        edit.saved = edit.saved.saturating_add(1);
    }

    /// `checkChatUsernameResult*` answering `checkBotUsername`.
    pub(crate) fn apply_bot_username_check(
        &mut self,
        result: UsernameCheckResult,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::Bots(BotsPurpose::CheckBotUsername { generation })) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        let create = &mut self.bots.owned.create;
        if let BotUsernameCheck::Checking {
            generation: asked,
            username,
        } = &create.check
            && *asked == generation
        {
            create.check = BotUsernameCheck::Known {
                username: username.clone(),
                result,
            };
        }
    }

    /// The `user` answer of `createBot`.
    pub(crate) fn apply_created_bot(&mut self, user_id: i64, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Bots(BotsPurpose::CreateBot { generation })) =
            pending.map(|p| p.purpose)
        {
            let create = &mut self.bots.owned.create;
            if let CreateStage::Creating(submission) = &create.stage
                && submission.generation == generation
            {
                create.stage = CreateStage::Created {
                    submission: submission.clone(),
                    user_id,
                };
            }
        }
    }

    /// A failed request of this feature; `true` when it was one.
    pub(crate) fn apply_owned_bots_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) -> bool {
        let Some(RequestPurpose::Bots(purpose)) = pending.map(|p| p.purpose) else {
            return false;
        };
        match purpose {
            BotsPurpose::GetBotEditText { about, .. } => {
                if let Some(edit) = self.bots.owned.edit.as_mut()
                    && let Some(id) = pending.map(|p| p.id)
                {
                    let slot = if about {
                        &mut edit.about
                    } else {
                        &mut edit.name
                    };
                    if *slot == BotText::Loading(id) {
                        *slot = BotText::Failed;
                    }
                }
            }
            BotsPurpose::SetBotName { .. }
            | BotsPurpose::SetBotAbout { .. }
            | BotsPurpose::SetBotPhoto { .. } => {
                let what = match purpose {
                    BotsPurpose::SetBotName { .. } => "the name",
                    BotsPurpose::SetBotAbout { .. } => "the about text",
                    _ => "the photo",
                };
                if let Some(edit) = self.bots.owned.edit.as_mut()
                    && let Some(save) = edit.save_of(purpose, pending)
                {
                    *save = BotSave::Failed(format!("Could not save {what} (error {}).", err.code));
                }
            }
            BotsPurpose::CheckBotUsername { generation } => {
                let create = &mut self.bots.owned.create;
                if let BotUsernameCheck::Checking {
                    generation: asked,
                    username,
                } = &create.check
                    && *asked == generation
                {
                    // tdesktop treats a failed check as "taken".
                    create.check = BotUsernameCheck::Known {
                        username: username.clone(),
                        result: UsernameCheckResult::Occupied,
                    };
                }
            }
            BotsPurpose::CreateBot { generation } => {
                let create = &mut self.bots.owned.create;
                if let CreateStage::Creating(submission) = &create.stage
                    && submission.generation == generation
                {
                    if err.premium_limit == Some(crate::premium_limits::PremiumLimitKind::OwnedBots)
                    {
                        create.stage = CreateStage::Editing;
                        self.open_premium_limit(
                            crate::premium_limits::PremiumLimitKind::OwnedBots,
                            0,
                        );
                        return true;
                    }
                    create.stage = CreateStage::Failed(err.flood_notice().unwrap_or_else(|| {
                        format!(
                            "Could not create the bot (error {}). {}",
                            err.code,
                            crate::owned_bots::CREATE_FAILED_HINT
                        )
                    }));
                }
            }
            _ => return false,
        }
        true
    }
}
