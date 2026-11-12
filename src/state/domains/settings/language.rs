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
/// The switch in flight holds the `RequestId` of each request it waits for
/// ([`LanguageSwitch`]); the purposes only say what a request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguagePurpose {
    /// `setOption` of a startup language option, which no switch waits on.
    /// Response is `ok`.
    SetOption,
    /// `setOption` of the `language_pack_id` a switch selects. Response is
    /// `ok`.
    SwitchOption,
    /// `getLocalizationTargetInfo`. Response is `localizationTargetInfo`.
    TargetInfo,
    /// `getLanguagePackStrings` of a switch's pack or its base pack.
    /// Response is `languagePackStrings`.
    Strings,
    /// `getLanguagePackStrings` of the announcement badge texts
    /// (`cloud_lng_badge_psa_<type>`) of one pack; the label set waits for the
    /// request itself. Response is `languagePackStrings`.
    PsaLabels,
    /// `synchronizeLanguagePack` / `addCustomServerLanguagePack` of a
    /// switch. Response is `ok`.
    Synchronize,
}

type Strings = Vec<(String, PackValue)>;

/// The requests of a switch whose refusal ends it: the `language_pack_id`
/// option and the pack synchronization. TDLib keeps the old language when
/// one is refused, so the switch leaves with the reason instead of
/// installing a pack TDLib did not select.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwitchGuards {
    pub option: Option<RequestId>,
    pub add: Option<RequestId>,
    pub sync: Option<RequestId>,
}

impl SwitchGuards {
    fn holds(&self, request: RequestId) -> bool {
        [self.option, self.add, self.sync].contains(&Some(request))
    }
}

/// A pack whose strings are being fetched: the pack's and its base pack's
/// strings, each under the request that asked for them.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageSwitch {
    pub info: LanguagePackInfoData,
    /// The pack's own strings.
    pub pack: Fetch<Strings>,
    /// The base pack's strings; a pack with no base pack starts with an
    /// empty, answered base.
    pub base: Fetch<Strings>,
    pub guards: SwitchGuards,
    /// What the server changed in this pack while its strings were being
    /// fetched; applied once the switch is ready.
    pub late: LateChange,
}

impl LanguageSwitch {
    /// A switch to `info`; `no_base` says the pack has no base pack.
    pub fn new(info: LanguagePackInfoData, no_base: bool) -> Self {
        Self {
            info,
            pack: Fetch::Idle,
            base: if no_base {
                Fetch::Ready(Vec::new())
            } else {
                Fetch::Idle
            },
            guards: SwitchGuards::default(),
            late: LateChange::Nothing,
        }
    }

    /// The pack's (`base` false) or the base pack's strings were asked as
    /// `request`.
    pub fn strings_sent(&mut self, base: bool, request: RequestId) {
        let fetch = if base { &mut self.base } else { &mut self.pack };
        fetch.begin();
        fetch.sent(request);
    }

    /// The strings `request` brought: taken by the fetch that waits for it.
    /// `false` when no fetch of this switch does (an older switch's answer).
    fn take_strings(&mut self, request: RequestId, strings: Strings) -> bool {
        if self.pack.waits_for(request) {
            self.pack.answered(Some(request), strings)
        } else {
            self.base.answered(Some(request), strings)
        }
    }

    /// `request` is one this switch waits for, so its refusal ends it.
    fn is_waiting_for(&self, request: RequestId) -> bool {
        self.pack.waits_for(request) || self.base.waits_for(request) || self.guards.holds(request)
    }
}

/// Where the interface language switch stands. It leaves by the strings
/// arriving, an error, or a newer switch replacing it.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum LanguageStage {
    #[default]
    Idle,
    Fetching(Box<LanguageSwitch>),
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
    /// Queue for the UI: strings to install, changed strings, a reload
    /// request, a failure.
    pub events: Vec<LanguageEvent>,
}

impl LanguageState {
    /// The switch whose strings are being fetched, mutably.
    pub fn switching_mut(&mut self) -> Option<&mut LanguageSwitch> {
        match &mut self.stage {
            LanguageStage::Fetching(switch) => Some(switch),
            LanguageStage::Idle => None,
        }
    }

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
                state.packs.answered(pending.map(|p| p.id), packs);
            }
            LanguagePayload::Strings(strings) => {
                if let Some(RequestPurpose::Settings(SettingsPurpose::Language(
                    LanguagePurpose::PsaLabels,
                ))) = pending.map(|p| p.purpose)
                {
                    self.chat_list
                        .psa_labels
                        .store(pending.map(|p| p.id), strings);
                    return;
                }
                let Some(pending) = pending.filter(|p| {
                    p.purpose
                        == RequestPurpose::Settings(SettingsPurpose::Language(
                            LanguagePurpose::Strings,
                        ))
                }) else {
                    return;
                };
                let LanguageStage::Fetching(switch) = &mut state.stage else {
                    return;
                };
                if !switch.take_strings(pending.id, strings) {
                    return;
                }
                if switch.pack.data().is_some()
                    && switch.base.data().is_some()
                    && let LanguageStage::Fetching(mut switch) = std::mem::take(&mut state.stage)
                    && let (Some(pack), Some(base)) =
                        (switch.pack.take_data(), switch.base.take_data())
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
            LanguagePurpose::TargetInfo => {
                state.packs.failed(request, error_reason(err), false);
            }
            // The switch's own strings, option or synchronize refused: TDLib
            // keeps the old language, so the switch leaves with the reason
            // instead of installing a pack TDLib did not select. A request
            // of an older switch fails nothing.
            LanguagePurpose::Strings
            | LanguagePurpose::SwitchOption
            | LanguagePurpose::Synchronize => {
                let waited = request
                    .zip(state.switching())
                    .is_some_and(|(request, switch)| switch.is_waiting_for(request));
                if waited {
                    state.stage = LanguageStage::Idle;
                    state.forget_failure();
                    state
                        .events
                        .push(LanguageEvent::SwitchFailed(error_reason(err)));
                }
            }
            // The badge types it asked for are asked again by the next pump.
            LanguagePurpose::PsaLabels => {
                if let Some(request) = request {
                    self.chat_list.psa_labels.fail_request(request);
                }
            }
            LanguagePurpose::SetOption => {}
        }
    }
}
