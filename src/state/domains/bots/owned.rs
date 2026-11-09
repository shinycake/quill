//! Editing a bot the viewer owns and creating a managed one (see
//! `crate::owned_bots` for what tdesktop does).
use crate::state::*;

/// The texts of the bot being edited, as the server holds them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BotEditTexts {
    pub bot_id: i64,
    /// `None` until `getBotName` answers.
    pub name: Option<String>,
    /// `None` until `getBotInfoShortDescription` answers.
    pub about: Option<String>,
    /// A load failed; the dialog says so instead of waiting forever.
    pub load_failed: bool,
    /// Texts sent and not yet confirmed by `ok`.
    pub pending_name: Option<String>,
    pub pending_about: Option<String>,
    /// The two loads in flight; an answer for another dialog drops.
    pub(crate) loads: Vec<RequestId>,
    /// Saves in flight (name, about, photo); only these settle the dialog.
    pub(crate) saves: Vec<RequestId>,
    /// Confirmed saves since the dialog opened.
    pub saved: u8,
    pub error: Option<String>,
}

/// The "Create Bot" dialog's server state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateBotState {
    /// Bumped per username check and per create, so late answers drop.
    pub generation: u64,
    /// The username of the check in flight (the answer does not echo it).
    pub check_username: String,
    /// `(username, verdict)` of the latest answered `checkBotUsername`.
    pub check: Option<(String, UsernameCheckResult)>,
    /// Generation of the check in flight.
    pub check_pending: Option<u64>,
    pub creating: Option<u64>,
    /// The new bot's user id once `createBot` answered.
    pub created: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnedBotsState {
    pub edit: Option<BotEditTexts>,
    pub create: CreateBotState,
}

impl BotEditTexts {
    /// Saves sent and not yet answered.
    pub fn saving(&self) -> usize {
        self.saves.len()
    }

    /// `pending` is a request of this dialog in `list`: settle it.
    fn take(list: &mut Vec<RequestId>, pending: Option<&PendingRequest>) -> bool {
        let Some(id) = pending.map(|p| p.id) else {
            return false;
        };
        let before = list.len();
        list.retain(|sent| *sent != id);
        list.len() != before
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
        if let Some(edit) = self.bots.owned.edit.as_mut()
            && BotEditTexts::take(&mut edit.loads, pending)
        {
            if about {
                edit.about = Some(text);
            } else {
                edit.name = Some(text);
            }
        }
    }

    /// `ok` for a save of the name, the about text or the photo.
    pub(crate) fn apply_owned_bots_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::Bots(purpose)) = pending.map(|p| p.purpose) else {
            return;
        };
        let (BotsPurpose::SetBotName { .. }
        | BotsPurpose::SetBotAbout { .. }
        | BotsPurpose::SetBotPhoto { .. }) = purpose
        else {
            return;
        };
        let Some(edit) = self.bots.owned.edit.as_mut() else {
            return;
        };
        if !BotEditTexts::take(&mut edit.saves, pending) {
            return;
        }
        match purpose {
            BotsPurpose::SetBotName { .. } => {
                if let Some(name) = edit.pending_name.take() {
                    edit.name = Some(name);
                }
            }
            BotsPurpose::SetBotAbout { .. } => {
                if let Some(about) = edit.pending_about.take() {
                    edit.about = Some(about);
                }
            }
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
        if create.check_pending == Some(generation) {
            create.check_pending = None;
            create.check = Some((create.check_username.clone(), result));
        }
    }

    /// The `user` answer of `createBot`.
    pub(crate) fn apply_created_bot(&mut self, user_id: i64, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Bots(BotsPurpose::CreateBot { generation })) =
            pending.map(|p| p.purpose)
        {
            let create = &mut self.bots.owned.create;
            if create.creating == Some(generation) {
                create.creating = None;
                create.created = Some(user_id);
                create.error = None;
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
            BotsPurpose::GetBotEditText { .. } => {
                if let Some(edit) = self.bots.owned.edit.as_mut()
                    && BotEditTexts::take(&mut edit.loads, pending)
                {
                    edit.load_failed = true;
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
                    && BotEditTexts::take(&mut edit.saves, pending)
                {
                    match purpose {
                        BotsPurpose::SetBotName { .. } => edit.pending_name = None,
                        BotsPurpose::SetBotAbout { .. } => edit.pending_about = None,
                        _ => {}
                    }
                    edit.error = Some(format!("Could not save {what} (error {}).", err.code));
                }
            }
            BotsPurpose::CheckBotUsername { generation } => {
                let create = &mut self.bots.owned.create;
                if create.check_pending == Some(generation) {
                    create.check_pending = None;
                    // tdesktop treats a failed check as "taken".
                    create.check =
                        Some((create.check_username.clone(), UsernameCheckResult::Occupied));
                }
            }
            BotsPurpose::CreateBot { generation } => {
                let create = &mut self.bots.owned.create;
                if create.creating == Some(generation) {
                    create.creating = None;
                    if err.premium_limit == Some(crate::premium_limits::PremiumLimitKind::OwnedBots)
                    {
                        create.error = None;
                        self.open_premium_limit(
                            crate::premium_limits::PremiumLimitKind::OwnedBots,
                            0,
                        );
                        return true;
                    }
                    create.error = Some(err.flood_notice().unwrap_or_else(|| {
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
