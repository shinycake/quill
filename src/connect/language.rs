//! Connect driver: interface language packs (docs/decisions/language-packs.md).
//! TDLib serves Telegram Desktop's strings (`localization_target`); the
//! driver fetches a pack's strings and the UI installs them.
use super::*;
use crate::ids::RequestId;
use crate::lang::prefs::{LanguageChoice, database_path, load_choice};
use crate::state::{
    LOCALIZATION_TARGET, LanguagePurpose, LanguageSwitch, RequestPurpose, SettingsPurpose,
};
use crate::telegram::envelope::LanguagePackInfoData;
use crate::telegram::requests::{
    add_custom_server_language_pack, get_language_pack_strings, get_localization_target_info,
    set_option_string, synchronize_language_pack,
};

fn purpose(purpose: LanguagePurpose) -> RequestPurpose {
    RequestPurpose::Settings(SettingsPurpose::Language(purpose))
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Right after `setTdlibParameters` (TDLib runs these once it is
    /// initialized): point TDLib at Telegram Desktop's strings and the
    /// shared pack database, select the saved pack and fetch its strings
    /// so the interface matches TDLib's latest copy.
    pub(crate) fn send_language_options(&mut self) -> Result<(), ConnectSendError> {
        let root = crate::settings::app_root_of(&self.paths).map(std::path::Path::to_path_buf);
        self.send_language_option(
            "localization_target",
            LOCALIZATION_TARGET,
            LanguagePurpose::SetOption,
        )?;
        if let Some(root) = &root {
            let path = database_path(root);
            self.send_language_option(
                "language_pack_database_path",
                &path.to_string_lossy(),
                LanguagePurpose::SetOption,
            )?;
        }
        let choice = root.as_deref().map(load_choice).unwrap_or_default();
        if choice.is_builtin() {
            return Ok(());
        }
        self.switch_language(info_of(&choice))
    }

    fn send_language_option(
        &mut self,
        name: &str,
        value: &str,
        kind: LanguagePurpose,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.session.request(purpose(kind), None);
        self.send_json_request(extra, &set_option_string(extra, name, value))
    }

    /// `getLocalizationTargetInfo` for the Language box.
    pub fn request_language_packs(&mut self) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(purpose(LanguagePurpose::TargetInfo), None);
        self.session.settings.language.packs.begin_with(extra);
        let result = self.send_json_request(extra, &get_localization_target_info(extra, false));
        // A send that failed is a failed list, drawn with Retry: back to
        // Idle would ask again at the next render.
        if result.is_err() {
            self.session.settings.language.packs.fail(crate::lang::tr(
                crate::lang::keys::Q_NOTES_SOMETHING_WENT_WRONG,
            ));
        }
        result
    }

    /// Switch the interface to `info`: select it in TDLib, fetch its strings
    /// (and its base pack's), then keep it synchronized. The strings land
    /// as a `LanguageEvent::Install` for the UI to install. English (`en`
    /// or an empty id) needs no strings: `ready` carries none.
    pub fn switch_language(&mut self, info: LanguagePackInfoData) -> Result<(), ConnectSendError> {
        let state = &mut self.session.settings.language;
        state.forget_failure();
        let builtin = info.id.is_empty() || info.id == "en";
        let id = if builtin {
            "en".to_string()
        } else {
            info.id.clone()
        };
        if builtin {
            state.stage = crate::state::LanguageStage::Idle;
            state.current = None;
            state.queue_install(crate::state::LanguageReady {
                info,
                pack: Vec::new(),
                base: Vec::new(),
            });
            return self
                .send_language_option("language_pack_id", &id, LanguagePurpose::SetOption)
                .map(|_| ());
        }
        let base_id = info.base_language_pack_id.clone();
        let custom = !info.is_official && !info.is_installed;
        state.stage = crate::state::LanguageStage::Fetching(Box::new(LanguageSwitch::new(
            info,
            base_id.is_empty(),
        )));
        let sent = self.send_language_switch(&id, &base_id, custom);
        if sent.is_err() {
            // A send that never left must not leave the switch pending, and
            // TDLib must not keep serving the pack the option already named:
            // the interface stays on the pack it shows.
            self.session.settings.language.stage = crate::state::LanguageStage::Idle;
            let _ = self.restore_language_option();
        }
        sent
    }

    /// Send the switch's requests; each one's id goes into the switch that
    /// waits for it, so only its own answer or refusal acts.
    fn send_language_switch(
        &mut self,
        id: &str,
        base_id: &str,
        custom: bool,
    ) -> Result<(), ConnectSendError> {
        if custom {
            let extra = self
                .session
                .request(purpose(LanguagePurpose::Synchronize), None);
            self.send_json_request(extra, &add_custom_server_language_pack(extra, id))?;
            self.note_switch(|switch| switch.guards.add = Some(extra));
        }
        let option =
            self.send_language_option("language_pack_id", id, LanguagePurpose::SwitchOption)?;
        self.note_switch(|switch| switch.guards.option = Some(option));
        for (pack, base) in [(id, false), (base_id, true)] {
            if pack.is_empty() {
                continue;
            }
            let extra = self
                .session
                .request(purpose(LanguagePurpose::Strings), None);
            self.send_json_request(extra, &get_language_pack_strings(extra, pack))?;
            self.note_switch(|switch| switch.strings_sent(base, extra));
        }
        let extra = self
            .session
            .request(purpose(LanguagePurpose::Synchronize), None);
        self.send_json_request(extra, &synchronize_language_pack(extra, id))?;
        self.note_switch(|switch| switch.guards.sync = Some(extra));
        Ok(())
    }

    fn note_switch(&mut self, record: impl FnOnce(&mut LanguageSwitch)) {
        if let Some(switch) = self.session.settings.language.switching_mut() {
            record(switch);
        }
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// The client comes on screen after running in the background. What its
    /// language flow made there (its startup pack, a reload, string updates)
    /// is for the choice saved when it started, and the interface may have
    /// moved on since: installing it would put the old pack back and save
    /// it as the choice. It is dropped, and the client follows the choice
    /// saved now.
    pub fn adopt_saved_language(&mut self) -> Result<(), ConnectSendError> {
        let state = &mut self.session.settings.language;
        state.drop_all_but_failures();
        let root = crate::settings::app_root_of(&self.paths).map(std::path::Path::to_path_buf);
        let choice = root.as_deref().map(load_choice).unwrap_or_default();
        let wanted = (!choice.is_builtin()).then_some(choice.pack_id.as_str());
        let shown = state.current.as_ref().map(|info| info.id.as_str());
        let switching = state.switching().map(|switch| switch.info.id.as_str());
        if switching.map_or(shown == wanted, |id| Some(id) == wanted) {
            return Ok(());
        }
        self.switch_language(info_of(&choice))
    }

    /// After a failed switch: point TDLib back at the pack the interface
    /// still uses.
    pub fn restore_language_option(&mut self) -> Result<(), ConnectSendError> {
        let id = self
            .session
            .settings
            .language
            .current
            .as_ref()
            .map_or_else(|| "en".to_string(), |info| info.id.clone());
        self.send_language_option("language_pack_id", &id, LanguagePurpose::SetOption)
            .map(|_| ())
    }
}

/// The pack info a saved choice describes (TDLib's list may not be loaded).
pub(crate) fn info_of(choice: &LanguageChoice) -> LanguagePackInfoData {
    LanguagePackInfoData {
        id: choice.pack_id.clone(),
        base_language_pack_id: choice.base_pack_id.clone(),
        name: choice.name.clone(),
        native_name: choice.native_name.clone(),
        plural_code: choice.plural_code.clone(),
        is_official: true,
        is_rtl: choice.is_rtl,
        is_beta: false,
        is_installed: true,
        total_string_count: 0,
        translated_string_count: 0,
    }
}
