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
    fn note(&mut self, own_pack: bool, strings: Vec<(String, PackValue)>) {
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
    /// One-shot for the UI: install these strings.
    pub ready: Option<LanguageReady>,
    /// One-shot for the UI: changed strings of the current pack.
    pub patch: Vec<(String, PackValue)>,
    /// One-shot for the UI: TDLib says every string changed; fetch the
    /// current pack again.
    pub reload: bool,
    /// One-shot for the UI: why the last switch failed.
    pub error: Option<String>,
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
                    state.patch.clear();
                    state.reload = false;
                    match switch.late {
                        LateChange::Nothing => {}
                        LateChange::Patch(late) => state.patch = late,
                        LateChange::Reload => state.reload = true,
                    }
                    state.ready = Some(LanguageReady {
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
                // snapshot no longer keeps apart: fetch both again.
                let deletes = strings.iter().any(|(_, v)| matches!(v, PackValue::Deleted));
                if language_pack_id == current.id && !strings.is_empty() && !deletes {
                    state.patch.extend(strings);
                } else if language_pack_id == current.id
                    || language_pack_id == current.base_language_pack_id
                {
                    // Every string changed, or the base pack changed under
                    // the pack's own strings: fetch both again.
                    state.reload = true;
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
                    state.error = Some(error_reason(err));
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
