//! Where your own QR code's link stands, from the session's user, phone
//! privacy rules and contact link.
use crate::peer_qr::link::{OwnLinkInputs, OwnLinkState, own_link};
use crate::privacy::PrivacyKeyState;
use crate::state::*;
use crate::telegram::requests_privacy::PrivacySettingKey;

impl Session {
    /// tdesktop `OwnQrLinkState` for the signed-in user; `None` before
    /// the user is known. `phone_label` is the number as the profile
    /// shows it.
    pub fn own_qr_link(&self, phone_label: &str, now: i64) -> Option<OwnLinkState> {
        let user = self.user(self.my_user_id?)?;
        let who = |key| match self.settings.privacy.get(&key) {
            Some(PrivacyKeyState::Ready(detail)) => detail.who,
            _ => None,
        };
        Some(own_link(OwnLinkInputs {
            username: &user.username,
            phone: &user.phone_number,
            phone_label,
            show_phone: who(PrivacySettingKey::ShowPhoneNumber),
            find_by_phone: who(PrivacySettingKey::AllowFindingByPhoneNumber),
            contact: &self.users_state.peer_qr.contact_link,
            now,
        }))
    }
}
