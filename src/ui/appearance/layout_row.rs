//! Appearance › Layout: the picker between Classic and Floating, shared by
//! the Settings card and the onboarding card. The selection is
//! `QuillApp::layout_choice`, and a click writes the pref through
//! `set_appearance`, so the two cards and the pref cannot disagree
//! (docs/decisions/floating-layout.md).

use super::*;
use crate::ui::chrome::miniature::miniature;
use gpui_kit::component::radio::{Radio, RadioGroup};
use quill::layout_style::{CLASSIC, ChromeMetrics, FLOATING, LayoutStyle};
use quill::settings::WallpaperSource;

/// The two options, in the order they are shown.
const OPTIONS: [LayoutStyle; 2] = [LayoutStyle::Classic, LayoutStyle::Floating];

/// The preview's size in the picker: two tiles fit the onboarding card.
const PREVIEW: gpui_kit::Size<Pixels> = gpui_kit::Size {
    width: px(160.),
    height: px(100.),
};

fn option_text(style: LayoutStyle) -> (&'static str, &'static str, &'static ChromeMetrics) {
    match style {
        LayoutStyle::Classic => (
            tr(keys::Q_PREFS_LAYOUT_CLASSIC),
            tr(keys::Q_PREFS_LAYOUT_CLASSIC_ABOUT),
            &CLASSIC,
        ),
        LayoutStyle::Floating => (
            tr(keys::Q_PREFS_LAYOUT_FLOATING),
            tr(keys::Q_PREFS_LAYOUT_FLOATING_ABOUT),
            &FLOATING,
        ),
    }
}

/// The test selector of an option's tile.
pub(in crate::ui) fn tile_selector(style: LayoutStyle) -> &'static str {
    match style {
        LayoutStyle::Classic => "layout-option-classic",
        LayoutStyle::Floating => "layout-option-floating",
    }
}

impl QuillApp {
    /// The two tiles, each a preview above its name and one line about it,
    /// as a radio group: Tab reaches it, the arrows move the choice.
    pub(in crate::ui) fn layout_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.layout_choice();
        let theme = cx.theme().clone();
        let radios = OPTIONS.map(|style| {
            let (name, about, metrics) = option_text(style);
            let selected = style == chosen;
            let ring = if selected {
                theme.primary
            } else {
                theme.border
            };
            Radio::new(tile_selector(style))
                .accessibility_label(name)
                // The radio's content takes its whole width: sized to the
                // tile, the two sit side by side.
                .w(PREVIEW.width + px(24.))
                .child(
                    div()
                        .debug_selector(move || tile_selector(style).into())
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .w(PREVIEW.width)
                        .child(
                            div()
                                .rounded(theme.radius_lg)
                                .overflow_hidden()
                                .border_2()
                                .border_color(ring)
                                .child(miniature(metrics, PREVIEW, &theme)),
                        )
                        .child(div().text_sm().font_medium().child(name))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(about),
                        ),
                )
        });
        let selected = OPTIONS.iter().position(|style| *style == chosen);
        RadioGroup::horizontal("appearance-layout")
            // A centred parent would shrink the group to one tile's width.
            .w_full()
            .selected_index(selected)
            .children(radios)
            .on_click(cx.listener(|this, &ix, _, cx| {
                let Some(style) = OPTIONS.get(ix).copied() else {
                    return;
                };
                this.set_appearance(cx, |appearance| appearance.layout = style);
            }))
            .into_any_element()
    }

    /// Floating over a flat background is cards on one colour: with
    /// Floating chosen and no wallpaper set, one quiet row offers the
    /// account's Telegram wallpaper. Derived, no state of its own:
    /// accepting sets the wallpaper and the row's condition is gone;
    /// declining is not pressing it. `onboarding` adds that the wallpaper
    /// arrives after sign-in.
    pub(in crate::ui) fn layout_wallpaper_offer(
        &self,
        onboarding: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let wanted = self.layout_choice() == LayoutStyle::Floating
            && self.appearance.wallpaper == WallpaperSource::Theme
            && self.theme_wallpaper().is_none();
        if !wanted {
            return None;
        }
        // A pattern for an account that never changed its background (or
        // before sign-in, when none is known); its own wallpaper otherwise.
        let dark = cx.theme().is_dark();
        let label = match self.account_telegram_wallpaper(dark) {
            None | Some(super::super::wallpaper::Wallpaper::Pattern { .. }) => {
                keys::Q_PREFS_LAYOUT_WALLPAPER_USE_PATTERN
            }
            Some(_) => keys::Q_PREFS_LAYOUT_WALLPAPER_USE_YOURS,
        };
        let muted = cx.theme().muted_foreground;
        let hint = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_PREFS_LAYOUT_WALLPAPER_HINT)),
            )
            .when(onboarding, |hint| {
                hint.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_FIRST_RUN_STYLE_WALLPAPER_LATER)),
                )
            });
        Some(
            div()
                .debug_selector(|| "layout-wallpaper-offer".into())
                .w_full()
                .flex()
                .items_center()
                .gap_3()
                .child(hint)
                .child(
                    Button::new("layout-wallpaper-use")
                        .outline()
                        .small()
                        .label(tr(label))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_appearance(cx, |appearance| {
                                appearance.wallpaper = WallpaperSource::Telegram;
                            });
                        })),
                )
                .into_any_element(),
        )
    }

    /// The Settings card's Layout row.
    pub(in crate::ui) fn appearance_layout_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let picker = self.layout_picker(cx);
        let section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .child(tr(keys::Q_PREFS_LAYOUT)),
            )
            .child(picker)
            .children(self.layout_wallpaper_offer(false, cx));
        self.setting_spot(&["appearance.layout"], section, cx)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "layout_tests.rs"]
mod tests;
