//! Settings → Language: the packs TDLib offers, a switch in flight and the
//! strings it produced (docs/decisions/language-packs.md). The UI installs
//! the strings (`lang::install`); the state only collects them.
use crate::lang::PackValue;
use crate::state::fetch::Fetch;
use crate::state::*;
use crate::telegram::envelope::{LanguagePackInfoData, LanguagePayload};

/// TDLib's name for Telegram Desktop's strings; Quill's `lng_*` keys are
/// those strings.
pub const LOCALIZATION_TARGET: &str = "tdesktop";

/// In-flight language requests; wrapped as `SettingsPurpose::Language`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguagePurpose {
    /// `setOption` of a language option. Response is `ok`.
    SetOption,
    /// `getLocalizationTargetInfo`. Response is `localizationTargetInfo`.
    TargetInfo,
    /// `getLanguagePackStrings` of the switch `generation`, for the pack
    /// itself or its base pack. Response is `languagePackStrings`.
    Strings { generation: u32, base: bool },
    /// `getLanguagePackStrings` of the announcement badge texts
    /// (`cloud_lng_badge_psa_<type>`) of one pack; `epoch` tells whether the
    /// pack is still the one asked about. Response is `languagePackStrings`.
    PsaLabels { epoch: u32 },
    /// `synchronizeLanguagePack` / `addCustomServerLanguagePack`.
    /// Response is `ok`.
    Synchronize,
}

/// A pack whose strings are being fetched.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageSwitch {
    pub info: LanguagePackInfoData,
    pub generation: u32,
    pub pack: Option<Vec<(String, PackValue)>>,
    /// `Some(vec![])` up front when the pack has no base pack.
    pub base: Option<Vec<(String, PackValue)>>,
    /// What the server changed in this pack while its strings were being
    /// fetched; applied once the switch is ready.
    pub late: LateChange,
}

/// Changes to the pack being switched to that arrived before its strings
/// did: the fetched snapshot may or may not include them.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum LateChange {
    #[default]
    Nothing,
    /// Changed strings to patch over the fetched ones.
    Patch(Vec<(String, PackValue)>),
    /// Deleted strings or a base-pack change: fetch both packs again.
    Reload,
}

impl LateChange {
    pub(crate) fn note(&mut self, own_pack: bool, strings: Vec<(String, PackValue)>) {
        let deletes = strings.iter().any(|(_, v)| matches!(v, PackValue::Deleted));
        if !own_pack || strings.is_empty() || deletes {
            *self = Self::Reload;
        } else if let Self::Patch(queued) = self {
            queued.extend(strings);
        } else if matches!(self, Self::Nothing) {
            *self = Self::Patch(strings);
        }
    }
}

/// The strings of a finished switch, for the UI to install.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageReady {
    pub info: LanguagePackInfoData,
    pub pack: Vec<(String, PackValue)>,
    pub base: Vec<(String, PackValue)>,
}

/// Settings → Language.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LanguageState {
    /// `localizationTargetInfo`: official packs and installed custom ones.
    pub packs: Fetch<Vec<LanguagePackInfoData>>,
    /// The pack the interface uses (the one TDLib's `language_pack_id`
    /// names); empty for the built-in English.
    pub current: Option<LanguagePackInfoData>,
    pub switching: Option<LanguageSwitch>,
    pub generation: u32,
    /// One-shot for the UI: how the last switch ended. A new switch clears
    /// it; the UI takes it (`drain_language`).
    pub outcome: SwitchOutcome,
    /// One-shot for the UI: what changed in the current pack since it was
    /// installed. Changed strings patch it; deleted ones, a base-pack change
    /// or an update with no strings mean fetch it again.
    pub refresh: LateChange,
}

/// How the last language switch ended.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SwitchOutcome {
    /// Nothing to take: no switch ended since the UI last looked.
    #[default]
    None,
    /// The strings to install.
    Ready(LanguageReady),
    /// Why the switch failed.
    Failed(String),
}

