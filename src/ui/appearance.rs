//! Settings → Appearance slice: the dialog plus the load/save/apply
//! funnel. Everything here is client-side — theme, auto-night, accent,
//! wallpaper, message font size and bubble style live in
//! `appearance_prefs.json` (see `quill::settings::AppearancePrefs`);
//! there is no TDLib setting for any of it. The exception is the
//! Language section (slice parity:settings-language): its tag is the
//! `system_language_code` sent in `setTdlibParameters`, persisted in
//! `language_prefs.json` and applied on restart.

use super::QuillApp;
use super::chat_theme::{set_high_contrast, set_theme_mode};
use super::synthetic::BubbleLook;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState, ColorSelect};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::{ActiveTheme, Theme, ThemeMode};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::AccountKey;
use quill::settings::{
    AccountPaths, AppearancePrefs, AutoNight, ChatPrefs, DEFAULT_LANGUAGE_CODE, LanguagePrefs,
    SUPPORTED_LANGUAGES, ThemeChoice, clamp_font_size, load_appearance_prefs, load_chat_prefs,
    local_minutes_since_midnight, night_active, safe_app_root, save_appearance_prefs,
    save_chat_prefs,
};
use std::cell::RefCell;
use std::rc::Rc;
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

const ACCENT_PRESETS: &[(u32, &str)] = &[
    (0x2f81f7, "Blue"),
    (0x3fb950, "Green"),
    (0x8957e5, "Purple"),
    (0xf778ba, "Pink"),
    (0xd29922, "Orange"),
    (0x39c5cf, "Teal"),
    (0xf85149, "Red"),
];

/// Wallpaper presets (0xRRGGBB); the "Default" chip keeps the theme
/// background.
const WALLPAPER_PRESETS: &[(u32, &str)] = &[
    (0x0e1621, "Dark blue"),
    (0x17212b, "Slate"),
    (0x1c2b33, "Teal"),
    (0x2a1e2e, "Plum"),
    (0x141414, "Near black"),
];

impl QuillApp {
    pub(crate) fn minimize_to_tray(&self) -> bool {
        self.appearance.minimize_to_tray
    }
    pub(super) fn close_appearance(&mut self) {
        super::keybindings::close_appearance_capture(
            &mut self.appearance_open,
            &mut self.keybinding_capture,
            &mut self.keybinding_error,
        );
    }

