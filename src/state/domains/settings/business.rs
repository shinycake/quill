//! Telegram Business settings of the own account (see
//! `crate::business_settings`): the time zone list and the saves in
//! flight. The saved values themselves come back in the own
//! `userFullInfo.business_info` (TDLib sends `updateUserFullInfo` after a
//! successful `setBusiness*`).
use crate::business_settings::{BusinessSettingKind, TimeZone, save_error_line};
use crate::state::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BusinessSettingsState {
    /// `getTimeZones`, sorted for the picker; `None` until it answers.
    pub time_zones: Option<Vec<TimeZone>>,
    /// `getTimeZones` was sent.
    pub time_zones_requested: bool,
    /// Saves sent and not answered yet.
    pub saving: Vec<BusinessSettingKind>,
    /// The last refused save, in words.
    pub error: Option<String>,
    /// Why the last own `getUserFullInfo` failed (or a lost request), for
    /// the Retry the Business screens show instead of "Loading…". Asking
    /// again clears it. Whether the info is in or on its way is never kept
    /// here: [`Session::business_own_info`] reads the cache and the request
    /// table.
    pub own_info_error: Option<String>,
}

impl Session {
    pub(crate) fn apply_time_zones(&mut self, zones: Vec<TimeZone>) {
        self.settings.business.time_zones = Some(zones);
    }

    /// `ok` of a `setBusiness*` request.
    pub(crate) fn apply_business_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Settings(SettingsPurpose::SaveBusinessSetting { kind })) =
            pending.map(|p| p.purpose)
        {
            self.settings.business.saving.retain(|k| *k != kind);
        }
    }

    /// The own full info the Business screens are built from (their saved
    /// values are in it): ready once it is cached, loading while a
    /// `getUserFullInfo` for it is in flight, failed with the last reason
    /// otherwise.
    pub fn business_own_info(&self) -> crate::state::fetch::Fetch<()> {
        use crate::state::fetch::Fetch;
        let Some(me) = self.my_user_id else {
            return Fetch::Idle;
        };
        if self.users_state.user_full_infos.contains_key(&me) {
            return Fetch::Ready(());
        }
        if self
            .requests
            .has_purpose_for_user(RequestPurpose::GetUserFullInfo, me)
        {
            return Fetch::Loading { stale: None };
        }
        match &self.settings.business.own_info_error {
            Some(error) => Fetch::Failed {
                error: error.clone(),
                stale: None,
            },
            None => Fetch::Idle,
        }
    }

    /// A failed `getUserFullInfo`: for the own user, the reason the
    /// Business screens show (a lost request settles here too).
    pub(crate) fn fail_own_full_info(&mut self, err: &TdError, pending: Option<&PendingRequest>) {
        let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::GetUserFullInfo) else {
            return;
        };
        if pending.user_id.is_some() && pending.user_id == self.my_user_id {
            self.settings.business.own_info_error = Some(error_reason(err));
        }
    }

    /// A failed business request; `true` when it was one.
    pub(crate) fn apply_business_error(
        &mut self,
        err: &TdError,
        purpose: Option<RequestPurpose>,
    ) -> bool {
        let state = &mut self.settings.business;
        match purpose {
            Some(RequestPurpose::Settings(SettingsPurpose::GetTimeZones)) => {
                state.time_zones_requested = false;
                state.error = Some(format!(
                    "Couldn't load the time zones: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SaveBusinessSetting { kind })) => {
                state.saving.retain(|k| *k != kind);
                state.error = Some(save_error_line(kind, &error_reason(err)));
            }
            _ => return false,
        }
        true
    }
}
