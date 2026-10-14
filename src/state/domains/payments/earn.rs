//! Reducer for the revenue screens and affiliate programs
//! (`crate::earn`). Answers only land when they match the open page; the
//! withdrawal flow follows tdesktop `HandleWithdrawalButton`.
use crate::earn::*;
use crate::premium_hub::StarTxPage;
use crate::state::*;

fn earn_purpose(pending: Option<&PendingRequest>) -> Option<EarnPurpose> {
    match pending.map(|p| p.purpose) {
        Some(RequestPurpose::Payments(PaymentsPurpose::Earn(purpose))) => Some(purpose),
        _ => None,
    }
}

impl Session {
    pub(crate) fn is_earn_request(&self, pending: Option<&PendingRequest>) -> bool {
        earn_purpose(pending).is_some()
    }

    fn earn_page_mut(&mut self, peer: EarnPeer) -> Option<&mut EarnPage> {
        self.payments
            .earn
            .page
            .as_mut()
            .filter(|page| page.peer == peer)
    }

    /// A `starTransactions` answer for a revenue page or a balance row.
    pub(crate) fn apply_earn_star_transactions(
        &mut self,
        page: StarTxPage,
        pending: Option<&PendingRequest>,
    ) {
        match earn_purpose(pending) {
            Some(EarnPurpose::StarTransactions {
                peer,
                filter,
                append,
            }) => {
                self.payments.earn.star_balances.insert(peer, page.balance);
                if let Some(open) = self.earn_page_mut(peer) {
                    open.history_mut(filter).apply(page, append);
                }
            }
            Some(EarnPurpose::StarBalance { peer }) => {
                self.payments.earn.star_balances.insert(peer, page.balance);
            }
            _ => {}
        }
    }

