//! Telegram Business settings of the own account (see
//! `crate::business_settings`): the time zone list and the saves in
//! flight. The saved values themselves come back in the own
//! `userFullInfo.business_info` (TDLib sends `updateUserFullInfo` after a
//! successful `setBusiness*`).
use crate::business_settings::{BusinessSettingKind, TimeZone, save_error_line};
use crate::state::*;

/// The part of the Business screens a refusal belongs to: a screen draws
/// only the refusals of its own part (the hub draws all of them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessArea {
    Save(BusinessSettingKind),
    ChatLinks,
    Chatbot,
    Ads,
}

/// The last refused request of the Business screens, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessRefusal {
    pub area: BusinessArea,
    pub line: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BusinessSettingsState {
    /// `getTimeZones`, sorted for the picker: asked once per session, again
    /// only after it failed.
    pub time_zones: crate::state::fetch::Fetch<Vec<TimeZone>>,
    /// Saves sent and not answered yet, each under its request: only that
    /// request's answer or error ends it, so a second save of the same
    /// setting keeps the screen "Saving…" until it answers.
    pub saving: Vec<(RequestId, BusinessSettingKind)>,
    /// The last refused request, with the part it belongs to.
    pub error: Option<BusinessRefusal>,
    /// Why the last own `getUserFullInfo` failed (or a lost request), for
    /// the Retry the Business screens show instead of "Loading…". Asking
    /// again clears it. Whether the info is in or on its way is never kept
    /// here: [`Session::business_own_info`] reads the cache and the request
    /// table.
    pub own_info_error: Option<String>,
}

impl BusinessSettingsState {
    /// A save is out.
    pub fn is_saving(&self) -> bool {
        !self.saving.is_empty()
    }

    /// The refusal a screen draws: the last refusal of a part `shows`
    /// picks, else why the time zones failed to load.
    pub fn refusal(&self, shows: impl Fn(BusinessArea) -> bool) -> Option<&str> {
        self.error
            .as_ref()
            .filter(|refusal| shows(refusal.area))
            .map(|refusal| refusal.line.as_str())
            .or_else(|| self.time_zones.error())
    }

    /// Record a refusal of `area`.
    pub fn refuse(&mut self, area: BusinessArea, line: String) {
        self.error = Some(BusinessRefusal { area, line });
    }
}

impl Session {
    pub(crate) fn apply_time_zones(&mut self, zones: Vec<TimeZone>, request: Option<RequestId>) {
        self.settings.business.time_zones.answered(request, zones);
    }

    /// `ok` of a `setBusiness*` request.
    pub(crate) fn apply_business_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Settings(SettingsPurpose::SaveBusinessSetting { .. })) =
            pending.map(|p| p.purpose)
        {
            let id = pending.map(|p| p.id);
            self.settings
                .business
                .saving
                .retain(|(request, _)| Some(*request) != id);
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
            return Fetch::Loading {
                stale: None,
                request: None,
            };
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
        pending_id: Option<RequestId>,
    ) -> bool {
        let state = &mut self.settings.business;
        match purpose {
            Some(RequestPurpose::Settings(SettingsPurpose::GetTimeZones)) => {
                // The failure line lives in the fetch only; the screens read
                // it from there ([`BusinessSettingsState::refusal`]).
                state.time_zones.failed(
                    pending_id,
                    format!("Couldn't load the time zones: {}", error_reason(err)),
                    false,
                );
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SaveBusinessSetting { kind })) => {
                state
                    .saving
                    .retain(|(request, _)| Some(*request) != pending_id);
                state.refuse(
                    BusinessArea::Save(kind),
                    save_error_line(kind, &error_reason(err)),
                );
            }
            _ => return false,
        }
        true
    }
}
