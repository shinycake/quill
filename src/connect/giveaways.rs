//! Connect driver: giveaways.
use super::*;
use crate::giveaway::GiveawayDraft;
use crate::ids::RequestId;
use crate::state::{GiveawayInfoLookup, GiveawayLaunch, RequestPurpose};
use crate::telegram::envelope::GiveawaySummary;
use crate::telegram::requests::{
    get_giveaway_info, get_premium_giveaway_payment_options, get_star_giveaway_payment_options,
    launch_prepaid_giveaway,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// `getGiveawayInfo` for the "About this giveaway" box. The answer
    /// lands in `payments.giveaway.info`; nothing else changes.
    pub fn request_giveaway_info(
        &mut self,
        chat_id: i64,
        message_id: i64,
        summary: GiveawaySummary,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || chat_id == 0 || message_id == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::GetGiveawayInfo,
            Some(crate::ids::ChatId(chat_id)),
        );
        self.session.payments.giveaway.info = Some(GiveawayInfoLookup {
            chat_id,
            message_id,
            summary,
            info: None,
            loading: true,
            error: None,
        });
        let result = self.send_json_request(extra, &get_giveaway_info(extra, chat_id, message_id));
        if result.is_err() {
            self.session.payments.giveaway.info = None;
        }
        result
    }

    /// Asks for the Premium and Stars prices of a new giveaway. Nothing is
    /// bought.
    pub fn request_giveaway_options(
        &mut self,
        boosted_chat_id: i64,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || boosted_chat_id == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let options = &mut self.session.payments.giveaway.options;
        options.premium_loading = true;
        options.stars_loading = true;
        options.premium_error = None;
        options.stars_error = None;
        let extra = self.session.request(
            RequestPurpose::GetPremiumGiveawayOptions,
            Some(crate::ids::ChatId(boosted_chat_id)),
        );
        let premium = self.send_json_request(
            extra,
            &get_premium_giveaway_payment_options(extra, boosted_chat_id),
        );
        if premium.is_err() {
            self.session.payments.giveaway.options.premium_loading = false;
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStarGiveawayOptions, None);
        let stars = self.send_json_request(extra, &get_star_giveaway_payment_options(extra));
        if stars.is_err() {
            self.session.payments.giveaway.options.stars_loading = false;
        }
        premium.and(stars).map(|_| ())
    }

    /// `launchPrepaidGiveaway` for a giveaway that is already paid for.
    /// Only the confirmation button calls this. A draft that is not
    /// prepaid or does not pass its checks sends nothing.
    pub fn launch_prepaid_giveaway(
        &mut self,
        draft: &GiveawayDraft,
        now: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some((giveaway_id, winner_count, star_count)) = draft.prepaid_launch() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if draft.validate(now).is_err()
            || matches!(
                self.session.payments.giveaway.launch,
                GiveawayLaunch::Launching { .. }
            )
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let chat_id = draft.boosted_chat_id;
        let extra = self.session.request(
            RequestPurpose::LaunchPrepaidGiveaway,
            Some(crate::ids::ChatId(chat_id)),
        );
        self.session.payments.giveaway.launch = GiveawayLaunch::Launching { chat_id };
        let json = launch_prepaid_giveaway(
            extra,
            giveaway_id,
            draft.parameters_json(),
            winner_count,
            star_count,
        );
        let result = self.send_json_request(extra, &json);
        if result.is_err() {
            self.session.payments.giveaway.launch = GiveawayLaunch::Idle;
        }
        result
    }
}
