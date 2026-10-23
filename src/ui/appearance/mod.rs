//! Settings → Appearance slice: the dialog plus the load/save/apply
//! funnel. Everything here is client-side — theme, auto-night, accent,
//! wallpaper, message font size and bubble style live in
//! `appearance_prefs.json` (see `quill::settings::AppearancePrefs`);
//! there is no TDLib setting for any of it. The exception is the
//! Language section (slice parity:settings-language): its tag is the
//! `system_language_code` sent in `setTdlibParameters`, persisted in
//! `language_prefs.json` and applied on restart.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::chat_theme::{set_high_contrast, set_theme_mode};
use super::synthetic::BubbleLook;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::{ActiveTheme, Theme, ThemeMode};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::AccountKey;
use quill::lang::{Key, keys, tr, tr_with};
use quill::settings::{
    AccountPaths, AppearancePrefs, AutoNight, ChatPrefs, ThemeChoice, clamp_font_size,
    load_appearance_prefs, load_chat_prefs, safe_app_root, save_appearance_prefs, save_chat_prefs,
};
/// Accent presets (0xRRGGBB); the "Default" chip keeps the theme accent.
/// The kit's unscaled rem size (gpui-component `Theme::font_size`).
const BASE_REM_PX: f32 = 16.;

/// tdesktop's lightness limits for a custom accent (`ColorizerFrom` in
/// window_themes_embedded.cpp, applied by the accent `ColorEditor`): at
/// most 160/255 on day themes and at least 64/255 on night themes, so
/// white text on the accent and the accent on the window stay readable.
/// Opaque: the accent has no alpha.
pub(crate) fn limit_custom_accent(color: Hsla, dark: bool) -> Hsla {
    let l = if dark {
        color.l.max(64. / 255.)
    } else {
        color.l.min(160. / 255.)
    };
    Hsla { l, a: 1., ..color }
}

/// `color` as the `accent_rgb` setting (0xRRGGBB). 0 means "theme
/// default" there, so pure black is stored as 0x000001.
pub(crate) fn accent_rgb_from(color: Hsla) -> u32 {
    let rgba = Rgba::from(color);
    let channel = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    let value = (channel(rgba.r) << 16) | (channel(rgba.g) << 8) | channel(rgba.b);
    value.max(1)
}

/// Wallpaper presets (0xRRGGBB); the "Default" chip keeps the theme
/// background.
const WALLPAPER_PRESETS: &[(u32, Key)] = &[
    (0x0e1621, keys::Q_LEFTOVERS_WALLPAPER_DARK_BLUE),
    (0x17212b, keys::Q_LEFTOVERS_WALLPAPER_SLATE),
    (0x1c2b33, keys::Q_LEFTOVERS_WALLPAPER_TEAL),
    (0x2a1e2e, keys::Q_LEFTOVERS_WALLPAPER_PLUM),
    (0x141414, keys::Q_LEFTOVERS_WALLPAPER_NEAR_BLACK),
];

impl QuillApp {
    pub(crate) fn minimize_to_tray(&self) -> bool {
        self.appearance.minimize_to_tray
    }
    pub(super) fn open_settings_on(&mut self, page: quill::settings_catalog::Page) {
        self.open_settings(Some(page));
    }

    /// Shortcuts take keystrokes for rebinding only while their page is up.
    pub(super) fn keybinding_capture_active(&mut self) -> bool {
        let shown = self.settings.open
            && self.settings.page == Some(quill::settings_catalog::Page::Shortcuts);
        super::keybindings::capture_active(
            shown,
            &mut self.settings.keybinding_capture,
            &mut self.settings.keybinding_error,
        )
    }

    /// Account-rooted prefs path. Appearance is a device setting (like
    /// Telegram's locally stored theme choice), so it lives under the
    /// primary account's root rather than per-account data.
    pub(crate) fn appearance_paths() -> AccountPaths {
        // `safe_app_root` has no `./quill-data` fallback (security: never
        // scatter account state under the launch directory); without a
        // platform data dir the prefs fall back to the temp dir.
        let root = safe_app_root().unwrap_or_else(|| std::env::temp_dir().join("quill-appearance"));
        AccountPaths::for_root(&root, &AccountKey::primary())
    }

    /// Load persisted prefs (defaults when the file is missing/corrupt).
    pub(crate) fn load_appearance() -> AppearancePrefs {
        let prefs = load_appearance_prefs(&Self::appearance_paths());
        quill::tray::monochrome::set_monochrome(prefs.tray_icon_monochrome);
        prefs
    }