    pub(super) fn keybinding_capture_active(&mut self) -> bool {
        super::keybindings::capture_active(
            self.appearance_open,
            &mut self.keybinding_capture,
            &mut self.keybinding_error,
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
        load_appearance_prefs(&Self::appearance_paths())
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
        let dark = if hc {
            true
        } else {
            match self.appearance.auto_night {
                AutoNight::Off => self.appearance.theme == ThemeChoice::Dark,
                // On Linux without a desktop portal this reports Light; the
                // mode is still honest — it follows what the platform says.
                // Matches gpui-component's own `From<WindowAppearance>` map
                // (Dark | VibrantDark → dark).
                AutoNight::System => matches!(
                    cx.window_appearance(),
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                ),
                AutoNight::Scheduled => night_active(
                    self.appearance.night_start_minutes,
                    self.appearance.night_end_minutes,
                    local_minutes_since_midnight(),
                ),
            }
        };
        let mode = if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        // Power saving: the switches gate animations as they are drawn; the
        // interface-animations one also folds into GPUI's reduced motion.
        quill::power_saving::set(self.appearance.power_saving);
        cx.set_reduce_motion(quill::power_saving::reduce_motion_now());
        let accent = quill::system_accent::effective(
            self.appearance.accent_rgb,
            self.appearance.system_accent,
            self.system_accent,
        );
        let scale = self.appearance.interface_scale_pct;
        let family = super::appearance_power::interface_font(&self.appearance.font_family, cx);
        let applied = (mode, accent, hc, scale, family.clone());
        if self.appearance_applied.as_ref() == Some(&applied) {
            return;
        }
        set_theme_mode(mode, None, cx);
        set_high_contrast(hc);
        let primary = if accent == 0 {
            Hsla::from(super::chat_theme::accent_strong())
        } else {
            Hsla::from(rgb(accent))
        };
        // `Theme::update` re-derives the renderable tokens from `colors`,
        // re-projects the Base layer and refreshes windows. Mutating
        // `global_mut` alone would leave primary buttons on the old accent.
        Theme::update(cx, |theme| {
            // Primary buttons draw from their own tokens: point them at the
            // accent too, or Send and other primary actions stay neutral.
            let colors = &mut theme.colors;
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
        self.appearance_applied = Some(applied);
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
        self.appearance.font_size_px = clamp_font_size(self.appearance.font_size_px);
        if let Err(err) = save_appearance_prefs(&Self::appearance_paths(), &self.appearance) {
            self.status_note = format!("Couldn't save appearance settings: {err}");
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
            self.status_note = format!("Couldn't save chat settings: {err}");
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

    /// Slice parity:settings-language: update the app language pref in the
    /// session and persist it to the account dir (via
    /// `ConnectDriver::save_language_prefs`). TDLib reads the tag once at
    /// startup, so the UI notes that the change applies after restart.
    pub(crate) fn set_language_pref(&mut self, code: &str, cx: &mut Context<Self>) {
        let prefs = LanguagePrefs {
            system_language_code: code.to_string(),
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.language_prefs = prefs;
            if let Err(err) = live.driver.save_language_prefs() {
                self.status_note = format!("couldn't save language setting: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.language_prefs = prefs;
            self.status_note = "demo: language setting is not saved".into();
        }
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
                .outgoing_fill,
        }
    }

    /// kit Phase 2 (redo) pattern: the Appearance dialog hosted in a kit
    /// `Dialog` via `window.open_dialog` (see `QuillShell::sync_kit_dialogs`).
    /// Esc / backdrop / ✕ clear state via `on_close`. Every control applies
    /// live through `set_appearance`, so there is no OK/apply step — the
    /// footer is a single Close button.
    pub(crate) fn build_appearance_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Appearance, |this, _, cx| {
                this.close_appearance();
                cx.notify();
            });
        app.update(cx, |this, cx| {
            if !this.system_accent_probed {
                this.refresh_system_accent(cx);
            }
            let mut body = div().flex().flex_col().gap_3();
            if this.translate_ui.settings_only {
                body = body.child(this.translate_settings_section(cx));
            } else if this.window_settings_screenshot {
                for section in this.window_behavior_sections(true, cx) {
                    body = body.child(section);
                }
            } else if this.appearance_power_screenshot {
                body = body.child(this.appearance_accent_section(cx));
                body = body.child(this.appearance_font_family_section(cx));
                body = body.child(this.appearance_power_section(cx));
            } else if this.keybindings_screenshot {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Keyboard shortcuts. Changes apply immediately and are saved on this device.",
                        ),
                );
                body = body.child(this.appearance_keybindings_section(cx));
            } else {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Customize how Quill looks and feels. Changes are saved on this device.",
                        ),
                );
                body = body.child(this.appearance_theme_section(cx));
                body = body.child(this.appearance_spellcheck_section(cx));
                body = body.child(this.appearance_suggest_emoji_section(cx));
                body = body.child(this.appearance_auto_night_section(cx));
                body = body.child(this.appearance_accent_section(cx));
                body = body.child(this.appearance_scale_section(cx));
                this.ensure_wallpapers_loaded(cx);
                body = body.child(this.appearance_wallpaper_section(cx));
                body = body.child(this.appearance_telegram_wallpapers_section(cx));
                body = body.child(this.appearance_font_section(cx));
                body = body.child(this.appearance_font_family_section(cx));
                body = body.child(this.appearance_bubble_section(cx));
                body = body.child(this.appearance_chat_list_section(cx));
                body = body.child(this.appearance_power_section(cx));
                body = body.child(this.appearance_send_key_section(cx));
                // Batch 7: Show Translate Button / Translate Entire Chats /
                // Do Not Translate.
                body = body.child(this.translate_settings_section(cx));
                // Slice parity:settings-language: the app language picker
                // (the tag TDLib gets in `setTdlibParameters`).
                body = body.child(this.appearance_language_section(cx));
                body = body.child(this.general_autostart_section(cx));
                body = body.child(this.general_link_handler_section(cx));
                body = body.child(this.update_settings_section(cx));
                body = body.child(this.about_settings_section(cx));
                for section in this.window_behavior_sections(quill::tray::tray_available(), cx) {
                    body = body.child(section);
                }
                body = body.child(this.appearance_keybindings_section(cx));
            }
            let footer = div().flex().justify_end().child(
                Button::new("close-appearance")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_appearance();
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::Appearance, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Appearance"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
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
                if selected { ", selected" } else { "" }
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
    pub(crate) fn window_behavior_sections(
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
        if tray.show_tray_icon {
            body.push(
                this.appearance_section(
                    cx,
                    "Show tray icon",
                    "Keep Quill in the system tray with the unread count.",
                    Switch::new("general-show-tray-icon")
                        .checked(this.appearance.show_tray_icon)
                        .accessibility_label("Show the Quill tray icon")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            this.set_appearance(cx, |a| a.show_tray_icon = on)
                        }))
                        .into_any_element(),
                ),
            );
        }
        if tray.start_in_tray {
            body.push(
                this.appearance_section(
                    cx,
                    "Start in tray",
                    "Open Quill from its tray menu when needed.",
                    Switch::new("general-start-in-tray")
                        .checked(this.appearance.start_in_tray)
                        .accessibility_label("Start Quill in the system tray")
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
        #[cfg(target_os = "macos")]
        {
            body.push(
                this.appearance_section(
                    cx,
                    "Warn before quitting",
                    "Hold \u{2318}Q to quit instead of quitting on the first press.",
                    Switch::new("general-mac-warn-before-quit")
                        .checked(this.appearance.mac_warn_before_quit)
                        .accessibility_label("Warn before quitting with Command Q")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            this.set_appearance(cx, |a| a.mac_warn_before_quit = on)
                        }))
                        .into_any_element(),
                ),
            );
        }
        body
    }

    /// tdesktop "When the window is closed": run in the background or quit.
    fn appearance_close_behavior_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let control = RadioGroup::vertical("appearance-close-behavior")
            .selected_index(Some(usize::from(!self.appearance.minimize_to_tray)))
            .children([
                Radio::new("appearance-close-background").label("Run in the background"),
                Radio::new("appearance-close-quit").label("Quit Quill"),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| a.minimize_to_tray = ix == 0);
            }));
        let hint = if cfg!(target_os = "macos") {
            "Quill keeps running and reopens from the tray icon."
        } else {
            "The window minimizes and reopens from the tray icon."
        };
        self.appearance_section(
            cx,
            "When the window is closed",
            hint,
            control.into_any_element(),
        )
    }

    fn appearance_theme_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = Some(match self.appearance.theme {
            ThemeChoice::Light => 0,
            ThemeChoice::Dark => 1,
            ThemeChoice::HighContrast => 2,
        });
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-theme")
            .selected_index(selected)
            .children([
                Radio::new("appearance-theme-light").label("☀️ Light"),
                Radio::new("appearance-theme-dark").label("🌙 Dark"),
                Radio::new("appearance-theme-hc").label("◐ High contrast"),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| {
                    a.theme = match ix {
                        0 => ThemeChoice::Light,
                        1 => ThemeChoice::Dark,
                        _ => ThemeChoice::HighContrast,
                    }
                });
            }));
        let hint = if self.appearance.auto_night == AutoNight::Off {
            "Applies to the whole app immediately."
        } else {
            "Auto-night is on — this applies while night mode is inactive."
        };
        self.appearance_section(cx, "Theme", hint, control.into_any_element())
    }

    fn appearance_auto_night_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
                let mode = MODES[ix];
                this.set_appearance(cx, |a| a.auto_night = mode);
            }));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.appearance_section(
                cx,
                "Auto-night",
                "Automatically switch to the dark theme at night.",
                control.into_any_element(),
            ));
        if current == AutoNight::Scheduled {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_sm().child("From"))
                    .child(self.appearance_time_stepper(
                        "night-start",
                        self.appearance.night_start_minutes,
                        true,
                        cx,
                    ))
                    .child(div().text_sm().child("To"))
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
                    .child("Follows the OS light/dark setting."),
            );
        }
        body.into_any_element()
    }

    fn appearance_accent_section(&self, cx: &mut Context<Self>) -> AnyElement {
        // The system color counts as chosen only while the OS reports one.
        let system = self.system_accent.filter(|_| self.appearance.system_accent);
        let current = if system.is_some() {
            u32::MAX
        } else {
            self.appearance.accent_rgb
        };
        let mut row = div()
            .flex()
            .gap_2()
            .items_center()
            .child(self.appearance_chip(
                "appearance-accent-default",
                "Default",
                current == 0,
                cx,
                |this, cx| {
                    this.set_appearance(cx, |a| {
                        a.accent_rgb = 0;
                        a.system_accent = false;
                    })
                },
            ));
        // tdesktop's "System accent color" (settings_chat.cpp), shown only
        // where the OS reports an accent.
        if let Some(color) = self.system_accent {
            row = row.child(self.appearance_swatch(
                "appearance-accent-system",
                color,
                "System",
                system.is_some(),
                cx,
                |this, cx| this.set_appearance(cx, |a| a.system_accent = true),
            ));
        }
        for &(color, name) in ACCENT_PRESETS {
            row = row.child(self.appearance_swatch(
                format!("appearance-accent-{color:06x}"),
                color,
                name,
                current == color,
                cx,
                move |this, cx| {
                    this.set_appearance(cx, |a| {
                        a.accent_rgb = color;
                        a.system_accent = false;
                    })
                },
            ));
        }
        // tdesktop's last accent circle opens a free-form color editor;
        // here it is the kit's framed color field, with the presets
        // featured at the top of its palette.
        let custom = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_sm().child("Custom"))
            .child(
                ColorSelect::new(&self.accent_picker)
                    .featured_colors(
                        ACCENT_PRESETS
                            .iter()
                            .map(|&(color, _)| Hsla::from(rgb(color)))
                            .collect(),
                    )
                    .accessibility_label("Custom accent color")
                    .w(px(180.)),
            );
        self.appearance_section(
            cx,
            "Accent color",
            if self.system_accent.is_some() {
                "Highlights, selections and links across the app. System follows \
                 your operating system's accent. Custom colors are kept light \
                 enough on dark themes and dark enough on light ones."
            } else {
                "Highlights, selections and links across the app. Custom colors are \
                 kept light enough on dark themes and dark enough on light ones."
            },
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(row)
                .child(custom)
                .into_any_element(),
        )
    }

    /// The custom accent field's state, showing `accent_rgb` (or the
    /// theme's accent when that is the default). A committed color is
    /// limited like tdesktop's (`limit_custom_accent`) and becomes the
    /// accent.
    pub(super) fn new_accent_picker(
        accent_rgb: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        let initial = if accent_rgb == 0 {
            Hsla::from(super::chat_theme::accent_strong())
        } else {
            Hsla::from(rgb(accent_rgb))
        };
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(initial));
        cx.subscribe_in(
            &picker,
            window,
            |this, picker, event: &ColorPickerEvent, window, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                let limited = limit_custom_accent(*color, cx.theme().is_dark());
                if limited != *color {
                    picker.update(cx, |picker, cx| picker.set_value(limited, window, cx));
                }
                let value = accent_rgb_from(limited);
                this.set_appearance(cx, |a| {
                    a.accent_rgb = value;
                    a.system_accent = false;
                });
            },
        )
        .detach();
        picker
    }

    /// tdesktop's "Interface scale" (Settings > Chat settings). GPUI has no
    /// window-wide scale factor, so this scales the rem size: text and
    /// rem-based spacing follow, fixed-pixel widths (avatars, chat rows) do
    /// not — the hint says so.
    /// Telegram's wallpapers (`getInstalledBackgrounds`): color and gradient
    /// fills paint exactly, photos show once downloaded, patterns show their
    /// fill (the pattern layer, blur and motion are not drawn).
    fn appearance_telegram_wallpapers_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let dark = cx.theme().is_dark();
        let session = self.session();
        let list = session.and_then(|s| s.installed_backgrounds.clone());
        let default_id = session
            .and_then(|s| s.default_backgrounds.get(&dark))
            .map(|b| b.id)
            .filter(|_| self.appearance.telegram_wallpaper);
        let error = session.and_then(|s| s.background_error.clone());
        let muted = cx.theme().muted_foreground;
        let mut grid = div().flex().flex_wrap().gap_2();
        match &list {
            None => {
                grid = grid.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading wallpapers…"),
                );
            }
            Some(list) if list.is_empty() => {
                grid = grid.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("No wallpapers installed on this account."),
                );
            }
            Some(list) => {
                for background in list {
                    let id = background.id;
                    let selected = default_id == Some(id);
                    let mut tile = self
                        .wallpaper_tile(
                            ("appearance-tg-wallpaper", id as u64),
                            background,
                            selected,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_telegram_wallpaper(id, cx)
                        }));
                    tile = tile.child(
                        div().absolute().top_0().right_0().child(
                            Button::new(("appearance-tg-wallpaper-remove", id as u64))
                                .icon(gpui_kit::assets::IconName::X)
                                .ghost()
                                .xsmall()
                                .tooltip("Remove from installed wallpapers")
                                .accessibility_label("Remove wallpaper")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_telegram_wallpaper(id, cx);
                                })),
                        ),
                    );
                    grid = grid.child(tile);
                }
            }
        }
        let mut body = div().flex().flex_col().gap_2().child(grid).child(
            div().flex().child(
                Button::new("appearance-tg-wallpaper-file")
                    .label("From file…")
                    .small()
                    .tooltip("Use a JPEG, PNG or WebP image as your wallpaper")
                    .on_click(cx.listener(|this, _, _, cx| this.choose_wallpaper_file(cx))),
            ),
        );
        if let Some(error) = error {
            body = body.child(
                div()
                    .id("appearance-tg-wallpaper-error")
                    .text_xs()
                    .text_color(super::danger_dark())
                    .child(format!("Couldn’t update wallpapers: {error}")),
            );
        }
        self.appearance_section(
            cx,
            "Telegram wallpapers",
            "Your installed wallpapers: colors, gradients, patterns and photos. Chat wallpapers and themes are set per chat from its info panel. Blur and motion are not drawn.",
            body.into_any_element(),
        )
    }

    /// Make an installed wallpaper the account's default for the current
    /// theme and show it.
    fn choose_telegram_wallpaper(&mut self, background_id: i64, cx: &mut Context<Self>) {
        let dark = cx.theme().is_dark();
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.set_default_background(background_id, dark) {
                    self.status_note = format!("could not set the wallpaper: {err:?}");
                    return;
                }
            }
            None => {
                // Screenshot demo: apply locally.
                if let Some(session) = self.demo_session.as_mut()
                    && let Some(bg) = session
                        .installed_backgrounds
                        .iter()
                        .flatten()
                        .find(|b| b.id == background_id)
                        .cloned()
                {
                    session.default_backgrounds.insert(dark, bg);
                }
            }
        }
        self.set_appearance(cx, |a| {
            a.telegram_wallpaper = true;
            a.wallpaper_rgb = None;
        });
    }

    fn remove_telegram_wallpaper(&mut self, background_id: i64, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.remove_installed_background(background_id) {
                    self.status_note = format!("could not remove the wallpaper: {err:?}");
                }
            }
            None => {
                if let Some(session) = self.demo_session.as_mut()
                    && let Some(list) = session.installed_backgrounds.as_mut()
                {
                    list.retain(|b| b.id != background_id);
                }
            }
        }
        cx.notify();
    }

    /// Fetch the installed wallpapers when Appearance opens, and start the
    /// photo downloads.
    pub(crate) fn ensure_wallpapers_loaded(&mut self, cx: &mut Context<Self>) {
        let dark = cx.theme().is_dark();
        if let Some(live) = self.live.as_mut() {
            if live.driver.session.installed_backgrounds.is_none() {
                let _ = live.driver.fetch_installed_backgrounds(dark);
            }
            live.driver.download_background_files();
        }
    }

    fn appearance_scale_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.appearance.interface_scale_pct;
        let mut row = div().flex().flex_wrap().gap_2().items_center();
        for pct in quill::settings::INTERFACE_SCALE_CHOICES {
            row = row.child(self.appearance_chip(
                format!("appearance-scale-{pct}"),
                format!("{pct}%"),
                current == pct,
                cx,
                move |this, cx| this.set_appearance(cx, |a| a.interface_scale_pct = pct),
            ));
        }
        self.appearance_section(
            cx,
            "Interface scale",
            "Scales the whole interface, applied right away.",
            row.into_any_element(),
        )
    }

    fn appearance_wallpaper_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .appearance
            .wallpaper_rgb
            .filter(|_| !self.appearance.telegram_wallpaper);
        let mut row = div()
            .flex()
            .gap_2()
            .items_center()
            .child(self.appearance_chip(
                "appearance-wallpaper-default",
                "Default",
                current.is_none(),
                cx,
                |this, cx| {
                    this.set_appearance(cx, |a| {
                        a.wallpaper_rgb = None;
                        a.telegram_wallpaper = false;
                    })
                },
            ));
        for &(color, name) in WALLPAPER_PRESETS {
            row = row.child(self.appearance_swatch(
                format!("appearance-wallpaper-{color:06x}"),
                color,
                name,
                current == Some(color),
                cx,
                move |this, cx| {
                    this.set_appearance(cx, |a| {
                        a.wallpaper_rgb = Some(color);
                        a.telegram_wallpaper = false;
                    })
                },
            ));
        }
        self.appearance_section(
            cx,
            "Chat wallpaper",
            "Solid color behind the message list — dark colors pair best with the dark theme.",
            row.into_any_element(),
        )
    }

    fn appearance_font_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let size = self.appearance.font_size_px;
        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("appearance-font-down")
                    .label("A−")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.bump_font_size(cx, -1))),
            )
            .child(
                div()
                    .text_sm()
                    .min_w(px(56.))
                    .text_center()
                    .child(format!("{size}px")),
            )
            .child(
                Button::new("appearance-font-up")
                    .label("A+")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.bump_font_size(cx, 1))),
            );
        self.appearance_section(
            cx,
            "Message text size",
            "Chat message text, 12–20 px.",
            control.into_any_element(),
        )
    }

    fn appearance_bubble_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let bubbles = self.appearance.bubbles;
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-style")
            .selected_index(Some(if bubbles { 0 } else { 1 }))
            .children([
                Radio::new("appearance-style-bubbles").label("💬 Bubbles"),
                Radio::new("appearance-style-plain").label("📄 Plain"),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| a.bubbles = ix == 0);
            }));
        self.appearance_section(
            cx,
            "Chat style",
            "Bubbles or plain rows without bubble backgrounds.",
            control.into_any_element(),
        )
    }

    /// A labeled toggle row: title + hint on the left, kit `Switch` on
    /// the right (the data-storage dialog pattern).
    pub(super) fn appearance_switch_row(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        title: &str,
        hint: &str,
        checked: bool,
        on_change: impl Fn(&mut AppearancePrefs, bool) + 'static,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
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
            .child(
                Switch::new(id)
                    .checked(checked)
                    .accessibility_label(title)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.set_appearance(cx, |a| on_change(a, on));
                    })),
            )
            .into_any_element()
    }

    /// Slice chatlist-list-style: chat-list row style — preview line
    /// count (kit RadioGroup, like the theme/chat-style sections), media
    /// icons and formatted preview text (kit Switch rows).
    fn appearance_chat_list_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let lines = self.appearance.preview_lines;
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-chat-list-lines")
            .selected_index(Some(if lines >= 3 { 1 } else { 0 }))
            .children([
                Radio::new("appearance-chat-list-lines-2").label("2 lines"),
                Radio::new("appearance-chat-list-lines-3").label("3 lines"),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| {
                    a.preview_lines = if ix == 0 { 2 } else { 3 };
                });
            }));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.appearance_section(
            cx,
            "Chat list rows",
            "Two lines shows the title plus the message preview; three lines adds the sender line.",
            control.into_any_element(),
        ));
        body = body.child(self.appearance_switch_row(
            cx,
            "appearance-chat-list-media-icons",
            "Media icons",
            "Show a photo, video or file icon before the preview text.",
            self.appearance.chat_list_media_icons,
            |a, on| a.chat_list_media_icons = on,
        ));
        body = body.child(self.appearance_switch_row(
            cx,
            "appearance-chat-list-rich-preview",
            "Formatted preview text",
            "Show bold, italic and other formatting in the chat-list preview.",
            self.appearance.chat_list_rich_preview,
            |a, on| a.chat_list_rich_preview = on,
        ));
        body = body.child(self.appearance_swipe_action_section(cx));
        body.into_any_element()
    }

    /// tdesktop's "Chat list quick action" (Settings > Chats): what a
    /// horizontal trackpad swipe on a chat row does. Disabled by default.
    fn appearance_swipe_action_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::chat_swipe::SwipeAction;
        let current = self.appearance.swipe_action;
        let selected = SwipeAction::ALL.iter().position(|a| *a == current);
        let control = RadioGroup::vertical("appearance-swipe-action")
            .selected_index(selected)
            .children(SwipeAction::ALL.iter().map(|action| {
                Radio::new(("appearance-swipe-action", *action as usize))
                    .label(action.settings_label())
            }))
            .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                if let Some(action) = SwipeAction::ALL.get(ix).copied() {
                    this.set_appearance(cx, |a| a.swipe_action = action);
                }
            }));
        self.appearance_section(
            cx,
            "Chat list quick action",
            "Middle-click a chat, or swipe it left with two fingers on a trackpad, \
             to run this action. A swipe runs it once you pass the threshold and \
             lift your fingers.",
            control.into_any_element(),
        )
    }

    /// Send-key mode section (parity:settings-enter-send,
    /// parity:settings-ctrlenter-send): which keystroke sends a chat
    /// message. Lives in the Appearance dialog — Quill has no separate
    /// Chat Settings screen yet.
    /// `parity:platform-autostart` — "Launch at login" switch. OS-level
    /// (XDG Autostart on Linux, LaunchAgents on macOS); unsupported
    /// platforms render an explanatory line instead of the switch.
    fn general_autostart_section(&self, cx: &mut Context<Self>) -> AnyElement {
        if !quill::autostart::supported() {
            return self.appearance_section(
                cx,
                "Launch at login",
                "Autostart is not available on this platform.",
                div().into_any_element(),
            );
        }
        let on = quill::autostart::is_enabled();
        let control = Switch::new("general-autostart-switch")
            .checked(on)
            .accessibility_label("Launch Quill at login")
            .on_click(cx.listener(|this, &on, _, cx| {
                if let Err(err) = quill::autostart::set_enabled(on) {
                    this.status_note = err.to_string();
                }
                cx.notify();
            }));
        self.appearance_section(
            cx,
            "Launch at login",
            "Start Quill automatically when you sign in to this device.",
            control.into_any_element(),
        )
    }

    /// "Open Telegram links with Quill": opt-in default handler for `tg:`
    /// links (`quill::link_handler`). Quill never claims it on its own.
    /// Windows/Linux get a switch; macOS cannot hand the scheme back to
    /// another app, so it gets a one-way "Make default" button.
    fn general_link_handler_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::link_handler::{self, ControlKind, LinkHandlerState};
        let state = link_handler::state();
        let ours = state == LinkHandlerState::Ours;
        let unavailable = matches!(state, LinkHandlerState::Unavailable(_));
        let hint = if unavailable {
            link_handler::status_text(&state)
        } else {
            format!(
                "{} {}",
                link_handler::status_text(&state),
                link_handler::turn_off_note()
            )
        };
        let control: AnyElement = match link_handler::control_kind() {
            ControlKind::Button if ours => div()
                .text_sm()
                .child("Quill is the default")
                .into_any_element(),
            ControlKind::Button => Button::new("general-link-handler-button")
                .label("Make Quill the default")
                .disabled(unavailable)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(err) = link_handler::make_default() {
                        this.status_note = err;
                    }
                    // The system confirmation is asynchronous: re-read the
                    // handler a few times so the status catches up.
                    for secs in [1u64, 3, 8] {
                        cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_secs(secs))
                                .await;
                            link_handler::invalidate();
                            let _ = this.update(cx, |_, cx| cx.notify());
                        })
                        .detach();
                    }
                    cx.notify();
                }))
                .into_any_element(),
            ControlKind::Switch => Switch::new("general-link-handler-switch")
                .checked(ours)
                .disabled(unavailable)
                .accessibility_label("Open Telegram links with Quill")
                .on_click(cx.listener(|this, &on, _, cx| {
                    let result = if on {
                        link_handler::make_default()
                    } else {
                        link_handler::release()
                    };
                    if let Err(err) = result {
                        this.status_note = err;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        };
        self.appearance_section(cx, "Open Telegram links with Quill", &hint, control)
    }

    fn appearance_send_key_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::composer::SendKeyMode;
        let current = self.chat_prefs.send_key_mode;
        let control = RadioGroup::horizontal("appearance-send-key")
            .selected_index(Some(if current == SendKeyMode::Enter { 0 } else { 1 }))
            .children([
                Radio::new("appearance-send-key-enter").label("⏎ Enter"),
                Radio::new("appearance-send-key-ctrlenter").label(if cfg!(target_os = "macos") {
                    "⌘⏎ Cmd+Enter"
                } else {
                    "⌃⏎ Ctrl+Enter"
                }),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_chat_prefs(cx, |c| {
                    c.send_key_mode = if ix == 0 {
                        SendKeyMode::Enter
                    } else {
                        SendKeyMode::CtrlEnter
                    };
                });
            }));
        self.appearance_section(
            cx,
            "Send messages with",
            "Enter sends, or Enter inserts a newline and Ctrl/Cmd+Enter sends.",
            control.into_any_element(),
        )
    }

    /// parity:platform-spellcheck: the spellcheck toggle (same row
    /// pattern as `appearance_switch_row`, but wired to ChatPrefs), plus
    /// tdesktop's spell-checker language list where Quill owns the
    /// dictionaries (Hunspell on Linux, ISpellChecker languages on
    /// Windows; macOS lets the system pick).
    fn appearance_spellcheck_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let checked = self.chat_prefs.spellcheck_enabled;
        let hint = self.spell.info.hint();
        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child("Check spelling"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(hint),
                    ),
            )
            .child(
                Switch::new("appearance-spellcheck")
                    .checked(checked)
                    .accessibility_label("Check spelling")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.spellcheck_enabled = on);
                        let text = this.composer.read(cx).value().to_string();
                        this.sync_spellcheck(&text, cx);
                    })),
            );
        let mut control = div().flex().flex_col().gap_2().child(row);
        if checked && !self.spell.info.available.is_empty() {
            let active = self.spell.info.active.clone();
            let mut chips = div().flex().flex_wrap().gap_2().child(self.appearance_chip(
                "spell-lang-auto",
                "Automatic",
                self.spell.info.chosen.is_empty(),
                cx,
                |this, cx| this.set_spell_languages(Vec::new(), cx),
            ));
            for code in self.spell.info.available.clone() {
                let on = active.contains(&code);
                let base = active.clone();
                let toggled = code.clone();
                chips = chips.child(self.appearance_chip(
                    SharedString::from(format!("spell-lang-{code}")),
                    code.clone(),
                    on && !self.spell.info.chosen.is_empty(),
                    cx,
                    move |this, cx| {
                        let mut next = if this.spell.info.chosen.is_empty() {
                            base.clone()
                        } else {
                            this.spell.info.chosen.clone()
                        };
                        if let Some(i) = next.iter().position(|c| *c == toggled) {
                            next.remove(i);
                        } else {
                            next.push(toggled.clone());
                        }
                        this.set_spell_languages(next, cx);
                    },
                ));
            }
            control = control.child(chips);
        }
        if checked && self.dictionary_manager_available() {
            control = control.child(self.dictionary_manager_section(cx));
        }
        self.appearance_section(cx, "Spelling", "", control.into_any_element())
    }

    /// tdesktop "Suggest emoji replacements" (`suggestEmoji`, default on):
    /// the `:name` emoji popup in the composer.
    fn appearance_suggest_emoji_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let suggest = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child("Suggest emoji replacements"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Type : and a name in the composer to pick a matching emoji."),
                    ),
            )
            .child(
                Switch::new("appearance-suggest-emoji")
                    .checked(self.chat_prefs.suggest_emoji)
                    .accessibility_label("Suggest emoji replacements")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.suggest_emoji = on);
                        this.sync_suggest_menu(cx);
                    })),
            );
        // tdesktop "Replace emoji automatically" (`replaceEmoji`).
        let replace = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child("Replace emoji automatically"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Turn :-) and <3 into emoji as you type."),
                    ),
            )
            .child(
                Switch::new("appearance-replace-emoji")
                    .checked(self.chat_prefs.replace_emoji)
                    .accessibility_label("Replace emoji automatically")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.replace_emoji = on);
                    })),
            );
        let control = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(suggest)
            .child(replace)
            .into_any_element();
        // tdesktop Chat settings "Large emoji": one to three emoji alone in
        // a message show as big glyphs (the history renderer reads the same
        // `MediaPrefs::big_emoji`).
        let large = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child("Large emoji"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show a message of one to three emoji as big glyphs."),
                    ),
            )
            .child(
                Switch::new("appearance-large-emoji")
                    .checked(self.session().is_none_or(|s| s.media_prefs.big_emoji))
                    .accessibility_label("Large emoji")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_media_pref(|prefs| prefs.big_emoji = on, cx)
                    })),
            );
        let control = div().flex().flex_col().gap_2().child(large).child(control);
        self.appearance_section(cx, "Emoji", "", control.into_any_element())
    }

    /// Slice parity:settings-language: the app language picker (the IETF
    /// tag TDLib gets in `setTdlibParameters`; it was hardcoded "en"
    /// before this slice). TDLib reads parameters once at startup, so
    /// the chosen language applies after restart — no runtime
    /// application is attempted (see `settings::LanguagePrefs` for why
    /// `setOption("language_pack_id")` can't do it). The app's own
    /// strings stay English; this only changes the tag reported to
    /// Telegram.
    fn appearance_language_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .session()
            .map(|s| s.language_prefs.system_language_code.as_str())
            .unwrap_or(DEFAULT_LANGUAGE_CODE);
        let control = RadioGroup::vertical("appearance-language")
            .selected_index(
                SUPPORTED_LANGUAGES
                    .iter()
                    .position(|(code, _)| *code == current),
            )
            .children(SUPPORTED_LANGUAGES.iter().map(|(code, name)| {
                Radio::new(format!("appearance-language-{code}")).label(format!("{name} ({code})"))
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.set_language_pref(SUPPORTED_LANGUAGES[*ix].0, cx);
            }));
        self.appearance_section(
            cx,
            "Language",
            "The language reported to Telegram. Applies after restart — the app's own text stays English for now.",
            control.into_any_element(),
        )
    }

    /// Parity slice (platform-custom-keybindings): the shortcuts section.
    /// Each row shows the action and its current keystroke; "Change" arms
    /// keystroke capture for that row, and "Reset" restores the default.
    /// The chip shows only a chord that is actually bound. A press that
    /// collides with fixed chrome or another rebindable action is refused
    /// and explained under the row.
    fn appearance_keybindings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use super::keybindings::{
            REBINDABLE_ACTIONS, apply_custom_bindings, conflict_message, resolve_keybindings,
        };
        use quill::settings::CustomKeybinding;

        let customs: Vec<CustomKeybinding> = self
            .live
            .as_ref()
            .map(|live| live.driver.load_custom_keybindings())
            .unwrap_or_default();
        let resolved = resolve_keybindings(&customs);
        let capturing = self.keybinding_capture.clone();
        let rows = REBINDABLE_ACTIONS.iter().map(|ra| {
            let row_state = resolved.iter().find(|row| row.id == ra.id);
            let current = row_state
                .map(|row| row.live.join(" / "))
                .filter(|live| !live.is_empty())
                .unwrap_or_else(|| "—".to_string());
            let id = ra.id.to_string();
            let label = ra.label.to_string();
            let is_capturing = capturing.as_deref() == Some(ra.id);
            let persisted_error = row_state.and_then(|row| {
                let conflict = row.rejected.as_ref()?;
                let chord = customs
                    .iter()
                    .find(|custom| custom.id == ra.id)
                    .map(|custom| custom.keystroke.as_str())
                    .unwrap_or(ra.id);
                Some(conflict_message(chord, conflict))
            });
            let error = if is_capturing {
                None
            } else {
                self.keybinding_error
                    .as_ref()
                    .filter(|(err_id, _)| err_id == ra.id)
                    .map(|(_, message)| message.clone())
                    .or(persisted_error)
            };
            // A joined pair like "cmd-shift-g / ctrl-shift-g" is wider than
            // the row; stack those so the label does not paint under the chip.
            let key_lines: Vec<String> = if is_capturing {
                vec!["press keys…".to_string()]
            } else if current.chars().count() > 20 {
                current.split(" / ").map(str::to_string).collect()
            } else {
                vec![current]
            };
            let row = div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .py_1()
                .child(div().flex_1().min_w_0().text_sm().child(label))
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_end()
                                .text_xs()
                                .font_family(super::message_text::MONO_FONT)
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(cx.theme().muted)
                                .children(key_lines.into_iter().map(|line| div().child(line))),
                        )
                        .child(
                            Button::new(format!("kb-change-{id}"))
                                .label(if is_capturing { "Cancel" } else { "Change" })
                                .small()
                                .ghost()
                                .on_click({
                                    let id = id.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        if this.keybinding_capture.as_deref() == Some(id.as_str()) {
                                            this.keybinding_capture = None;
                                        } else {
                                            this.keybinding_capture = Some(id.clone());
                                            window.focus(&this.keybinding_focus, cx);
                                        }
                                        cx.notify();
                                    })
                                }),
                        )
                        .child(
                            Button::new(format!("kb-reset-{id}"))
                                .label("Reset")
                                .small()
                                .ghost()
                                .on_click({
                                    let id = id.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        let custom = CustomKeybinding {
                                            id: id.clone(),
                                            keystroke: String::new(),
                                        };
                                        if let Some(live) = this.live.as_mut() {
                                            let _ = live.driver.save_custom_keybinding(custom);
                                            let customs = live.driver.load_custom_keybindings();
                                            apply_custom_bindings(cx, &customs);
                                        }
                                        this.keybinding_capture = None;
                                        if this
                                            .keybinding_error
                                            .as_ref()
                                            .is_some_and(|(err_id, _)| err_id == &id)
                                        {
                                            this.keybinding_error = None;
                                        }
                                        cx.notify();
                                    })
                                }),
                        ),
                );
            let row = if is_capturing {
                row.track_focus(&self.keybinding_focus)
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        // Stop the key before it dismisses Appearance or runs
                        // a global binding. GPUI also matches keybindings
                        // before this bubble handler; the capture interceptor
                        // consumes those. Modifier-only presses stay armed.
                        if this.keybinding_capture_active() {
                            cx.stop_propagation();
                            this.handle_keybinding_capture(&event.keystroke, cx);
                        }
                    }))
                    .into_any_element()
            } else {
                row.into_any_element()
            };
            if let Some(message) = error {
                div()
                    .flex()
                    .flex_col()
                    .child(row)
                    .child(div().text_xs().text_color(super::danger()).child(message))
                    .into_any_element()
            } else {
                row
            }
        });
        self.appearance_section(
            cx,
            "Keyboard shortcuts",
            "Rebind the shortcuts below. Changes apply immediately and are saved on this device. Window and app shortcuts (quit, close, …) can't be changed.",
            div().flex().flex_col().children(rows).into_any_element(),
        )
    }

    /// Apply one captured key while a shortcuts row is armed.
    ///
    /// Escape cancels. Bare modifiers are ignored so Ctrl+K can finish.
    /// A chord that collides with fixed chrome or another live shortcut is
    /// not saved; the active chip stays on the chord that is really bound.
    pub(super) fn handle_keybinding_capture(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) {
        use super::keybindings::{
            apply_custom_bindings, canonical_event_chord, conflict_message, is_modifier_key,
            keybinding_conflict,
        };
        use quill::settings::CustomKeybinding;

        if !self.keybinding_capture_active() {
            return;
        }
        let Some(id) = self.keybinding_capture.clone() else {
            return;
        };
        if is_modifier_key(&keystroke.key) {
            return;
        }
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.keybinding_capture = None;
            cx.notify();
            return;
        }
        let Some(chord) = canonical_event_chord(keystroke) else {
            return;
        };
        let customs = self
            .live
            .as_ref()
            .map(|live| live.driver.load_custom_keybindings())
            .unwrap_or_default();
        if let Some(conflict) = keybinding_conflict(&id, &chord, &customs) {
            self.keybinding_error = Some((id, conflict_message(&chord, &conflict)));
            self.keybinding_capture = None;
            cx.notify();
            return;
        }
        let custom = CustomKeybinding {
            id: id.clone(),
            keystroke: chord,
        };
        let saved = self.live.as_mut().map(|live| {
            let ok = live.driver.save_custom_keybinding(custom).is_ok();
            let customs = ok.then(|| live.driver.load_custom_keybindings());
            (ok, customs)
        });
        match saved {
            Some((true, Some(customs))) => {
                apply_custom_bindings(cx, &customs);
                if self
                    .keybinding_error
                    .as_ref()
                    .is_some_and(|(err_id, _)| err_id == &id)
                {
                    self.keybinding_error = None;
                }
            }
            Some((false, _)) => {
                self.keybinding_error = Some((
                    id,
                    "Couldn't save that shortcut. The previous one is still active.".into(),
                ));
            }
            _ => {}
        }
        self.keybinding_capture = None;
        cx.notify();
    }
}

crate::ui::shell::register_dialogs! {
    Appearance => DialogSpec::new(
        6600,
        |app| app.appearance_open,
        QuillApp::build_appearance_dialog,
    ),
}

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
