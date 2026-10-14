//! Connect driver: revenue screens and affiliate programs
//! (`crate::earn`). Withdrawals only fetch a Fragment URL; the 2FA
//! password goes straight into the request and is never kept.
use super::*;
use crate::earn::*;
use crate::ids::RequestId;
use crate::premium_hub::TxFilter;
use crate::state::{PaymentsPurpose, RequestPurpose};

fn purpose(purpose: EarnPurpose) -> RequestPurpose {
    RequestPurpose::Payments(PaymentsPurpose::Earn(purpose))
}

impl<S: JsonSender> ConnectDriver<S> {
    fn earn_ready(&self) -> Result<(), ConnectSendError> {
        if self.chats_path_active() {
            Ok(())
        } else {
            Err(ConnectSendError::InvalidRequest)
        }
    }

    fn send_earn(
        &mut self,
        earn: EarnPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.session.request(purpose(earn), None);
        let json = build(extra);
        self.send_json_request(extra, &json)
    }

    fn earn_in_flight(&self, earn: EarnPurpose) -> bool {
        self.session.requests.has_purpose(purpose(earn))
    }

    /// `getApplicationConfig` once, for the affiliate switches.
    pub fn ensure_earn_app_config(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        if self.session.payments.earn.limits.app_config_loaded
            || self.earn_in_flight(EarnPurpose::AppConfig)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetBoostsPerGift)
        {
            return Ok(());
        }
        self.send_earn(
            EarnPurpose::AppConfig,
            crate::telegram::requests::get_application_config,
        )
        .map(|_| ())
    }

    /// Load what an info-panel page shows.
    pub fn open_earn_panel(
        &mut self,
        panel: EarnPanel,
        is_dark: bool,
    ) -> Result<(), ConnectSendError> {
        match panel {
            EarnPanel::Revenue { peer, screen } => self.open_earn_page(peer, screen, is_dark),
            EarnPanel::Setup { bot_user_id } => self.open_starref_setup(bot_user_id),
            EarnPanel::Join { peer } => self.open_starref_join(peer),
        }
    }

    /// Whether the user has Stars revenue statistics (tdesktop asks
    /// `getStarRevenueStatistics` with the Stars balance to show "View
    /// Statistics").
    pub fn probe_self_star_stats(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let Some(me) = self.session.my_user_id else {
            return Ok(());
        };
        let peer = EarnPeer::User(me);
        if self.session.payments.earn.self_stats.is_some()
            || self.earn_in_flight(EarnPurpose::StarStats { peer })
        {
            return Ok(());
        }
        self.send_earn(EarnPurpose::StarStats { peer }, |extra| {
            get_star_revenue_statistics(extra, peer, false)
        })
        .map(|_| ())
    }

    /// Open a revenue page and load it the way tdesktop does: TON
    /// statistics and history, Stars statistics and history, and the ads
    /// account link.
    pub fn open_earn_page(
        &mut self,
        peer: EarnPeer,
        screen: EarnScreen,
        is_dark: bool,
    ) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let me = self.session.my_user_id;
        let is_self = matches!(peer, EarnPeer::User(id) if Some(id) == me);
        let is_bot = matches!(peer, EarnPeer::User(_)) && !is_self;
        let mut page = EarnPage::new(peer, screen);
        let want_ton = screen == EarnScreen::Monetization;
        // tdesktop's channel page reads the Stars side too; a bot's
        // Stars live on its own "Stars Balance" page.
        let want_stars = screen == EarnScreen::Stars || !is_bot;
        if want_ton {
            page.ton = Load::Loading;
            page.ton_history.loading = true;
        }
        if want_stars {
            page.stars = Load::Loading;
        }
        self.session.payments.earn.page = Some(page);
        let _ = self.ensure_earn_app_config();
        let chat_id = peer.chat_id();
        if want_ton {
            self.send_earn(EarnPurpose::TonStats { peer }, |extra| {
                get_chat_revenue_statistics(extra, chat_id, is_dark)
            })?;
            self.send_earn(
                EarnPurpose::TonTransactions {
                    peer,
                    append: false,
                },
                |extra| get_chat_revenue_transactions(extra, chat_id, "", TON_FIRST_SLICE),
            )?;
        }
        if want_stars {
            self.send_earn(EarnPurpose::StarStats { peer }, |extra| {
                get_star_revenue_statistics(extra, peer, is_dark)
            })?;
            let filters: &[TxFilter] = match screen {
                // The channel page has one Stars list.
                EarnScreen::Monetization => &[TxFilter::All],
                // tdesktop shows no history on the user's own page.
                EarnScreen::Stars if is_self => &[],
                EarnScreen::Stars => &TxFilter::ALL,
            };
            for &filter in filters {
                self.fetch_earn_star_history(peer, filter, false)?;
            }
            if !is_self {
                self.send_earn(EarnPurpose::AdAccountUrl { peer }, |extra| {
                    get_star_ad_account_url(extra, peer)
                })?;
            }
        }
        Ok(())
    }

    fn fetch_earn_star_history(
        &mut self,
        peer: EarnPeer,
        filter: TxFilter,
        append: bool,
    ) -> Result<(), ConnectSendError> {
        let Some(page) = self
            .session
            .payments
            .earn
            .page
            .as_mut()
            .filter(|p| p.peer == peer)
        else {
            return Ok(());
        };
        let history = page.history_mut(filter);
        if history.loading && append {
            return Ok(());
        }
        let offset = if append {
            history.next_offset.clone()
        } else {
            String::new()
        };
        history.loading = true;
        self.send_earn(
            EarnPurpose::StarTransactions {
                peer,
                filter,
                append,
            },
            |extra| get_star_transactions(extra, peer, filter, &offset, STAR_TX_LIMIT),
        )
        .map(|_| ())
    }

    /// "Show more" under the TON history.
    pub fn more_earn_ton_history(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let Some(page) = self.session.payments.earn.page.as_mut() else {
            return Ok(());
        };
        let peer = page.peer;
        let history = &mut page.ton_history;
        if history.loading || history.next_offset.is_empty() {
            return Ok(());
        }
        history.loading = true;
        let offset = history.next_offset.clone();
        self.send_earn(
            EarnPurpose::TonTransactions { peer, append: true },
            |extra| get_chat_revenue_transactions(extra, peer.chat_id(), &offset, TON_MORE_SLICE),
        )
        .map(|_| ())
    }

    /// The next page of a Stars history list.
    pub fn more_earn_star_history(&mut self, filter: TxFilter) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let Some(page) = self.session.payments.earn.page.as_ref() else {
            return Ok(());
        };
        if page.history(filter).next_offset.is_empty() {
            return Ok(());
        }
        let peer = page.peer;
        self.fetch_earn_star_history(peer, filter, true)
    }

    /// The balances behind a profile or bot edit row, once per peer
    /// (tdesktop loads them when the profile opens).
    pub fn fetch_earn_balances(
        &mut self,
        peer: EarnPeer,
        ton: bool,
        stars: bool,
    ) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        if self.session.payments.earn.balances_asked.contains(&peer) {
            return Ok(());
        }
        self.session.payments.earn.balances_asked.push(peer);
        if ton {
            self.send_earn(EarnPurpose::TonBalance { peer }, |extra| {
                get_chat_revenue_statistics(extra, peer.chat_id(), false)
            })?;
        }
        if stars {
            self.send_earn(EarnPurpose::StarBalance { peer }, |extra| {
                get_star_transactions(extra, peer, TxFilter::All, "", 1)
            })?;
        }
        Ok(())
    }

    /// Collect or Withdraw: the first try goes without a password
    /// (tdesktop `HandleWithdrawalButton`); the answer decides between the
    /// password box and the security check.
    pub fn start_withdrawal(&mut self, target: WithdrawTarget) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        if self.session.payments.earn.withdraw.is_some() {
            return Ok(());
        }
        if let WithdrawTarget::Stars { count, .. } = target
            && count <= 0
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.payments.earn.withdraw = Some(Withdraw {
            target,
            step: WithdrawStep::Checking,
        });
        let result = self.send_withdrawal(target, "");
        if result.is_err() {
            self.session.payments.earn.withdraw = None;
        }
        result
    }

    /// Submit the password box. The password is used for this one request
    /// and dropped by the caller.
    pub fn submit_withdrawal_password(&mut self, password: &str) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let Some(withdraw) = self.session.payments.earn.withdraw.as_mut() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if password.is_empty()
            || matches!(withdraw.step, WithdrawStep::Password { sending: true, .. })
        {
            return Ok(());
        }
        withdraw.step = WithdrawStep::Password {
            sending: true,
            error: None,
        };
        let target = withdraw.target;
        let result = self.send_withdrawal(target, password);
        if result.is_err()
            && let Some(withdraw) = self.session.payments.earn.withdraw.as_mut()
        {
            withdraw.step = WithdrawStep::Password {
                sending: false,
                error: None,
            };
        }
        result
    }

    fn send_withdrawal(
        &mut self,
        target: WithdrawTarget,
        password: &str,
    ) -> Result<(), ConnectSendError> {
        let earn = EarnPurpose::WithdrawalUrl {
            with_password: !password.is_empty(),
        };
        match target {
            WithdrawTarget::Ton { chat_id } => self.send_earn(earn, |extra| {
                get_chat_revenue_withdrawal_url(extra, chat_id, password)
            }),
            WithdrawTarget::Stars { owner, count } => self.send_earn(earn, |extra| {
                get_star_withdrawal_url(extra, owner, count, password)
            }),
        }
        .map(|_| ())
    }

    pub fn cancel_withdrawal(&mut self) {
        self.session.payments.earn.withdraw = None;
    }

    // ---- Affiliate programs -------------------------------------------

    /// Open the setup page of an own bot (tdesktop `Setup::Make`).
    pub fn open_starref_setup(&mut self, bot_user_id: i64) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let program = self
            .session
            .bots
            .bot_info
            .get(&bot_user_id)
            .and_then(|info| info.as_ref()?.affiliate_program);
        let earn = &mut self.session.payments.earn;
        earn.setup = Some(SetupPage::new(bot_user_id, program));
        earn.setup_closed = false;
        let _ = self.ensure_earn_app_config();
        Ok(())
    }

    /// Start or update the program with the sliders' values, after the
    /// confirmation box.
    pub fn save_starref_program(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let Some(setup) = self.session.payments.earn.setup.as_mut() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if setup.saving {
            return Ok(());
        }
        // tdesktop keeps `ConfirmUpdate` open until the answer.
        setup.saving = true;
        let (bot_user_id, params) = (setup.bot_user_id, setup.params);
        let result = self.send_earn(
            EarnPurpose::SetProgram {
                bot_user_id,
                end: false,
            },
            |extra| set_chat_affiliate_program(extra, bot_user_id, Some(params)),
        );
        self.unlock_setup_on_error(&result);
        result.map(|_| ())
    }

    /// End the program, after `ConfirmEndBox`.
    pub fn end_starref_program(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let Some(setup) = self.session.payments.earn.setup.as_mut() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if setup.saving {
            return Ok(());
        }
        setup.saving = true;
        setup.confirm = None;
        let bot_user_id = setup.bot_user_id;
        let result = self.send_earn(
            EarnPurpose::SetProgram {
                bot_user_id,
                end: true,
            },
            |extra| set_chat_affiliate_program(extra, bot_user_id, None),
        );
        self.unlock_setup_on_error(&result);
        result.map(|_| ())
    }

    fn unlock_setup_on_error<T>(&mut self, result: &Result<T, ConnectSendError>) {
        if result.is_err()
            && let Some(setup) = self.session.payments.earn.setup.as_mut()
        {
            setup.saving = false;
        }
    }

    /// Open the join page of `peer` (tdesktop `Join::Make`) and load both
    /// lists.
    pub fn open_starref_join(&mut self, peer: EarnPeer) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        self.session.payments.earn.join = Some(JoinPage::new(peer));
        let _ = self.ensure_earn_app_config();
        self.fetch_connected_programs(false)?;
        self.fetch_found_programs(false, false)
    }

    fn fetch_connected_programs(&mut self, append: bool) -> Result<(), ConnectSendError> {
        let me = self.session.my_user_id;
        let Some(join) = self.session.payments.earn.join.as_mut() else {
            return Ok(());
        };
        if join.mine.loading {
            return Ok(());
        }
        join.mine.loading = true;
        let offset = if append {
            join.mine.next_offset.clone()
        } else {
            String::new()
        };
        let affiliate = join.peer.affiliate_json(me);
        self.send_earn(EarnPurpose::ConnectedPrograms { append }, |extra| {
            get_connected_affiliate_programs(extra, affiliate, &offset, PROGRAMS_PER_PAGE)
        })
        .map(|_| ())
    }

    fn fetch_found_programs(
        &mut self,
        for_box: bool,
        append: bool,
    ) -> Result<(), ConnectSendError> {
        let me = self.session.my_user_id;
        let earn = &mut self.session.payments.earn;
        let (peer, sort) = if for_box {
            match earn.setup.as_ref() {
                Some(setup) => (EarnPeer::User(setup.bot_user_id), ProgramSort::default()),
                None => return Ok(()),
            }
        } else {
            match earn.join.as_ref() {
                Some(join) => (join.peer, join.sort),
                None => return Ok(()),
            }
        };
        let list = if for_box {
            earn.programs_box.as_mut()
        } else {
            earn.join.as_mut().map(|j| &mut j.found)
        };
        let Some(list) = list else {
            return Ok(());
        };
        if list.loading && append {
            return Ok(());
        }
        list.loading = true;
        let offset = if append {
            list.next_offset.clone()
        } else {
            String::new()
        };
        let affiliate = peer.affiliate_json(me);
        self.send_earn(EarnPurpose::SearchPrograms { for_box, append }, |extra| {
            search_affiliate_programs(extra, affiliate, sort, &offset, PROGRAMS_PER_PAGE)
        })
        .map(|_| ())
    }

    /// The "Sort by" menu reloads the programs list.
    pub fn set_starref_sort(&mut self, sort: ProgramSort) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let Some(join) = self.session.payments.earn.join.as_mut() else {
            return Ok(());
        };
        if join.sort == sort {
            return Ok(());
        }
        join.sort = sort;
        join.found = ProgramList::default();
        self.fetch_found_programs(false, false)
    }

    /// Scrolling to the end of a list loads its next page.
    pub fn more_starref_programs(
        &mut self,
        mine: bool,
        for_box: bool,
    ) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let earn = &self.session.payments.earn;
        let next = if mine {
            earn.join.as_ref().map(|j| j.mine.next_offset.clone())
        } else if for_box {
            earn.programs_box.as_ref().map(|b| b.next_offset.clone())
        } else {
            earn.join.as_ref().map(|j| j.found.next_offset.clone())
        };
        if next.is_none_or(|offset| offset.is_empty()) {
            return Ok(());
        }
        if mine {
            self.fetch_connected_programs(true)
        } else {
            self.fetch_found_programs(for_box, true)
        }
    }

    /// "View existing programs" of the setup page.
    pub fn open_starref_programs_box(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        self.session.payments.earn.programs_box = Some(ProgramList::default());
        self.fetch_found_programs(true, false)
    }

    /// A click on a program: resolve whether it is joined (tdesktop
    /// `rowClicked` → `Resolve`), asking for the recipients first when the
    /// user will pick one.
    pub fn open_affiliate_program(
        &mut self,
        bot_user_id: i64,
        for_box: bool,
    ) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let me = self.session.my_user_id;
        let earn = &self.session.payments.earn;
        let peer = if for_box {
            earn.setup.as_ref().map(|s| EarnPeer::User(s.bot_user_id))
        } else {
            earn.join.as_ref().map(|j| j.peer)
        };
        let Some(peer) = peer else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let choose_recipient = for_box || matches!(peer, EarnPeer::User(id) if Some(id) == me);
        if choose_recipient {
            self.request_earn_recipients()?;
        }
        let affiliate = peer.affiliate_json(me);
        self.send_earn(
            EarnPurpose::ConnectedProgram {
                bot_user_id,
                recipient: peer,
                choose_recipient,
            },
            |extra| get_connected_affiliate_program(extra, affiliate, bot_user_id),
        )
        .map(|_| ())
    }

    /// The bot profile's "Affiliate Program" row: the join box straight
    /// away, with the user's recipients (tdesktop `addAffiliateProgram`).
    pub fn open_bot_affiliate_join(&mut self, bot_user_id: i64) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let me = self
            .session
            .my_user_id
            .ok_or(ConnectSendError::InvalidRequest)?;
        let Some(info) = self
            .session
            .bots
            .bot_info
            .get(&bot_user_id)
            .and_then(|info| info.as_ref()?.affiliate_program)
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        self.request_earn_recipients()?;
        let join = JoinBox {
            bot_user_id,
            info,
            recipient: EarnPeer::User(me),
            choose_recipient: true,
            joining: false,
        };
        let earn = &mut self.session.payments.earn;
        if earn.recipients_loading > 0 {
            earn.pending_join = Some(join);
        } else {
            earn.join_box = Some(join);
        }
        Ok(())
    }

    /// The recipients: the user, owned public channels and owned bots.
    fn request_earn_recipients(&mut self) -> Result<(), ConnectSendError> {
        let earn = &self.session.payments.earn;
        if earn.recipients.is_some() || earn.recipients_loading > 0 {
            return Ok(());
        }
        self.session.payments.earn.recipients_loading = 2;
        let chats = self.send_earn(EarnPurpose::RecipientChats, get_created_public_chats);
        let bots = self.send_earn(EarnPurpose::RecipientBots, get_owned_bots);
        if chats.is_err() || bots.is_err() {
            let earn = &mut self.session.payments.earn;
            earn.recipients_loading = 0;
            earn.recipients = None;
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(())
    }

    /// Pick who the commissions of the open join box go to.
    pub fn set_join_recipient(&mut self, recipient: EarnPeer) {
        if let Some(join) = self.session.payments.earn.join_box.as_mut() {
            join.recipient = recipient;
        }
    }

    /// "Join Program".
    pub fn join_affiliate_program(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let me = self.session.my_user_id;
        let Some(join) = self.session.payments.earn.join_box.as_mut() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if join.joining {
            return Ok(());
        }
        join.joining = true;
        let (bot_user_id, recipient) = (join.bot_user_id, join.recipient);
        let affiliate = recipient.affiliate_json(me);
        let result = self.send_earn(EarnPurpose::Connect { recipient }, |extra| {
            connect_affiliate_program(extra, affiliate, bot_user_id)
        });
        if result.is_err()
            && let Some(join) = self.session.payments.earn.join_box.as_mut()
        {
            join.joining = false;
        }
        result.map(|_| ())
    }

    /// "Revoke Link" confirmed.
    pub fn revoke_affiliate_link(&mut self) -> Result<(), ConnectSendError> {
        self.earn_ready()?;
        let me = self.session.my_user_id;
        let Some(revoke) = self.session.payments.earn.revoke.as_mut() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if revoke.sending {
            return Ok(());
        }
        revoke.sending = true;
        let affiliate = revoke.recipient.affiliate_json(me);
        let url = revoke.program.url.clone();
        let result = self.send_earn(EarnPurpose::Disconnect, |extra| {
            disconnect_affiliate_program(extra, affiliate, &url)
        });
        if result.is_err()
            && let Some(revoke) = self.session.payments.earn.revoke.as_mut()
        {
            revoke.sending = false;
        }
        result.map(|_| ())
    }
}