    pub(crate) fn apply_earn(&mut self, payload: EarnPayload, pending: Option<&PendingRequest>) {
        let purpose = earn_purpose(pending);
        let me = self.my_user_id;
        let earn = &mut self.payments.earn;
        match (payload, purpose) {
            (EarnPayload::UpdateTonAmounts { chat_id, amounts }, _) => {
                for peer in [EarnPeer::Chat(chat_id), EarnPeer::User(chat_id)] {
                    if earn.ton_balances.contains_key(&peer)
                        || earn.page.as_ref().is_some_and(|p| p.peer == peer)
                    {
                        earn.ton_balances.insert(peer, amounts);
                    }
                    if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer)
                        && let Load::Ready(stats) = &mut page.ton
                    {
                        stats.amounts = amounts;
                    }
                }
            }
            (EarnPayload::UpdateStarStatus { owner, status }, _) => {
                let status = status.stamped(crate::local_time::now_unix());
                earn.star_balances.insert(owner, status.current);
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == owner)
                    && let Load::Ready(stats) = &mut page.stars
                {
                    stats.status = status;
                }
            }
            (EarnPayload::TonStats(stats), Some(EarnPurpose::TonStats { peer })) => {
                earn.ton_balances.insert(peer, stats.amounts);
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    page.ton = Load::Ready(stats);
                }
            }
            (EarnPayload::TonStats(stats), Some(EarnPurpose::TonBalance { peer })) => {
                earn.ton_balances.insert(peer, stats.amounts);
            }
            (
                EarnPayload::TonTransactions(page_data),
                Some(EarnPurpose::TonTransactions { peer, append }),
            ) => {
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    let history = &mut page.ton_history;
                    if !append {
                        history.rows.clear();
                    }
                    history.rows.extend(page_data.transactions);
                    history.next_offset = page_data.next_offset;
                    history.loading = false;
                    history.loaded = true;
                }
            }
            (EarnPayload::StarStats(mut stats), Some(EarnPurpose::StarStats { peer })) => {
                stats.status = stats.status.stamped(crate::local_time::now_unix());
                earn.star_balances.insert(peer, stats.status.current);
                if me.is_some_and(|me| peer == EarnPeer::User(me)) {
                    earn.self_stats = Some(true);
                }
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    page.stars = Load::Ready(stats);
                }
            }
            (
                EarnPayload::FoundPrograms(found),
                Some(EarnPurpose::SearchPrograms { for_box, append }),
            ) => {
                let list = if for_box {
                    earn.programs_box.as_mut()
                } else {
                    earn.join.as_mut().map(|join| &mut join.found)
                };
                if let Some(list) = list {
                    if !append {
                        list.programs.clear();
                    }
                    for program in found.programs {
                        if !list
                            .programs
                            .iter()
                            .any(|p| p.bot_user_id == program.bot_user_id)
                        {
                            list.programs.push(program);
                        }
                    }
                    list.total_count = found.total_count;
                    list.next_offset = found.next_offset;
                    list.loading = false;
                    list.loaded = true;
                }
            }
            (
                EarnPayload::ConnectedList(found),
                Some(EarnPurpose::ConnectedPrograms { append }),
            ) => {
                if let Some(list) = earn.join.as_mut().map(|join| &mut join.mine) {
                    if !append {
                        list.programs.clear();
                    }
                    for program in found.programs {
                        if !list.programs.iter().any(|p| p.url == program.url) {
                            list.programs.push(program);
                        }
                    }
                    list.total_count = found.total_count;
                    list.next_offset = found.next_offset;
                    list.loading = false;
                    list.loaded = true;
                }
            }
            (
                EarnPayload::Connected(program),
                Some(EarnPurpose::ConnectedProgram {
                    bot_user_id,
                    recipient,
                    choose_recipient,
                }),
            ) => {
                // tdesktop `ListController::open`: a live link opens the
                // link box, anything else the join box.
                if program.bot_user_id == bot_user_id
                    && !program.url.is_empty()
                    && !program.is_disconnected
                {
                    earn.link_box = Some(LinkBox { program, recipient });
                    earn.pending_join = None;
                } else {
                    self.open_join_box_after_resolve(bot_user_id, recipient, choose_recipient);
                }
            }
            (EarnPayload::Connected(program), Some(EarnPurpose::Connect { recipient })) => {
                // Joined: the link box replaces the join box and the
                // program moves to "My Programs" when it was for this page.
                earn.join_box = None;
                if let Some(join) = earn.join.as_mut().filter(|j| j.peer == recipient) {
                    join.found
                        .programs
                        .retain(|p| p.bot_user_id != program.bot_user_id);
                    if !join.mine.programs.iter().any(|p| p.url == program.url) {
                        join.mine.programs.insert(0, program.clone());
                    }
                }
                earn.link_box = Some(LinkBox { program, recipient });
            }
            (EarnPayload::Connected(program), Some(EarnPurpose::Disconnect)) => {
                let revoked = earn.revoke.take();
                if let Some(join) = earn.join.as_mut() {
                    join.mine
                        .programs
                        .retain(|p| p.bot_user_id != program.bot_user_id);
                    if let Some(info) = revoked.as_ref() {
                        let found = FoundProgram {
                            bot_user_id: program.bot_user_id,
                            info: ProgramInfo {
                                params: info.program.params,
                                ..ProgramInfo::default()
                            },
                        };
                        if !join
                            .found
                            .programs
                            .iter()
                            .any(|p| p.bot_user_id == found.bot_user_id)
                        {
                            join.found.programs.insert(0, found);
                        }
                    }
                }
                earn.note("Link removed. It will no longer work.");
            }
            _ => {}
        }
    }

    /// After `getConnectedAffiliateProgram` said "not joined": open the
    /// join box with the program the list showed, or park it until the
    /// recipients arrive when the user picks one.
    fn open_join_box_after_resolve(
        &mut self,
        bot_user_id: i64,
        recipient: EarnPeer,
        choose_recipient: bool,
    ) {
        let earn = &mut self.payments.earn;
        let info = earn
            .join
            .as_ref()
            .and_then(|j| {
                j.found
                    .programs
                    .iter()
                    .find(|p| p.bot_user_id == bot_user_id)
            })
            .or_else(|| {
                earn.programs_box
                    .as_ref()
                    .and_then(|b| b.programs.iter().find(|p| p.bot_user_id == bot_user_id))
            })
            .map(|p| p.info);
        let Some(info) = info else {
            return;
        };
        let join = JoinBox {
            bot_user_id,
            info,
            recipient,
            choose_recipient,
            joining: false,
        };
        if choose_recipient && earn.recipients_loading > 0 {
            earn.pending_join = Some(join);
        } else {
            earn.join_box = Some(join);
        }
    }

    /// An `httpUrl` answer: a Fragment withdrawal page or the ads account.
    pub(crate) fn apply_earn_url(&mut self, url: String, pending: Option<&PendingRequest>) -> bool {
        let earn = &mut self.payments.earn;
        match earn_purpose(pending) {
            Some(EarnPurpose::WithdrawalUrl { .. }) => {
                earn.withdraw = None;
                if !url.is_empty() {
                    earn.open_url = Some(url);
                }
                true
            }
            Some(EarnPurpose::AdAccountUrl { peer }) => {
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    page.ad_account_url = Some(url).filter(|u| !u.is_empty());
                }
                true
            }
            _ => false,
        }
    }

    /// `ok` for `setChatAffiliateProgram`: tdesktop goes back from the
    /// setup page and toasts what happened.
    pub(crate) fn apply_earn_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(EarnPurpose::SetProgram { bot_user_id, end }) = earn_purpose(pending) else {
            return;
        };
        let earn = &mut self.payments.earn;
        let existed = earn
            .setup
            .as_ref()
            .filter(|s| s.bot_user_id == bot_user_id)
            .is_some_and(SetupPage::exists);
        earn.setup = None;
        earn.setup_closed = true;
        earn.note(if end {
            "Affiliate program ended. Participating affiliates have been notified. All referral links will be disabled in 24 hours."
        } else if existed {
            "Affiliate program updated. Any Telegram user, channel owner or mini app developer can join your program."
        } else {
            "Affiliate program started. Any Telegram user, channel owner or mini app developer can now join your program."
        });
    }

    /// The recipients' public channels (`getCreatedPublicChats`); `true`
    /// when the answer was ours.
    pub(crate) fn apply_earn_recipient_chats(
        &mut self,
        chat_ids: &[crate::ids::ChatId],
        pending: Option<&PendingRequest>,
    ) -> bool {
        if earn_purpose(pending) != Some(EarnPurpose::RecipientChats) {
            return false;
        }
        let channels: Vec<EarnPeer> = chat_ids
            .iter()
            .filter(|id| self.earn_can_post_in_channel(id.0))
            .map(|id| EarnPeer::Chat(id.0))
            .collect();
        self.add_earn_recipients(channels);
        true
    }

    /// The recipients' bots (`getOwnedBots`); `true` when ours.
    pub(crate) fn apply_earn_recipient_bots(
        &mut self,
        user_ids: &[i64],
        pending: Option<&PendingRequest>,
    ) -> bool {
        if earn_purpose(pending) != Some(EarnPurpose::RecipientBots) {
            return false;
        }
        self.add_earn_recipients(user_ids.iter().map(|id| EarnPeer::User(*id)).collect());
        true
    }

    /// tdesktop `BotStarRef::Join::Allowed`: the user, an own bot or a
    /// channel the user posts in, when the app config allows joining.
    pub fn earn_join_allowed(&self, peer: EarnPeer) -> bool {
        if !self.payments.earn.limits.starref_join_allowed {
            return false;
        }
        match peer {
            EarnPeer::User(id) if self.my_user_id == Some(id) => true,
            EarnPeer::User(id) => self.user(id).is_some_and(|u| u.is_bot && u.can_be_edited),
            EarnPeer::Chat(id) => self.earn_can_post_in_channel(id),
        }
    }

    /// tdesktop `BotStarRef::Setup::Allowed`: a bot, when the app config
    /// allows programs.
    pub fn earn_setup_allowed(&self, bot_user_id: i64) -> bool {
        self.payments.earn.limits.starref_setup_allowed
            && self.user(bot_user_id).is_some_and(|u| u.is_bot)
    }

    /// The signed-in user created the channel (tdesktop `amCreator`).
    pub fn earn_is_channel_creator(&self, chat_id: i64) -> bool {
        self.chats.get(&chat_id).is_some_and(|chat| {
            chat.my_member_status == Some(crate::telegram::envelope::ChannelMemberStatus::Creator)
        })
    }

    /// A broadcast channel the user may post in (tdesktop keeps only
    /// those from `getAdminedPublicChannels`).
    pub(crate) fn earn_can_post_in_channel(&self, chat_id: i64) -> bool {
        self.chats.get(&chat_id).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            ) && chat.channel_admin_can_post()
        })
    }

    fn add_earn_recipients(&mut self, more: Vec<EarnPeer>) {
        let me = self.my_user_id;
        let earn = &mut self.payments.earn;
        let list = earn.recipients.get_or_insert_with(Vec::new);
        if let Some(me) = me
            && !list.contains(&EarnPeer::User(me))
        {
            list.insert(0, EarnPeer::User(me));
        }
        for peer in more {
            if !list.contains(&peer) {
                list.push(peer);
            }
        }
        earn.recipients_loading = earn.recipients_loading.saturating_sub(1);
        if earn.recipients_loading == 0
            && let Some(join) = earn.pending_join.take()
        {
            earn.join_box = Some(join);
        }
    }

    /// One `updateOption` for the revenue limits.
    pub(crate) fn apply_earn_option(&mut self, name: &str, value: &OptionValue) {
        let (int, flag) = match value {
            OptionValue::Integer(v) => (Some(*v), None),
            OptionValue::Boolean(b) => (None, Some(*b)),
            _ => (None, None),
        };
        self.payments.earn.limits.apply_option(name, int, flag);
    }

    /// A failed revenue or affiliate request.
    pub(crate) fn apply_earn_error(&mut self, err: &TdError, purpose: EarnPurpose) {
        let reason = error_reason(err);
        let earn = &mut self.payments.earn;
        match purpose {
            EarnPurpose::TonStats { peer } => {
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    // tdesktop: a group without TON ads just leaves the
                    // TON part out; anything else is shown.
                    if matches!(peer, EarnPeer::Chat(_))
                        && err.earn_hint == Some(EarnErrorHint::BroadcastRequired)
                    {
                        page.ton_unavailable = true;
                        page.ton = Load::Idle;
                    } else {
                        page.ton = Load::Failed(format!("Couldn't load the revenue: {reason}"));
                    }
                }
            }
            EarnPurpose::StarStats { peer } => {
                if self.my_user_id.is_some_and(|me| peer == EarnPeer::User(me)) {
                    earn.self_stats = Some(false);
                }
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    page.stars = Load::Failed(format!("Couldn't load the revenue: {reason}"));
                }
            }
            EarnPurpose::TonTransactions { peer, .. } => {
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    page.ton_history.loading = false;
                    page.ton_history.loaded = true;
                }
            }
            EarnPurpose::StarTransactions { peer, filter, .. } => {
                if let Some(page) = earn.page.as_mut().filter(|p| p.peer == peer) {
                    let history = page.history_mut(filter);
                    history.loading = false;
                    history.loaded = true;
                }
            }
            // Settled either way, so the gates do not ask again each frame.
            EarnPurpose::AppConfig => earn.limits.app_config_loaded = true,
            EarnPurpose::TonBalance { .. }
            | EarnPurpose::StarBalance { .. }
            | EarnPurpose::AdAccountUrl { .. } => {}
            EarnPurpose::WithdrawalUrl { with_password } => {
                Self::apply_withdrawal_error(earn, err, with_password, &reason);
            }
            EarnPurpose::SetProgram { .. } => {
                if let Some(setup) = earn.setup.as_mut() {
                    setup.saving = false;
                }
                earn.note(format!("Couldn't save the affiliate program: {reason}"));
            }
            EarnPurpose::SearchPrograms { for_box, .. } => {
                let list = if for_box {
                    earn.programs_box.as_mut()
                } else {
                    earn.join.as_mut().map(|j| &mut j.found)
                };
                if let Some(list) = list {
                    list.loading = false;
                    list.loaded = true;
                }
            }
            EarnPurpose::ConnectedPrograms { .. } => {
                if let Some(join) = earn.join.as_mut() {
                    join.mine.loading = false;
                    join.mine.loaded = true;
                }
            }
            EarnPurpose::ConnectedProgram {
                bot_user_id,
                recipient,
                choose_recipient,
            } => {
                // tdesktop `Resolve` treats a failure as "not joined".
                self.open_join_box_after_resolve(bot_user_id, recipient, choose_recipient);
            }
            EarnPurpose::Connect { .. } => {
                if let Some(join) = earn.join_box.as_mut() {
                    join.joining = false;
                }
                earn.note(format!("Failed: {reason}"));
            }
            EarnPurpose::Disconnect => {
                if let Some(revoke) = earn.revoke.as_mut() {
                    revoke.sending = false;
                }
                earn.note(format!("Couldn't revoke the link: {reason}"));
            }
            EarnPurpose::RecipientChats | EarnPurpose::RecipientBots => {
                // tdesktop treats a failed list as empty.
                self.add_earn_recipients(Vec::new());
            }
        }
    }

    /// tdesktop `HandleWithdrawalButton`'s failure branches.
    fn apply_withdrawal_error(
        earn: &mut EarnState,
        err: &TdError,
        with_password: bool,
        reason: &str,
    ) {
        let Some(withdraw) = earn.withdraw.as_mut() else {
            return;
        };
        match err.earn_hint {
            Some(EarnErrorHint::AmountTooSmall) => {
                earn.withdraw = None;
                earn.note("The amount is too small to withdraw.");
            }
            Some(EarnErrorHint::PasswordMissing) => {
                withdraw.step = WithdrawStep::SecurityCheck { later: false };
            }
            Some(EarnErrorHint::TooFresh) => {
                withdraw.step = WithdrawStep::SecurityCheck { later: true };
            }
            Some(EarnErrorHint::PasswordInvalid) if with_password => {
                withdraw.step = WithdrawStep::Password {
                    sending: false,
                    error: Some("Wrong cloud password".into()),
                };
            }
            _ if !with_password => {
                // Any other answer to the password-less try opens the
                // password box (tdesktop `processOut`).
                withdraw.step = WithdrawStep::Password {
                    sending: false,
                    error: None,
                };
            }
            _ => {
                withdraw.step = WithdrawStep::Password {
                    sending: false,
                    error: None,
                };
                earn.note(format!("Couldn't withdraw: {reason}"));
            }
        }
    }
}
