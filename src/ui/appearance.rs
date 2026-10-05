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
    #[cfg(target_os = "macos")]
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
    /// change, and `Theme::sync_base` mirrors the mutated fields (incl.
    /// accent) into the Base layer. Only notifies when the (mode,
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
        let accent = self.appearance.accent_rgb;
        if self.appearance_applied == Some((mode, accent, hc)) {
            return;
        }
        set_theme_mode(mode, None, cx);
        set_high_contrast(hc);
        Theme::global_mut(cx).colors.primary = if accent == 0 {
            Hsla::from(super::chat_theme::accent_strong())
        } else {
            Hsla::from(rgb(accent))
        };
        // The accent mutation touches fields the Base layer mirrors
        // (scrollbar styles, semantic tokens, text-view defaults) — they
        // only reach the Base layer once Theme::sync_base runs.
        Theme::sync_base(cx);
        self.appearance_applied = Some((mode, accent, hc));
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
        self.group_call_composer.update(cx, |input, cx| {
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
            let mut body = div().flex().flex_col().gap_3();
            if this.keybindings_screenshot {
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
                body = body.child(this.appearance_auto_night_section(cx));
                body = body.child(this.appearance_accent_section(cx));
                body = body.child(this.appearance_wallpaper_section(cx));
                body = body.child(this.appearance_font_section(cx));
                body = body.child(this.appearance_bubble_section(cx));
                body = body.child(this.appearance_chat_list_section(cx));
                body = body.child(this.appearance_send_key_section(cx));
                // Slice parity:settings-language: the app language picker
                // (the tag TDLib gets in `setTdlibParameters`).
                body = body.child(this.appearance_language_section(cx));
                body = body.child(this.general_autostart_section(cx));
                body = body.child(this.update_settings_section(cx));
                body = body.child(this.appearance_section(
                    cx, "Start in tray", "Open Quill from its tray menu when needed.",
                    Switch::new("general-start-in-tray").checked(this.appearance.start_in_tray)
                        .accessibility_label("Start Quill in the system tray")
                        .on_click(cx.listener(|this, &on, _, cx| this.set_appearance(cx, |a| a.start_in_tray = on)))
                        .into_any_element(),
                ));
                // Parity slice (platform-custom-keybindings).
                if cfg!(target_os = "macos") {
                    body = body.child(this.appearance_section(
                        cx, "Minimize to tray", "Use the tray menu to reopen Quill.",
                        Switch::new("general-minimize-to-tray").checked(this.appearance.minimize_to_tray)
                            .accessibility_label("Minimize Quill to the system tray")
                            .on_click(cx.listener(|this, &on, _, cx| this.set_appearance(cx, |a| a.minimize_to_tray = on))).into_any_element()
                    ));
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
    fn appearance_section(
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
        let current = self.appearance.accent_rgb;
        let mut row = div()
            .flex()
            .gap_2()
            .items_center()
            .child(self.appearance_chip(
                "appearance-accent-default",
                "Default",
                current == 0,
                cx,
                |this, cx| this.set_appearance(cx, |a| a.accent_rgb = 0),
            ));
        for &(color, name) in ACCENT_PRESETS {
            row = row.child(self.appearance_swatch(
                format!("appearance-accent-{color:06x}"),
                color,
                name,
                current == color,
                cx,
                move |this, cx| this.set_appearance(cx, |a| a.accent_rgb = color),
            ));
        }
        self.appearance_section(
            cx,
            "Accent color",
            "Highlights, selections and links across the app.",
            row.into_any_element(),
        )
    }

    fn appearance_wallpaper_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.appearance.wallpaper_rgb;
        let mut row = div()
            .flex()
            .gap_2()
            .items_center()
            .child(self.appearance_chip(
                "appearance-wallpaper-default",
                "Default",
                current.is_none(),
                cx,
                |this, cx| this.set_appearance(cx, |a| a.wallpaper_rgb = None),
            ));
        for &(color, name) in WALLPAPER_PRESETS {
            row = row.child(self.appearance_swatch(
                format!("appearance-wallpaper-{color:06x}"),
                color,
                name,
                current == Some(color),
                cx,
                move |this, cx| this.set_appearance(cx, |a| a.wallpaper_rgb = Some(color)),
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
    fn appearance_switch_row(
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
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(hint.to_string()),
                    ),
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
        body.into_any_element()
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
                "Autostart is not supported on this platform yet.",
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
    /// pattern as `appearance_switch_row`, but wired to ChatPrefs).
    fn appearance_spellcheck_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let checked = self.chat_prefs.spellcheck_enabled;
        let control = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child("Check spelling"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Flag misspelled words in the message composer (English)."),
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
            )
            .into_any_element();
        self.appearance_section(cx, "Spelling", "", control)
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