    /// Recompute the effective theme from the prefs (manual choice,
    /// overridden by auto-night while active) and push it into the
    /// gpui-component global Theme. `Theme::change` resets the whole
    /// palette, so the accent override is re-applied after every mode
    /// change, and `Theme::update` re-derives tokens and the Base layer
    /// projection from it. Only notifies when the (mode,
    /// accent, high-contrast) triple actually changed — the minute tick
    /// calls this and must be free when idle.
    ///
    /// stories-high-contrast: `ThemeChoice::HighContrast` pairs the dark
    /// kit theme with the high-contrast token palette and wins over
    /// auto-night — an explicit accessibility choice is never silently
    /// reverted by the schedule.
    pub(crate) fn apply_appearance(&mut self, cx: &mut Context<Self>) {
        let hc = self.appearance.theme == ThemeChoice::HighContrast;
        // The day or night theme (tdesktop keeps one of each and switches
        // with night mode); high contrast keeps its own palette.
        let (theme_id, palette) = self.resolve_app_theme(hc || self.night_mode_now(cx));
        // Quill Light and Quill Dark (and high contrast) are drawn by the
        // token pairs and the kit's own colours, exactly as before themes:
        // no palette is installed for them.
        let painted = !hc
            && !theme_id
                .builtin()
                .is_some_and(quill::app_theme::BuiltinTheme::is_quill);
        let dark = hc || palette.is_dark();
        let mode = if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        // Power saving: the switches gate animations as they are drawn; the
        // interface-animations one also folds into GPUI's reduced motion.
        #[cfg(test)]
        quill::power_saving::isolate_for_tests();
        quill::power_saving::set(self.appearance.power_saving);
        let ignored_before = quill::battery_saving::ignored();
        quill::battery_saving::set_ignore(self.appearance.ignore_battery_saving);
        if ignored_before != self.appearance.ignore_battery_saving {
            // The probe task parks while the switch is off.
            quill::wake::BATTERY.poke();
        }
        quill::battery_saving::apply();
        if self.demo_session.is_none() {
            super::battery_watch::watch(cx);
        }
        self.start_omarchy_watch(cx);
        cx.set_reduce_motion(quill::power_saving::reduce_motion_now());
        // Quill's themes and high contrast take the single accent of old.
        let accent = quill::system_accent::effective(
            self.appearance.accent_rgb,
            self.appearance.system_accent,
            self.settings.system_accent,
        );
        let scale = self.appearance.interface_scale_pct;
        let family = super::appearance_power::interface_font(&self.appearance.font_family, cx);
        let look = if painted {
            palette.fingerprint()
        } else {
            u64::from(accent)
        };
        let applied = (mode, look, hc, scale, family.clone());
        if self.settings.appearance_applied.as_ref() == Some(&applied) {
            return;
        }
        set_theme_mode(mode, None, cx);
        set_high_contrast(hc);
        self.install_app_theme((!hc).then(|| (theme_id, palette.clone())), painted);
        let primary = if accent == 0 {
            Hsla::from(super::chat_theme::accent_strong())
        } else {
            Hsla::from(rgb(accent))
        };
        // `Theme::update` re-derives the renderable tokens from `colors`,
        // re-projects the Base layer and refreshes windows. Mutating
        // `global_mut` alone would leave primary buttons on the old accent.
        Theme::update(cx, |theme| {
            let colors = &mut theme.colors;
            if painted {
                super::app_theme::kit_colors::apply(colors, &palette);
            } else {
                // Primary buttons draw from their own tokens: point them at
                // the accent too, or Send and other primary actions stay
                // neutral.
                colors.primary = primary;
                colors.primary_foreground = gpui_kit::white();
                colors.button_primary = primary;
                colors.button_primary_hover = Hsla {
                    l: (primary.l + 0.06).min(1.),
                    ..primary
                };
                colors.button_primary_active = Hsla {
                    l: (primary.l - 0.06).max(0.),
                    ..primary
                };
                colors.button_primary_foreground = gpui_kit::white();
                // The kit's red is 3.8:1 on white: keep destructive text
                // and fills at WCAG AA.
                super::app_theme::kit_colors::ensure_danger_aa(colors, dark);
            }
            // The rem size stays at the kit's default: the interface scale
            // zooms whole windows instead (below), so scaling rems too
            // would apply it twice.
            theme.font_size = px(BASE_REM_PX);
            theme.font_family = family.into();
        });
        // Interface scale: every window is drawn `zoom` times larger
        // (`interface_zoom`). GPUI re-lays the windows out through their
        // resize callbacks, which need the windows free: defer past this
        // update.
        let zoom = super::interface_zoom::zoom_for_percent(scale);
        cx.defer(move |_| super::interface_zoom::set_zoom(zoom));
        self.settings.appearance_applied = Some(applied);
        cx.notify();
    }

