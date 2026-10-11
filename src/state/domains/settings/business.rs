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
