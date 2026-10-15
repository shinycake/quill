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
        self.send_language_option("localization_target", LOCALIZATION_TARGET)?;
        if let Some(root) = &root {
            let path = database_path(root);
            self.send_language_option("language_pack_database_path", &path.to_string_lossy())?;
        }
        let choice = root.as_deref().map(load_choice).unwrap_or_default();
        if choice.is_builtin() {
            return Ok(());
        }
        self.switch_language(info_of(&choice))
    }

    fn send_language_option(&mut self, name: &str, value: &str) -> Result<(), ConnectSendError> {
        let extra = self
            .session
            .request(purpose(LanguagePurpose::SetOption), None);
        self.send_json_request(extra, &set_option_string(extra, name, value))
            .map(|_| ())
    }

    /// `getLocalizationTargetInfo` for the Language box.
    pub fn request_language_packs(&mut self) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(purpose(LanguagePurpose::TargetInfo), None);
        self.session.settings.language.packs_loading = true;
        let result = self.send_json_request(extra, &get_localization_target_info(extra, false));
        if result.is_err() {
            self.session.settings.language.packs_loading = false;
        }
        result
    }

    /// Switch the interface to `info`: select it in TDLib, fetch its strings
    /// (and its base pack's), then keep it synchronized. The strings land
    /// in `settings.language.ready` for the UI to install. English (`en`
    /// or an empty id) needs no strings: `ready` carries none.
    pub fn switch_language(&mut self, info: LanguagePackInfoData) -> Result<(), ConnectSendError> {
        let state = &mut self.session.settings.language;
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        state.error = None;
        let builtin = info.id.is_empty() || info.id == "en";
        let id = if builtin {
            "en".to_string()
        } else {
            info.id.clone()
        };
        if builtin {
            state.switching = None;
            state.current = None;
            state.ready = Some(crate::state::LanguageReady {
                info,
                pack: Vec::new(),
                base: Vec::new(),
            });
            return self.send_language_option("language_pack_id", &id);
        }
        let base_id = info.base_language_pack_id.clone();
        let custom = !info.is_official && !info.is_installed;
        state.switching = Some(LanguageSwitch {
            info,
            generation,
            pack: None,
            base: base_id.is_empty().then(Vec::new),
        });
        if custom {
            let extra = self
                .session
                .request(purpose(LanguagePurpose::Synchronize), None);
            self.send_json_request(extra, &add_custom_server_language_pack(extra, &id))?;
        }
        self.send_language_option("language_pack_id", &id)?;
        for (pack, base) in [(id.as_str(), false), (base_id.as_str(), true)] {
            if pack.is_empty() {
                continue;
            }
            let extra = self
                .session
                .request(purpose(LanguagePurpose::Strings { generation, base }), None);
            self.send_json_request(extra, &get_language_pack_strings(extra, pack))?;
        }
        let extra = self
            .session
            .request(purpose(LanguagePurpose::Synchronize), None);
        self.send_json_request(extra, &synchronize_language_pack(extra, &id))
            .map(|_| ())
    }
}

impl<S: JsonSender> ConnectDriver<S> {
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
        self.send_language_option("language_pack_id", &id)
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
