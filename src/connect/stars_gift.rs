//! Connect driver: the Gift Stars dialog (`crate::stars_gift`) and the
//! gift recipient list.
use super::*;
use crate::ids::RequestId;
use crate::stars_gift::{StarsGiftTarget, can_gift_stars};
use crate::state::RequestPurpose;
use crate::telegram::requests::get_star_gift_payment_options;

impl<S: JsonSender> ConnectDriver<S> {
    /// Open the dialog on the contact list and load the contacts.
    pub fn open_stars_gift(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let state = &mut self.session.payments.stars_gift;
        state.clear_target();
        state.open = true;
        self.fetch_contacts()?;
        // The people you write to most are listed first, unless the
        // frequent contacts are switched off.
        let _ = self.fetch_top_chats();
        Ok(())
    }

    /// "Gift Premium or Stars": load the contacts and the frequent
    /// contacts the gift recipient list shows (tdesktop
    /// `ChooseStarGiftRecipient`).
    pub fn load_gift_recipients(&mut self) -> Result<(), ConnectSendError> {
        self.fetch_contacts()?;
        let _ = self.fetch_top_chats();
        Ok(())
    }

    pub fn close_stars_gift(&mut self) {
        let state = &mut self.session.payments.stars_gift;
        state.clear_target();
        state.open = false;
    }

    /// A contact was picked: ask which packs they can get
    /// (`getStarGiftPaymentOptions`). Yourself and bots are refused.
    pub fn choose_stars_gift_user(
        &mut self,
        user_id: i64,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let is_bot = self.session.is_bot_user(user_id);
        if !self.chats_path_active()
            || !self.session.payments.stars_gift.open
            || !can_gift_stars(user_id, self.session.my_user_id, is_bot)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStarGiftOptions, None);
        let state = &mut self.session.payments.stars_gift;
        state.clear_target();
        state.target = Some(StarsGiftTarget {
            user_id,
            name: name.to_string(),
        });
        state.loading = true;
        let json = get_star_gift_payment_options(extra, user_id);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.stars_gift.loading = false;
            return Err(err);
        }
        Ok(extra)
    }
}
