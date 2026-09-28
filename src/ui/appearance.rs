//! Settings → Appearance slice: the dialog plus the load/save/apply
//! funnel. Everything here is client-side — theme, auto-night, accent,
//! wallpaper, message font size and bubble style live in
//! `appearance_prefs.json` (see `quill::settings::AppearancePrefs`);
//! there is no TDLib setting for any of it.

use super::QuillApp;
use super::synthetic::BubbleLook;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::theme::{ActiveTheme, Theme, ThemeMode};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::AccountKey;
use quill::settings::{
    AccountPaths, AppearancePrefs, AutoNight, ThemeChoice, clamp_font_size, default_app_root,
    load_appearance_prefs, local_minutes_since_midnight, night_active, save_appearance_prefs,
};

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
    /// Account-rooted prefs path. Appearance is a device setting (like
    /// Telegram's locally stored theme choice), so it lives under the
    /// primary account's root rather than per-account data.
    pub(crate) fn appearance_paths() -> AccountPaths {
        AccountPaths::for_root(&default_app_root(), &AccountKey::primary())
    }

    /// Load persisted prefs (defaults when the file is missing/corrupt).
    pub(crate) fn load_appearance() -> AppearancePrefs {
        load_appearance_prefs(&Self::appearance_paths())
    }

    /// Recompute the effective theme from the prefs (manual choice,
    /// overridden by auto-night while active) and push it into the
    /// gpui-component global Theme. `Theme::change` resets the whole
    /// palette, so the accent override is re-applied after every mode
    /// change. Only notifies when the (mode, accent) pair actually
    /// changed — the minute tick calls this and must be free when idle.
    pub(crate) fn apply_appearance(&mut self, cx: &mut Context<Self>) {
        let dark = match self.appearance.auto_night {
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
        };
        let mode = if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        let accent = self.appearance.accent_rgb;
        if self.appearance_applied == Some((mode, accent)) {
            return;
        }
        Theme::change(mode, None, cx);
        if accent != 0 {
            Theme::global_mut(cx).colors.accent = Hsla::from(rgb(accent));
        }
        self.appearance_applied = Some((mode, accent));
        cx.notify();
    }

    /// The single funnel every Appearance control uses: mutate, clamp,
    /// persist, re-apply, re-render. A change can never be
    /// visible-but-unsaved or saved-but-not-applied.
    fn set_appearance(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut AppearancePrefs)) {
        f(&mut self.appearance);
        self.appearance.font_size_px = clamp_font_size(self.appearance.font_size_px);
        if let Err(err) = save_appearance_prefs(&Self::appearance_paths(), &self.appearance) {
            self.status_note = format!("Couldn't save appearance settings: {err}");
        }
        self.apply_appearance(cx);
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
            let next = cur.wrapping_add(delta).rem_euclid(24 * 60) as u16;
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

    /// The Appearance dialog (TGX Settings → Appearance / tdesktop
    /// Settings → Appearance). Every control applies live through
    /// `set_appearance`. Shell mirrors `notification_defaults_overlay`.
    pub(crate) fn appearance_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        div()
            .id("appearance-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("appearance-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.appearance_open = false;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("appearance-dialog")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .rounded_lg()
                    .bg(theme.sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .min_w(px(460.))
                    .max_w(px(600.))
                    .max_h(px(720.))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().font_semibold().child("Appearance"))
                            .child(
                                Button::new("close-appearance")
                                    .label("Close")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.appearance_open = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "Theme, accent, wallpaper, text size and chat style. \
                                 Changes apply immediately and are saved on this device.",
                    ))
                    .child(self.appearance_theme_section(cx))
                    .child(self.appearance_auto_night_section(cx))
                    .child(self.appearance_accent_section(cx))
                    .child(self.appearance_wallpaper_section(cx))
                    .child(self.appearance_font_section(cx))
                    .child(self.appearance_bubble_section(cx)),
            )
            .into_any_element()
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
    fn appearance_chip(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        selected: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut QuillApp, &mut Context<QuillApp>) + 'static,
    ) -> AnyElement {
        let theme = cx.theme();
        div()
            .id(id.into())
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(if selected { theme.accent } else { theme.border })
            .when(selected, |this| this.bg(theme.accent.opacity(0.15)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
            .child(div().text_sm().child(label.into()))
            .into_any_element()
    }

    /// A color swatch; the selected one gets the accent ring.
    fn appearance_swatch(
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
        let current = self.appearance.theme;
        let chips = div()
            .flex()
            .gap_2()
            .child(self.appearance_chip(
                "appearance-theme-light",
                "☀️ Light",
                current == ThemeChoice::Light,
                cx,
                |this, cx| this.set_appearance(cx, |a| a.theme = ThemeChoice::Light),
            ))
            .child(self.appearance_chip(
                "appearance-theme-dark",
                "🌙 Dark",
                current == ThemeChoice::Dark,
                cx,
                |this, cx| this.set_appearance(cx, |a| a.theme = ThemeChoice::Dark),
            ));
        let hint = if self.appearance.auto_night == AutoNight::Off {
            "Applies to the whole app immediately."
        } else {
            "Auto-night is on — this applies while night mode is inactive."
        };
        self.appearance_section(cx, "Theme", hint, chips.into_any_element())
    }

    fn appearance_auto_night_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.appearance.auto_night;
        let mut chips = div().flex().gap_2();
        for &(mode, id, label) in &[
            (AutoNight::Off, "appearance-night-off", "Off"),
            (AutoNight::System, "appearance-night-system", "System"),
            (
                AutoNight::Scheduled,
                "appearance-night-scheduled",
                "Scheduled",
            ),
        ] {
            chips = chips.child(self.appearance_chip(
                id,
                label,
                current == mode,
                cx,
                move |this, cx| this.set_appearance(cx, |a| a.auto_night = mode),
            ));
        }
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.appearance_section(
                cx,
                "Auto-night",
                "Automatically switch to the dark theme at night.",
                chips.into_any_element(),
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
        let control = div()
            .flex()
            .gap_2()
            .child(self.appearance_chip(
                "appearance-style-bubbles",
                "💬 Bubbles",
                bubbles,
                cx,
                |this, cx| this.set_appearance(cx, |a| a.bubbles = true),
            ))
            .child(self.appearance_chip(
                "appearance-style-plain",
                "📄 Plain",
                !bubbles,
                cx,
                |this, cx| this.set_appearance(cx, |a| a.bubbles = false),
            ));
        self.appearance_section(
            cx,
            "Chat style",
            "Bubbles or plain rows without bubble backgrounds.",
            control.into_any_element(),
        )
    }
}
