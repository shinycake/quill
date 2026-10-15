//! Screenshot demos and fixtures of the Language box: a short official
//! list and a Brazilian Portuguese pack with the strings the box and the
//! Settings list use, so a switch can be seen without Telegram.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::lang::PackValue;
use quill::state::LanguageReady;
use quill::telegram::envelope::LanguagePackInfoData;

register_demos![
    DemoSpec::chats("ready-language", "screenshot demo — Settings → Language")
        .setup(QuillApp::demo_language_box),
    DemoSpec::chats(
        "ready-language-pt",
        "screenshot demo — Settings → Language after switching to Português (Brasil)"
    )
    .setup(QuillApp::demo_language_box_pt),
];

fn pack(id: &str, name: &str, native: &str, plural: &str, rtl: bool) -> LanguagePackInfoData {
    LanguagePackInfoData {
        id: id.into(),
        base_language_pack_id: String::new(),
        name: name.into(),
        native_name: native.into(),
        plural_code: plural.into(),
        is_official: true,
        is_rtl: rtl,
        is_beta: false,
        is_installed: false,
        total_string_count: 0,
        translated_string_count: 0,
    }
}

/// The official list the demos show.
pub(super) fn demo_packs() -> Vec<LanguagePackInfoData> {
    vec![
        pack("en", "English", "English", "en", false),
        pack(
            "pt-br",
            "Portuguese (Brazil)",
            "Português (Brasil)",
            "pt",
            false,
        ),
        pack("de", "German", "Deutsch", "de", false),
        pack("es", "Spanish", "Español", "es", false),
        pack("fr", "French", "Français", "fr", false),
        pack("it", "Italian", "Italiano", "it", false),
        pack("he", "Hebrew", "עברית", "he", true),
        pack("ja", "Japanese", "日本語", "ja", false),
    ]
}

const PT_BR: &[(&str, &str)] = &[
    ("lng_languages", "Idioma"),
    ("lng_settings_language", "Idioma"),
    ("lng_languages_none", "Nenhum idioma encontrado."),
    ("lng_participant_filter", "Pesquisar"),
    ("lng_box_ok", "OK"),
    ("lng_proxy_edit_share", "Compartilhar"),
    ("lng_proxy_menu_delete", "Apagar"),
    ("lng_proxy_menu_restore", "Restaurar"),
    ("lng_translate_settings_show", "Mostrar Botão Traduzir"),
    ("lng_translate_settings_chat", "Traduzir Chats Inteiros"),
    ("lng_translate_settings_choose", "Não Traduzir"),
    ("lng_translate_settings_subtitle", "Traduzir Mensagens"),
    ("lng_menu_settings", "Configurações"),
    ("lng_settings_section_notify", "Notificações e Sons"),
    ("lng_settings_section_privacy", "Privacidade e Segurança"),
    (
        "lng_settings_section_chat_settings",
        "Configurações de Chat",
    ),
    ("lng_settings_advanced", "Avançado"),
];

/// What switching to `info` produces in demos: the Portuguese fixture for
/// `pt-br`, English for anything else.
pub(super) fn demo_ready(info: LanguagePackInfoData) -> LanguageReady {
    let pack = if info.id == "pt-br" {
        PT_BR
            .iter()
            .map(|(key, value)| ((*key).to_string(), PackValue::Ordinary((*value).into())))
            .chain(std::iter::once((
                "lng_languages_count".to_string(),
                PackValue::Pluralized([
                    String::new(),
                    "{count} idioma".into(),
                    String::new(),
                    String::new(),
                    String::new(),
                    "{count} idiomas".into(),
                ]),
            )))
            .collect()
    } else {
        Vec::new()
    };
    let info = if pack.is_empty() {
        pack_or_english(None)
    } else {
        info
    };
    LanguageReady {
        info,
        pack,
        base: Vec::new(),
    }
}

fn pack_or_english(info: Option<LanguagePackInfoData>) -> LanguagePackInfoData {
    info.unwrap_or_else(|| pack("en", "English", "English", "en", false))
}

impl QuillApp {
    fn demo_language_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_language_box(window, cx);
    }

    fn demo_language_box_pt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(info) = demo_packs().into_iter().find(|p| p.id == "pt-br") {
            self.choose_language(info, cx);
        }
        self.open_language_box(window, cx);
    }
}
