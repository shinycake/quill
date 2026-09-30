//! Settings → Appearance slice: the dialog plus the load/save/apply
//! funnel. Everything here is client-side — theme, auto-night, accent,
//! wallpaper, message font size and bubble style live in
//! `appearance_prefs.json` (see `quill::settings::AppearancePrefs`);
//! there is no TDLib setting for any of it.

use super::QuillApp;
use super::chat_theme::set_theme_mode;
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
    AccountPaths, AppearancePrefs, AutoNight, ThemeChoice, clamp_font_size, load_appearance_prefs,
    local_minutes_since_midnight, night_active, safe_app_root, save_appearance_prefs,
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
    /// accent) pair actually changed — the minute tick calls this and
    /// must be free when idle.
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
        set_theme_mode(mode, None, cx);
        if accent != 0 {
            Theme::global_mut(cx).colors.accent = Hsla::from(rgb(accent));
        }
        // The accent mutation touches fields the Base layer mirrors
        // (scrollbar styles, semantic tokens, text-view defaults) — they
        // only reach the Base layer once Theme::sync_base runs.
        Theme::sync_base(cx);
        self.appearance_applied = Some((mode, accent));
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
                this.appearance_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let mut body = div().flex().flex_col().gap_3();
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Theme, accent, wallpaper, text size, chat style and chat list. \
                         Changes apply immediately and are saved on this device.",
                    ),
            );
            body = body.child(this.appearance_theme_section(cx));
            body = body.child(this.appearance_auto_night_section(cx));
            body = body.child(this.appearance_accent_section(cx));
            body = body.child(this.appearance_wallpaper_section(cx));
            body = body.child(this.appearance_font_section(cx));
            body = body.child(this.appearance_bubble_section(cx));
            // Parity slice chatlist-list-style: chat-list density and
            // preview display.
            body = body.child(this.appearance_chat_list_section(cx));
            let footer = div().flex().justify_end().child(
                Button::new("close-appearance")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.appearance_open = false;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::Appearance, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title("Appearance")
                .content({
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
                })
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
        let selected = Some(if self.appearance.theme == ThemeChoice::Light {
            0
        } else {
            1
        });
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-theme")
            .selected_index(selected)
            .children([
                Radio::new("appearance-theme-light").label("☀️ Light"),
                Radio::new("appearance-theme-dark").label("🌙 Dark"),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| {
                    a.theme = if ix == 0 {
                        ThemeChoice::Light
                    } else {
                        ThemeChoice::Dark
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

    /// Parity slice chatlist-list-style: chat-list density and preview
    /// display, in Settings → Appearance (kit Phase 6 section style).
    /// TGX 0.22.2 ships the same options under Themes and Chats →
    /// Chats List Style.
    fn appearance_chat_list_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = self.appearance.chat_list_style();
        let lines = RadioGroup::horizontal("appearance-chat-list-lines")
            .selected_index(Some(usize::from(style.three_lines)))
            .children([
                Radio::new("appearance-chat-list-two").label("Two lines"),
                Radio::new("appearance-chat-list-three").label("Three lines"),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| a.chat_list_three_lines = ix == 1);
            }));
        let muted = cx.theme().muted_foreground;
        let switch_row = |label: &'static str, hint: &'static str, switch: Switch| -> AnyElement {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(label)
                        .child(div().text_xs().text_color(muted).child(hint)),
                )
                .child(switch)
                .into_any_element()
        };
        let control = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(lines)
            .child(switch_row(
                "Media icons",
                "Show a type glyph (📷, 🎬, 🎙…) before media message previews.",
                Switch::new("appearance-chat-list-media-icons")
                    .checked(style.media_icons)
                    .accessibility_label("Media icons")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_appearance(cx, |a| a.chat_list_media_icons = on);
                    })),
            ))
            .child(switch_row(
                "Formatted previews",
                "Render bold, italic and other formatting in message previews.",
                Switch::new("appearance-chat-list-rich-preview")
                    .checked(style.rich_preview)
                    .accessibility_label("Formatted previews")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_appearance(cx, |a| a.chat_list_rich_preview = on);
                    })),
            ));
        self.appearance_section(
            cx,
            "Chat list",
            "Two or three lines per chat, media icons and formatted previews.",
            control.into_any_element(),
        )
    }
}