    /// The single funnel every Appearance control uses: mutate, clamp,
    /// persist, re-apply, re-render. If the save fails the change is
    /// still applied live — the status note reports the failure instead
    /// of pretending the change was saved.
    pub(crate) fn set_appearance(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut AppearancePrefs),
    ) {
        f(&mut self.appearance);
        quill::tray::set_tray_enabled(self.appearance.show_tray_icon);
        quill::tray::monochrome::set_monochrome(self.appearance.tray_icon_monochrome);
        self.appearance.font_size_px = clamp_font_size(self.appearance.font_size_px);
        if let Err(err) = save_appearance_prefs(&Self::appearance_paths(), &self.appearance) {
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_PREFS_COULDNT_SAVE_APPEARANCE_SETTINGS,
                &[("err", &err.to_string())],
            )));
        }
        self.apply_appearance(cx);
        cx.notify();
    }

    /// Load persisted chat prefs (defaults when the file is missing/corrupt).
    pub(crate) fn load_chat_prefs() -> ChatPrefs {
        load_chat_prefs(&Self::appearance_paths())
    }

    /// The single funnel for chat-prefs controls: mutate, persist,
    /// re-render. Same failure contract as `set_appearance`.
    pub(crate) fn set_chat_prefs(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ChatPrefs),
    ) {
        f(&mut self.chat_prefs);
        if let Err(err) = save_chat_prefs(&Self::appearance_paths(), &self.chat_prefs) {
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_PREFS_COULDNT_SAVE_CHAT_SETTINGS,
                &[("err", &err.to_string())],
            )));
        }
        // Keep kit's newline-vs-submit behavior in sync with the mode on
        // the two chat composers (other inputs always submit on Enter).
        let submit = self.chat_prefs.send_key_mode == quill::composer::SendKeyMode::Enter;
        self.composer.update(cx, |input, cx| {
            input.set_submit_on_enter(submit, cx);
        });
        self.group_call.composer.update(cx, |input, cx| {
            input.set_submit_on_enter(submit, cx);
        });
        cx.notify();
    }

    /// Step the scheduled auto-night start/end time (30-minute steps,
    /// wraps past midnight).
    fn bump_night_time(&mut self, cx: &mut Context<Self>, is_start: bool, delta: i16) {
        self.set_appearance(cx, |a| {
            let cur = if is_start {
                a.night_start_minutes
            } else {
                a.night_end_minutes
            } as i16;
            let next = (cur + delta).rem_euclid(24 * 60) as u16;
            if is_start {
                a.night_start_minutes = next;
            } else {
                a.night_end_minutes = next;
            }
        });
    }

    /// Step the message font size (clamped to 12–20 px in
    /// `set_appearance`).
    fn bump_font_size(&mut self, cx: &mut Context<Self>, delta: i8) {
        self.set_appearance(cx, |a| {
            a.font_size_px = a.font_size_px.saturating_add_signed(delta);
        });
    }

    /// Current message font size (Settings → Appearance → Message text
    /// size).
    pub(crate) fn msg_font(&self) -> Pixels {
        px(self.appearance.font_size_px as f32)
    }

    /// Bubble look for history rows: font size + bubble/plain style.
    /// Text is white in bubble mode, theme foreground in plain mode.
    /// Takes `&App` (not `&mut Context`) so callers can keep using `cx`
    /// afterwards.
    pub(crate) fn bubble_look(&self, cx: &App) -> BubbleLook {
        BubbleLook {
            font: self.msg_font(),
            plain: !self.appearance.bubbles,
            text: if self.appearance.bubbles {
                Hsla::from(rgb(0xffffff))
            } else {
                cx.theme().foreground
            },
            joined_above: false,
            out_fill: self
                .chat_look(self.open_chat_id().map(|c| c.0), cx)
                .outgoing,
            wide: self.chat_wide(),
            history: self.history.width.get(),
        }
    }

    /// A labeled section: title + control row + hint line.
    pub(super) fn appearance_section(
        &self,
        cx: &mut Context<Self>,
        title: &str,
        hint: &str,
        control: AnyElement,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().font_semibold().text_sm().child(title.to_string()))
            .child(control)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(hint.to_string()),
            )
            .into_any_element()
    }

    /// A setting with a small control (a switch, a button): title and hint
    /// on the left, the control on the right, like the newer settings rows.
    pub(crate) fn appearance_toggle_section(
        &self,
        cx: &mut Context<Self>,
        title: &str,
        hint: &str,
        control: AnyElement,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().text_sm().child(title.to_string()))
                    .when(!hint.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(hint.to_string()),
                        )
                    }),
            )
            .child(div().flex_none().child(control))
            .into_any_element()
    }

    /// A selectable chip; the selected one gets the accent border.
    pub(crate) fn appearance_chip(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        selected: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut QuillApp, &mut Context<QuillApp>) + 'static,
    ) -> AnyElement {
        let theme = cx.theme();
        let label = label.into();
        div()
            .id(id.into())
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme.primary
            } else {
                theme.border
            })
            .when(selected, |this| this.bg(theme.primary.opacity(0.15)))
            .role(gpui_kit::Role::Button)
            .aria_label(label.clone())
            .tab_index(0)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
            .child(div().text_sm().child(label))
            .into_any_element()
    }

    /// A color swatch; the selected one gets the accent ring.
    pub(crate) fn appearance_swatch(
        &self,
        id: impl Into<SharedString>,
        color: u32,
        name: &str,
        selected: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut QuillApp, &mut Context<QuillApp>) + 'static,
    ) -> AnyElement {
        let theme = cx.theme();
        div()
            .id(id.into())
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .role(gpui_kit::Role::Button)
            .aria_label(format!(
                "{name}{}",
                if selected {
                    tr(keys::Q_PREFS_SELECTED)
                } else {
                    ""
                }
            ))
            .tab_index(0)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
            .child(
                div()
                    .w(px(32.))
                    .h(px(32.))
                    .rounded_md()
                    .bg(rgb(color))
                    .border_2()
                    .border_color(if selected {
                        theme.accent
                    } else {
                        Hsla::from(rgba(0x00000000))
                    }),
            )
            .child(div().text_xs().child(name.to_string()))
            .into_any_element()
    }

    /// A −/+ stepper for the scheduled auto-night window (30-minute
    /// steps, wraps past midnight).
    fn appearance_time_stepper(
        &self,
        id_prefix: &str,
        minutes: u16,
        is_start: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = format!("{:02}:{:02}", minutes / 60, minutes % 60);
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new(format!("{id_prefix}-down"))
                    .label("−")
                    .ghost()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.bump_night_time(cx, is_start, -30)),
                    ),
            )
            .child(div().text_sm().min_w(px(48.)).text_center().child(label))
            .child(
                Button::new(format!("{id_prefix}-up"))
                    .label("+")
                    .ghost()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.bump_night_time(cx, is_start, 30)),
                    ),
            )
            .into_any_element()
    }

    /// Tray and window-close switches of the General settings. `tray_available`
    /// is a parameter so the screenshot demo can show them without a tray.
    pub(in crate::ui) fn window_behavior_sections(
        &self,
        tray_available: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let this = self;
        let mut body: Vec<AnyElement> = Vec::new();
        // Tray-dependent switches only exist while a tray icon does:
        // a hidden window with no tray to reopen it from would
        // strand the user (Linux without a StatusNotifier host).
        let tray =
            quill::tray::tray_setting_switches(tray_available, this.appearance.show_tray_icon);
        body.extend(this.tray_icon_sections(tray_available, cx));
        if tray.start_in_tray {
            body.push(
                this.appearance_toggle_section(
                    cx,
                    tr(keys::Q_PREFS_START_IN_TRAY),
                    tr(keys::Q_PREFS_OPEN_QUILL_FROM_ITS_TRAY),
                    Switch::new("general-start-in-tray")
                        .checked(this.appearance.start_in_tray)
                        .accessibility_label(tr(keys::Q_PREFS_START_QUILL_IN_THE_SYSTEM))
                        .on_click(cx.listener(|this, &on, _, cx| {
                            this.set_appearance(cx, |a| a.start_in_tray = on)
                        }))
                        .into_any_element(),
                ),
            );
        }
        if tray.run_in_background {
            body.push(this.appearance_close_behavior_section(cx));
        }
        body.extend(this.native_frame_section(super::window_frame::supported(), cx));
        body
    }

    /// tdesktop "Use system window frame" (Linux and Windows only);
    /// `supported` is a parameter so tests can show it on any platform.
    pub(crate) fn native_frame_section(
        &self,
        supported: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        supported.then(|| {
            self.appearance_toggle_section(
                cx,
                tr(keys::SETTINGS_NATIVE_FRAME),
                tr(keys::Q_PREFS_THE_SYSTEM_DRAWS_THE_TITLE),
                Switch::new("general-native-window-frame")
                    .checked(self.appearance.native_window_frame)
                    .accessibility_label(tr(keys::Q_PREFS_USE_THE_SYSTEM_WINDOW_FRAME))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_appearance(cx, |a| a.native_window_frame = on)
                    }))
                    .into_any_element(),
            )
        })
    }

    /// tdesktop "When the window is closed": run in the background or quit.
    fn appearance_close_behavior_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let control = RadioGroup::vertical("appearance-close-behavior")
            .selected_index(Some(usize::from(!self.appearance.minimize_to_tray)))
            .children([
                Radio::new("appearance-close-background")
                    .label(tr(keys::SETTINGS_RUN_IN_BACKGROUND)),
                Radio::new("appearance-close-quit").label(tr(keys::Q_PREFS_QUIT_QUILL)),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| a.minimize_to_tray = ix == 0);
            }));
        let hint = tr(keys::Q_PREFS_QUILL_KEEPS_RUNNING_AND_REOPENS);
        self.appearance_section(
            cx,
            tr(keys::Q_PREFS_WHEN_THE_WINDOW_IS_CLOSED),
            hint,
            control.into_any_element(),
        )
    }

    pub(in crate::ui) fn appearance_auto_night_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const MODES: [AutoNight; 3] = [AutoNight::Off, AutoNight::System, AutoNight::Scheduled];
        const LABELS: [&str; 3] = ["Off", "System", "Scheduled"];
        let current = self.appearance.auto_night;
        // kit Phase 6 style: one kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-night")
            .selected_index(MODES.iter().position(|m| *m == current))
            .children(
                LABELS
                    .iter()
                    .map(|label| Radio::new(format!("appearance-night-{label}")).label(*label)),
            )
            .on_click(cx.listener(|this, &ix, _, cx| {
                if let Some(&mode) = MODES.get(ix) {
                    this.set_appearance(cx, |a| a.auto_night = mode);
                }
            }));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.appearance_section(
                cx,
                tr(keys::Q_PREFS_AUTO_NIGHT),
                tr(keys::Q_PREFS_AUTOMATICALLY_SWITCH_TO_THE_DARK),
                control.into_any_element(),
            ));
        if current == AutoNight::Scheduled {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_sm().child(tr(keys::Q_PREFS_FROM)))
                    .child(self.appearance_time_stepper(
                        "night-start",
                        self.appearance.night_start_minutes,
                        true,
                        cx,
                    ))
                    .child(div().text_sm().child(tr(keys::Q_PREFS_TO)))
                    .child(self.appearance_time_stepper(
                        "night-end",
                        self.appearance.night_end_minutes,
                        false,
                        cx,
                    )),
            );
        }
        if current == AutoNight::System {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_FOLLOWS_THE_OS_LIGHT_DARK)),
            );
        }
        body.into_any_element()
    }
}

