//! Appearance: interface font family, system accent refresh and the
//! "Battery and animations" switches (tdesktop `settings_power_saving.cpp`,
//! `chat/font` in `settings_chat.cpp`).
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_kit::component::theme::{ActiveTheme, Theme, installed_font_names};
use gpui_kit::component::{IndexPath, StyledExt as _};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::power_saving::{Flag, GROUPS, with_flag};
use std::sync::OnceLock;

/// The first entry of the font list: the platform default (`lng_font_default`).
const DEFAULT_LABEL: &str = "Default";

/// Installed font families, from the kit's list (enumerated once per run).
fn installed_fonts(cx: &App) -> &'static [String] {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    FONTS.get_or_init(|| quill::font_choice::choices(installed_font_names(cx).iter().cloned()))
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
        Flag::EmojiReactions => "power-emoji-reactions",
        Flag::EmojiChat => "power-emoji-chat",
        Flag::ChatBackground => "power-chat-background",
        Flag::ChatSpoiler => "power-spoiler",
        Flag::Calls => "power-calls",
        Flag::EmojiStatus => "power-emoji-status",
        Flag::ChatEffects => "power-chat-effects",
    }
}

impl QuillApp {
    /// Refresh the operating system's accent color. macOS and Windows read
    /// it right away. On Linux the read runs on a background thread (it
    /// spawns `gdbus`), so this only requests it and returns the cached
    /// value; a short poll applies the result once it lands.
    pub(in crate::ui) fn refresh_system_accent(&mut self, cx: &mut Context<Self>) {
        // Screenshot demos keep their fixture accent.
        if self.demo_session.is_some() {
            return;
        }
        self.settings.system_accent = quill::system_accent::read();
        self.settings.system_accent_probed = true;
        if quill::system_accent::pending() {
            cx.spawn(async move |this, cx| {
                while quill::system_accent::pending() {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                }
                let _ = this.update(cx, |this, cx| {
                    this.settings.system_accent = quill::system_accent::read();
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
    pub(in crate::ui) fn appearance_font_family_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let missing = !self.appearance.font_family.is_empty()
            && quill::font_choice::resolve(&self.appearance.font_family, installed_fonts(cx))
                .is_none();
        let hint = if missing {
            tr(keys::Q_MISC_THIS_FONT_ISNT_INSTALLED_SO).to_string()
        } else {
            tr(keys::Q_MISC_THE_FONT_FOR_THE_WHOLE).to_string()
        };
        self.appearance_section(
            cx,
            tr(keys::Q_MISC_FONT_FAMILY),
            &hint,
            Select::new(&self.settings.font_picker)
                .search_placeholder(tr(keys::Q_MISC_SEARCH_FONTS))
                .accessibility_label(tr(keys::Q_MISC_INTERFACE_FONT_FAMILY))
                .menu_max_h(px(280.))
                .w(px(260.))
                .into_any_element(),
        )
    }

    /// "Battery and animations": one switch per category, checked while the
    /// animation plays (tdesktop's checkboxes are the inverse of its flags),
    /// then "Save Power on Low Battery" where the platform reports a
    /// battery saver. While that saver forces everything on, the switches
    /// are dimmed and a click explains why they do not change.
    pub(in crate::ui) fn appearance_power_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let bits = self.appearance.power_saving;
        let muted = cx.theme().muted_foreground;
        let saver = quill::battery_saving::state();
        let forced = quill::battery_saving::forcing_for(
            saver,
            quill::battery_saving::thermal_hot(),
            self.appearance.ignore_battery_saving,
        );
        let mut groups = div().flex().flex_col().gap_2();
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
            groups = groups.child(block);
        }
        if forced {
            // tdesktop dims the box and a click shows the toast.
            groups = groups.relative().opacity(0.6).child(
                div()
                    .id("power-forced-overlay")
                    .absolute()
                    .inset_0()
                    .cursor_pointer()
                    .occlude()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_status_note(StatusNote::silent(forced_message()), cx);
                    })),
            );
        }
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .child(tr(keys::SETTINGS_POWER_MENU)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::SETTINGS_POWER_SUBTITLE)),
            )
            .child(groups);
        if saver.is_some() {
            body = body
                .child(div().mt_2().child(self.appearance_switch_row(
                    cx,
                    "power-auto",
                    tr(keys::SETTINGS_POWER_AUTO),
                    "",
                    !self.appearance.ignore_battery_saving,
                    |prefs, on| prefs.ignore_battery_saving = !on,
                )))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::SETTINGS_POWER_AUTO_ABOUT)),
                );
        }
        body.into_any_element()
    }
}

/// tdesktop `lng_settings_power_turn_off`.
fn forced_message() -> &'static str {
    tr(keys::SETTINGS_POWER_TURN_OFF)
}
