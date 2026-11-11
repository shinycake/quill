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

type Strings = Vec<(String, PackValue)>;

/// Which of a switch's two string sets have arrived.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SwitchStrings {
    /// Neither the pack nor its base.
    #[default]
    Neither,
    Pack(Strings),
    Base(Strings),
    Both { pack: Strings, base: Strings },
}

impl SwitchStrings {
    fn with(self, base: bool, strings: Strings) -> Self {
        match (self, base) {
            (Self::Neither | Self::Pack(_), false) => Self::Pack(strings),
            (Self::Neither | Self::Base(_), true) => Self::Base(strings),
            (Self::Base(held) | Self::Both { base: held, .. }, false) => Self::Both {
                pack: strings,
                base: held,
            },
            (Self::Pack(held) | Self::Both { pack: held, .. }, true) => Self::Both {
                pack: held,
                base: strings,
            },
        }
    }
}

/// A pack whose strings are being fetched.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageSwitch {
    pub info: LanguagePackInfoData,
    pub generation: u32,
    /// The strings that arrived; a pack with no base pack starts with an
    /// empty base.
    pub strings: SwitchStrings,
    /// What the server changed in this pack while its strings were being
    /// fetched; applied once the switch is ready.
    pub late: LateChange,
}

impl LanguageSwitch {
    /// A switch to `info`; `no_base` says the pack has no base pack.
    pub fn new(info: LanguagePackInfoData, generation: u32, no_base: bool) -> Self {
        Self {
            info,
            generation,
            strings: if no_base {
                SwitchStrings::Base(Vec::new())
            } else {
                SwitchStrings::Neither
            },
            late: LateChange::Nothing,
        }
    }
}

/// Where the interface language switch stands. It leaves by the strings
/// arriving, an error, or a newer switch replacing it.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum LanguageStage {
    #[default]
    Idle,
    Fetching(LanguageSwitch),
}

/// What the language flow hands to the UI, in the order it happened. The UI
/// drains them all at once.
#[derive(Debug, Clone, PartialEq)]
pub enum LanguageEvent {
    /// Install these strings.
    Install(LanguageReady),
    /// Changed strings of the current pack.
    Patch(Strings),
    /// TDLib says every string changed: fetch the current pack again.
    Reload,
    /// Why the last switch failed.
    SwitchFailed(String),
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
    pub stage: LanguageStage,
    pub generation: u32,
    /// Queue for the UI: strings to install, changed strings, a reload
    /// request, a failure.
    pub events: Vec<LanguageEvent>,
}

impl LanguageState {
    /// The switch whose strings are being fetched.
    pub fn switching(&self) -> Option<&LanguageSwitch> {
        match &self.stage {
            LanguageStage::Fetching(switch) => Some(switch),
            LanguageStage::Idle => None,
        }
    }

    /// Queue strings to install in place of any not yet installed.
    pub fn queue_install(&mut self, ready: LanguageReady) {
        self.events
            .retain(|event| !matches!(event, LanguageEvent::Install(_)));
        self.events.push(LanguageEvent::Install(ready));
    }

    /// Queue changed strings of the current pack, joining a patch queued
    /// last.
    pub fn queue_patch(&mut self, strings: Strings) {
        if let Some(LanguageEvent::Patch(queued)) = self.events.last_mut() {
            queued.extend(strings);
        } else {
            self.events.push(LanguageEvent::Patch(strings));
        }
    }

    pub fn queue_reload(&mut self) {
        if !self.events.contains(&LanguageEvent::Reload) {
            self.events.push(LanguageEvent::Reload);
        }
    }

    /// Everything but a failure is dropped.
    pub fn drop_all_but_failures(&mut self) {
        self.events
            .retain(|event| matches!(event, LanguageEvent::SwitchFailed(_)));
    }

    /// A new switch starts without the last failure.
    pub fn forget_failure(&mut self) {
        self.events
            .retain(|event| !matches!(event, LanguageEvent::SwitchFailed(_)));
    }

    /// Strings are waiting to be installed.
    pub fn install_queued(&self) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event, LanguageEvent::Install(_)))
    }

    /// The strings waiting to be installed, taken once.
    pub fn take_install(&mut self) -> Option<LanguageReady> {
        let at = self
            .events
            .iter()
            .position(|event| matches!(event, LanguageEvent::Install(_)))?;
        match self.events.remove(at) {
            LanguageEvent::Install(ready) => Some(ready),
            _ => None,
        }
    }

    /// Strings of changes waiting for the UI.
    pub fn queued_patch(&self) -> Strings {
        self.events
            .iter()
            .filter_map(|event| match event {
                LanguageEvent::Patch(strings) => Some(strings.iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// A reload of the current pack is waiting for the UI.
    pub fn reload_queued(&self) -> bool {
        self.events.contains(&LanguageEvent::Reload)
    }

    /// The failure of the last switch, if the UI has not taken it.
    pub fn failure(&self) -> Option<&str> {
        self.events.iter().find_map(|event| match event {
            LanguageEvent::SwitchFailed(error) => Some(error.as_str()),
            _ => None,
        })
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
                let LanguageStage::Fetching(switch) = &mut state.stage else {
                    return;
                };
                if switch.generation != generation {
                    return;
                }
                switch.strings = std::mem::take(&mut switch.strings).with(base, strings);
                if matches!(switch.strings, SwitchStrings::Both { .. })
                    && let LanguageStage::Fetching(switch) = std::mem::take(&mut state.stage)
                    && let SwitchStrings::Both { pack, base } = switch.strings
                {
                    state.current = Some(switch.info.clone());
                    // Changes queued for the old pack do not apply to this one.
                    state.events.retain(|event| {
                        !matches!(event, LanguageEvent::Patch(_) | LanguageEvent::Reload)
                    });
                    state.queue_install(LanguageReady {
                        info: switch.info,
                        pack,
                        base,
                    });
                    match switch.late {
                        LateChange::Nothing => {}
                        LateChange::Patch(late) => state.queue_patch(late),
                        LateChange::Reload => state.queue_reload(),
                    }
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
                if let LanguageStage::Fetching(switch) = &mut state.stage {
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
                    state.queue_patch(strings);
                } else if language_pack_id == current.id
                    || language_pack_id == current.base_language_pack_id
                {
                    // Every string changed, or the base pack changed under
                    // the pack's own strings: fetch both again.
                    state.queue_reload();
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
                if state.switching().map(|s| s.generation) == Some(generation) {
                    state.stage = LanguageStage::Idle;
                    state.forget_failure();
                    state.events.push(LanguageEvent::SwitchFailed(error_reason(err)));
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