mod appearance_keybindings_section;
mod new_accent_picker;
mod send_to_menu_section;
mod telegram_wallpapers;

#[cfg(test)]
mod tests {
    use super::{accent_rgb_from, limit_custom_accent};
    use gpui_kit::{Hsla, rgb};

    #[test]
    fn custom_accent_lightness_follows_tdesktop_limits() {
        let pale = Hsla::from(rgb(0xf0f4ff));
        let light = limit_custom_accent(pale, false);
        assert!((light.l - 160. / 255.).abs() < 1e-6);
        assert_eq!(limit_custom_accent(pale, true).l, pale.l);

        let deep = Hsla::from(rgb(0x0a1020));
        assert!((limit_custom_accent(deep, true).l - 64. / 255.).abs() < 1e-6);
        assert_eq!(limit_custom_accent(deep, false).l, deep.l);

        let translucent = Hsla { a: 0.3, ..deep };
        assert_eq!(limit_custom_accent(translucent, false).a, 1.);
    }

    #[test]
    fn accent_rgb_round_trips_and_never_means_default() {
        for value in [0x2f81f7, 0x3fb950, 0xf85149, 0xffffff, 0x123456] {
            assert_eq!(accent_rgb_from(Hsla::from(rgb(value))), value);
        }
        assert_eq!(accent_rgb_from(Hsla::from(rgb(0x000000))), 0x000001);
    }
}