impl LanguageState {
    pub fn ready(&self) -> Option<&LanguageReady> {
        match &self.outcome {
            SwitchOutcome::Ready(ready) => Some(ready),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match &self.outcome {
            SwitchOutcome::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The strings to install, if that is how the last switch ended.
    pub fn take_ready(&mut self) -> Option<LanguageReady> {
        match std::mem::take(&mut self.outcome) {
            SwitchOutcome::Ready(ready) => Some(ready),
            other => {
                self.outcome = other;
                None
            }
        }
    }
}

impl Session {
    pub(crate) fn apply_language_payload(
        &mut self,
        payload: LanguagePayload,
        pending: Option<&PendingRequest>,
    ) {
        let state = &mut self.settings.language;
        match payload {
            LanguagePayload::TargetInfo(packs) => {
                state.packs.succeed(packs);
            }
            LanguagePayload::Strings(strings) => {
                if let Some(RequestPurpose::Settings(SettingsPurpose::Language(
                    LanguagePurpose::PsaLabels { epoch },
                ))) = pending.map(|p| p.purpose)
                {
                    self.chat_list
                        .psa_labels
                        .store(epoch, pending.map(|p| p.id), strings);
                    return;
                }
                let Some(RequestPurpose::Settings(SettingsPurpose::Language(
                    LanguagePurpose::Strings { generation, base },
                ))) = pending.map(|p| p.purpose)
                else {
                    return;
                };
                let Some(switch) = state.switching.as_mut() else {
                    return;
                };
                if switch.generation != generation {
                    return;
                }
                if base {
                    switch.base = Some(strings);
                } else {
                    switch.pack = Some(strings);
                }
                if switch.pack.is_some()
                    && switch.base.is_some()
                    && let Some(switch) = state.switching.take()
                {
                    state.current = Some(switch.info.clone());
                    // Changes queued for the old pack do not apply to this one.
                    state.refresh = switch.late;
                    state.outcome = SwitchOutcome::Ready(LanguageReady {
                        info: switch.info,
                        pack: switch.pack.unwrap_or_default(),
                        base: switch.base.unwrap_or_default(),
                    });
                }
            }
            LanguagePayload::UpdateStrings {
                localization_target,
                language_pack_id,
                strings,
            } => {
                if localization_target != LOCALIZATION_TARGET {
                    return;
                }
                // The pack being switched to is not `current` yet: keep what
                // changed in it for when its strings arrive.
                if let Some(switch) = state.switching.as_mut() {
                    let own = language_pack_id == switch.info.id;
                    if own || language_pack_id == switch.info.base_language_pack_id {
                        switch.late.note(own, strings.clone());
                    }
                }
                let Some(current) = state.current.as_ref() else {
                    return;
                };
                // A deleted string falls back to the base pack, which the
                // snapshot no longer keeps apart, and a base-pack change
                // moves strings under the pack's own: fetch both again.
                let own = language_pack_id == current.id;
                if own || language_pack_id == current.base_language_pack_id {
                    state.refresh.note(own, strings);
                }
            }
        }
    }

    pub(crate) fn apply_language_error(
        &mut self,
        err: &TdError,
        purpose: LanguagePurpose,
        request: Option<RequestId>,
    ) {
        let state = &mut self.settings.language;
        match purpose {
            LanguagePurpose::TargetInfo => state.packs.fail(error_reason(err)),
            LanguagePurpose::Strings { generation, .. } => {
                if state.switching.as_ref().map(|s| s.generation) == Some(generation) {
                    state.switching = None;
                    state.outcome = SwitchOutcome::Failed(error_reason(err));
                }
            }
            // The badge types it asked for are asked again by the next pump.
            LanguagePurpose::PsaLabels { .. } => {
                if let Some(request) = request {
                    self.chat_list.psa_labels.fail_request(request);
                }
            }
            LanguagePurpose::SetOption | LanguagePurpose::Synchronize => {}
        }
    }
}
