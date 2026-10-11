//! Connect driver: Telegram Business settings of the own account
//! (`crate::business_settings`).
use super::*;
use crate::business_info::{BusinessLocation, OpeningHours};
use crate::business_settings::{self as bs, AwaySettings, BusinessSettingKind, GreetingSettings};
use crate::state::{RequestPurpose, SettingsPurpose};

impl<S: JsonSender> ConnectDriver<S> {
    fn send_business_request(
        &mut self,
        purpose: RequestPurpose,
        build: impl FnOnce(&str) -> String,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose, None);
        let json = build(&extra.as_extra());
        self.send_json_request(extra, &json).map(|_| ())
    }

    /// `getTimeZones`, once per session unless it failed.
    pub fn ensure_time_zones(&mut self) -> Result<(), ConnectSendError> {
        let state = &mut self.session.settings.business;
        if state.time_zones.is_some() || state.time_zones_requested {
            return Ok(());
        }
        state.time_zones_requested = true;
        let sent = self.send_business_request(
            RequestPurpose::Settings(SettingsPurpose::GetTimeZones),
            bs::get_time_zones_request,
        );
        if sent.is_err() {
            self.session.settings.business.time_zones_requested = false;
        }
        sent
    }

    fn save_business(
        &mut self,
        kind: BusinessSettingKind,
        build: impl FnOnce(&str) -> String,
    ) -> Result<(), ConnectSendError> {
        self.session.settings.business.error = None;
        self.send_business_request(
            RequestPurpose::Settings(SettingsPurpose::SaveBusinessSetting { kind }),
            build,
        )?;
        self.session.settings.business.saving.push(kind);
        Ok(())
    }

    /// `setBusinessGreetingMessageSettings`; `None` turns it off.
    pub fn save_business_greeting(
        &mut self,
        settings: Option<&GreetingSettings>,
    ) -> Result<(), ConnectSendError> {
        if settings.is_some_and(|s| !bs::INACTIVITY_DAYS.contains(&s.inactivity_days)) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.save_business(BusinessSettingKind::Greeting, |extra| {
            bs::set_greeting_request(extra, settings)
        })
    }

    /// `setBusinessAwayMessageSettings`; `None` turns it off.
    pub fn save_business_away(
        &mut self,
        settings: Option<&AwaySettings>,
    ) -> Result<(), ConnectSendError> {
        if let Some(bs::AwaySchedule::Custom { start, end }) = settings.map(|s| s.schedule)
            && bs::bad_custom_interval(start, end)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.save_business(BusinessSettingKind::Away, |extra| {
            bs::set_away_request(extra, settings)
        })
    }

    /// `setBusinessOpeningHours`; `None` removes them.
    pub fn save_business_hours(
        &mut self,
        hours: Option<&OpeningHours>,
    ) -> Result<(), ConnectSendError> {
        self.save_business(BusinessSettingKind::OpeningHours, |extra| {
            bs::set_opening_hours_request(extra, hours)
        })
    }

    /// `setBusinessLocation`; `None` removes it.
    pub fn save_business_location(
        &mut self,
        location: Option<&BusinessLocation>,
    ) -> Result<(), ConnectSendError> {
        self.save_business(BusinessSettingKind::Location, |extra| {
            bs::set_location_request(extra, location)
        })
    }
}
