//! Appearance: interface font family, system accent refresh and the
//! "Battery and animations" switches (tdesktop `settings_power_saving.cpp`,
//! `chat/font` in `settings_chat.cpp`).

use super::QuillApp;
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_kit::component::theme::{ActiveTheme, Theme};
use gpui_kit::component::{IndexPath, StyledExt as _};
use gpui_kit::*;
use quill::power_saving::{Flag, GROUPS, with_flag};
use std::sync::OnceLock;

/// The first entry of the font list: the platform default (`lng_font_default`).
const DEFAULT_LABEL: &str = "Default";

/// Installed font families, queried once: fonts don't change while running.
fn installed_fonts(cx: &App) -> &'static [String] {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    FONTS.get_or_init(|| quill::font_choice::choices(cx.text_system().all_font_names()))
}

/// The family to draw the interface in: the stored choice when it is still
/// installed, else the platform default the theme started with.
pub(super) fn interface_font(stored: &str, cx: &App) -> String {
    static DEFAULT: OnceLock<String> = OnceLock::new();
    let default = DEFAULT.get_or_init(|| cx.global::<Theme>().font_family.to_string());
    if stored.trim().is_empty() {
        return default.clone();
    }
    quill::font_choice::resolve(stored, installed_fonts(cx))
        .map_or_else(|| default.clone(), str::to_string)
}

/// The switch id for a power-saving flag.
fn flag_id(flag: Flag) -> &'static str {
    match flag {
        Flag::Animations => "power-animations",
        Flag::StickersPanel => "power-stickers-panel",
        Flag::StickersChat => "power-stickers-chat",
        Flag::EmojiPanel => "power-emoji-panel",
        Flag::EmojiChat => "power-emoji-chat",
        Flag::ChatSpoiler => "power-spoiler",
        Flag::Calls => "power-calls",
    }
}

impl QuillApp {
    /// Refresh the operating system's accent color. macOS and Windows read
    /// it right away. On Linux the read runs on a background thread (it
    /// spawns `gdbus`), so this only requests it and returns the cached
    /// value; a short poll applies the result once it lands.
    pub(super) fn refresh_system_accent(&mut self, cx: &mut Context<Self>) {
        // Screenshot demos keep their fixture accent.
        if self.demo_session.is_some() {
            return;
        }
        self.system_accent = quill::system_accent::read();
        self.system_accent_probed = true;
        if quill::system_accent::pending() {
            cx.spawn(async move |this, cx| {
                while quill::system_accent::pending() {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                }
                let _ = this.update(cx, |this, cx| {
                    this.system_accent = quill::system_accent::read();
                    this.apply_appearance(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// The font family picker state: "Default" first, then the installed
    /// families, searchable.
    pub(super) fn new_font_picker(
        stored: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<SearchableVec<SharedString>>> {
        let installed = installed_fonts(cx);
        let mut items: Vec<SharedString> = vec![DEFAULT_LABEL.into()];
        items.extend(
            installed
                .iter()
                .map(|name| SharedString::from(name.clone())),
        );
        let selected = quill::font_choice::resolve(stored, installed)
            .and_then(|name| installed.iter().position(|n| n == name))
            .map_or(0, |ix| ix + 1);
        let picker = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(items),
                Some(IndexPath::new(selected)),
                window,
                cx,
            )
            .searchable(true)
        });
        cx.subscribe(
            &picker,
            |this, _, event: &SelectEvent<SearchableVec<SharedString>>, cx| {
                let SelectEvent::Confirm(value) = event;
                let family = match value {
                    Some(name) if name.as_ref() != DEFAULT_LABEL => name.to_string(),
                    _ => String::new(),
                };
                this.set_appearance(cx, |a| a.font_family = family);
            },
        )
        .detach();
        picker
    }

    /// tdesktop's "Font family" row.
    pub(super) fn appearance_font_family_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let missing = !self.appearance.font_family.is_empty()
            && quill::font_choice::resolve(&self.appearance.font_family, installed_fonts(cx))
                .is_none();
        let hint = if missing {
            "This font isn't installed, so the default is used.".to_string()
        } else {
            "The font for the whole interface. Code stays monospaced.".to_string()
        };
        self.appearance_section(
            cx,
            "Font family",
            &hint,
            Select::new(&self.font_picker)
                .search_placeholder("Search fonts")
                .accessibility_label("Interface font family")
                .menu_max_h(px(280.))
                .w(px(260.))
                .into_any_element(),
        )
    }

    /// "Battery and animations": one switch per category, checked while the
    /// animation plays (tdesktop's checkboxes are the inverse of its flags).
    pub(super) fn appearance_power_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let bits = self.appearance.power_saving;
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .font_semibold()
                .text_sm()
                .child("Battery and animations"),
        );
        for group in &GROUPS {
            let mut block = div().flex().flex_col().gap_1();
            if let Some(title) = group.title {
                block = block.child(div().text_xs().text_color(muted).child(title));
            }
            for &(flag, label) in group.items {
                let playing = bits & flag.bit() == 0;
                block = block.child(self.appearance_switch_row(
                    cx,
                    flag_id(flag),
                    label,
                    "",
                    playing,
                    move |prefs, on| prefs.power_saving = with_flag(prefs.power_saving, flag, !on),
                ));
            }
            body = body.child(block);
        }
        body.child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Switch an animation off to keep it still and save battery."),
        )
        .into_any_element()
    }
}
