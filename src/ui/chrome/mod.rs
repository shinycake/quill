//! The window layout in the UI (docs/decisions/floating-layout.md): the
//! chrome a window draws this frame, read through [`QuillApp::chrome`], and
//! the helpers render sites pass their elements through. Render sites never
//! ask which layout is on (`scripts/check-layout-style.py`).
#![deny(clippy::indexing_slicing)]

pub(in crate::ui) mod bars;
pub(in crate::ui) mod frame;
pub(in crate::ui) mod frames;
pub(in crate::ui) mod history_parts;
pub(in crate::ui) mod list_parts;
pub(in crate::ui) mod menu_pop;
pub(in crate::ui) mod menu_shape;
pub(in crate::ui) mod miniature;
pub(in crate::ui) mod radius;

use crate::ui::app::QuillApp;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::layout_style::{
    BackdropKind, Chrome, ChromeMetrics, Contrast, LayoutStyle, Look, Power, Readings,
    contrast_ratio, look_for, metrics_of, resolve,
};

/// WCAG 2.1 relative luminance of a colour.
pub(in crate::ui) fn luminance(color: Hsla) -> f32 {
    let rgb = Rgba::from(color);
    let channel = |c: f32| {
        let c = c.clamp(0., 1.);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b)
}

/// Where a chat's wallpaper is painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum WallpaperSeat {
    /// Behind the history, which reports its area for gradient bubbles.
    History,
    /// Under the whole window, title strip included, behind the cards.
    Window,
}

/// How one page gives way to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PageSwitch {
    Cut,
    Cross,
}

/// How strongly the message under an open menu is tinted.
const MENU_TARGET_TINT: f32 = 0.35;

/// Web K's composer height glide.
const COMPOSER_GLIDE_MS: u16 = 250;

impl QuillApp {
    /// The layout in force: the `QUILL_DEMO_LAYOUT` override when set, else
    /// the Appearance choice. The Settings card and the onboarding card
    /// show this one, so a demo run with the override shows it selected.
    pub(in crate::ui) fn layout_choice(&self) -> LayoutStyle {
        quill::layout_style::choice(self.appearance.layout, LayoutStyle::demo_override())
    }

    /// How one page gives way to the next in this window (a chat switch,
    /// a folder page): cut in Classic, crossed in Floating.
    pub(in crate::ui) fn page_switch(&self) -> PageSwitch {
        match self.look() {
            Look::Lines => PageSwitch::Cut,
            Look::Cards => PageSwitch::Cross,
        }
    }

    /// How many milliseconds the rows take to follow the composer's new
    /// height (Web K's W12): 0, at once, in Classic and with reduced motion.
    pub(in crate::ui) fn composer_glide_ms(&self, cx: &App) -> u16 {
        match self.look() {
            Look::Cards if !cx.reduce_motion() => COMPOSER_GLIDE_MS,
            Look::Lines | Look::Cards => 0,
        }
    }

    /// How the open chat's row is drawn in this window.
    pub(in crate::ui) fn selected_row(&self) -> quill::layout_style::SelectedRow {
        self.look().selected_row()
    }

    /// The look of the window whose state is in place.
    pub(in crate::ui) fn look(&self) -> Look {
        look_for(
            self.appearance.layout,
            LayoutStyle::demo_override(),
            f32::from(self.slices.window.width.get()),
        )
    }

    /// The metrics of the window whose state is in place, which need no
    /// theme: the shell's columns read them.
    pub(in crate::ui) fn chrome_metrics(&self) -> &'static ChromeMetrics {
        metrics_of(self.look())
    }

    /// The room around this window's cards; `None` where its columns are
    /// lined and nothing stands apart.
    pub(in crate::ui) fn card_gutter(&self) -> Option<f32> {
        match crate::ui::shell_layout::Columns::of(self.chrome_metrics()) {
            crate::ui::shell_layout::Columns::Lined => None,
            crate::ui::shell_layout::Columns::Cards { gutter, .. } => Some(gutter),
        }
    }

    /// Where this window's width puts the chat list: a drawer in
    /// Floating's band of 601 to 925, beside the chat otherwise. It needs
    /// no other column's width, so the folder rail can ask it.
    pub(in crate::ui) fn list_dock_by_width(&self) -> crate::ui::shell_layout::ListDock {
        let width = f32::from(self.slices.window.width.get());
        match crate::ui::shell_layout::Columns::of(self.chrome_metrics()).drawer_at(width) {
            true => crate::ui::shell_layout::ListDock::Drawer,
            false => crate::ui::shell_layout::ListDock::Beside,
        }
    }

    /// Where the chat's wallpaper is painted: behind the history
    /// (Classic), or under the whole window, behind the cards (Floating).
    pub(in crate::ui) fn wallpaper_seat(&self) -> WallpaperSeat {
        match self.look() {
            Look::Lines => WallpaperSeat::History,
            Look::Cards => WallpaperSeat::Window,
        }
    }

    /// The chat header's frame: Classic's line under it, or nothing of its
    /// own: it is the first row of the bars' card (`stack_card`).
    pub(in crate::ui) fn header_frame<E: Styled>(&self, this: E, cx: &App) -> E {
        match self.look() {
            Look::Lines => this.border_b_1().border_color(cx.theme().border),
            Look::Cards => this,
        }
    }

    /// The composer's frame: Classic's line over it, or nothing of its own:
    /// it is a row of the composer's card (`stack_card`).
    pub(in crate::ui) fn composer_frame<E: Styled>(&self, this: E, cx: &App) -> E {
        match self.look() {
            Look::Lines => this.border_t_1().border_color(cx.theme().border),
            Look::Cards => this,
        }
    }

    /// Where the conversation's parts sit in this window's frame, from
    /// the pane's measured width and the pills' measured heights.
    pub(in crate::ui) fn conversation_frame(&self) -> frame::ConversationFrame {
        let metrics = self.chrome_metrics();
        match metrics.look {
            Look::Lines => frame::ConversationFrame::Stacked,
            Look::Cards => {
                let (top, composer) = self.slices.window.pill_heights();
                let placed = frame::floating_frame(frame::FrameInput {
                    pane: f32::from(self.history.measure.get().pane()),
                    gutter: metrics.gutter,
                    bar_gap: metrics.bar_gap,
                    chat_column: metrics.chat_column.unwrap_or(0.),
                    top: f32::from(top),
                    composer: f32::from(composer),
                });
                frame::ConversationFrame::Floating(frame::Pills {
                    gutter: px(metrics.gutter),
                    column: metrics.chat_column.map(px),
                    lanes_left: px(self.left_tabs_reserved()),
                    insets: placed.insets,
                })
            }
        }
    }

    /// How far the history's rows keep clear of the pills over them.
    pub(in crate::ui) fn history_insets(&self) -> frame::HistoryInsets {
        self.conversation_frame().insets()
    }

    /// A chat's left tab column: Classic's lined column, or a card.
    pub(in crate::ui) fn tab_column_frame<E: Styled + FluentBuilder>(
        &self,
        this: E,
        cx: &App,
    ) -> E {
        let theme = cx.theme();
        match self.look() {
            Look::Lines => this
                .bg(theme.background)
                .border_r_1()
                .border_color(theme.border),
            Look::Cards => {
                let chrome = self.chrome();
                let glass = self.card_glass(cx);
                frames::card(
                    frames::surface(this, &chrome, theme, glass.as_ref()),
                    &chrome,
                    theme,
                    glass.as_ref(),
                )
            }
        }
    }

    /// The glass the Floating cards draw while Glass effects is on and
    /// resolves to glass (the `Card` recipe), else `None`: the opaque
    /// card. Classic has no cards, so never any.
    pub(in crate::ui) fn card_glass(&self, cx: &App) -> Option<LensStyle> {
        match self.look() {
            Look::Lines => None,
            Look::Cards => super::glass::lens(super::glass::GlassRole::Card, cx),
        }
    }

    /// A chat row's resting fill: the list's own, or none on a glass card,
    /// where an opaque row would hide the glass.
    pub(in crate::ui) fn list_row_fill(&self, cx: &App) -> Hsla {
        match self.card_glass(cx) {
            Some(_) => transparent_black(),
            None => cx.theme().sidebar,
        }
    }

    /// A chat row's fill: [`Self::list_row_fill`], with the hover fill
    /// over it in Floating while the row's menu is open, so the menu shows
    /// which chat it is for (Web K marks the row under its menu).
    pub(in crate::ui) fn row_fill_under_menu(&self, chat: quill::ids::ChatId, cx: &App) -> Hsla {
        let fill = self.list_row_fill(cx);
        let target = self
            .message_ui
            .menu
            .row()
            .is_some_and(|menu| menu.chat_id == chat);
        match (self.look(), target) {
            (Look::Cards, true) => fill.blend(crate::ui::pressable::hover_fill(cx.theme())),
            (Look::Cards, false) | (Look::Lines, _) => fill,
        }
    }

    /// Where the chat header's ⋮ menu opens from its button: towards the
    /// card's inside in Floating, so it stays in the header card.
    pub(in crate::ui) fn header_menu_anchor(&self) -> gpui_kit::Anchor {
        match self.look() {
            Look::Lines => gpui_kit::Anchor::TopLeft,
            Look::Cards => gpui_kit::Anchor::TopRight,
        }
    }

    /// The icon a header ⋮ menu entry shows: its own over cards, as the
    /// row and message menus have; none in Classic.
    pub(in crate::ui) fn header_menu_icon(
        &self,
        icon: gpui_kit::assets::IconName,
    ) -> Option<gpui_kit::assets::IconName> {
        match self.look() {
            Look::Lines => None,
            Look::Cards => Some(icon),
        }
    }

    /// The tint Floating lays on the message its menu is open on (Web K
    /// marks it, so the menu shows whose it is); none in Classic.
    pub(in crate::ui) fn menu_target_tint(
        &self,
        chat: quill::ids::ChatId,
        message: quill::ids::MessageId,
        cx: &App,
    ) -> Option<Hsla> {
        let target = self
            .message_ui
            .menu
            .open()
            .is_some_and(|open| open.at.chat_id == chat && open.at.message_id == message);
        match (self.look(), target) {
            (Look::Cards, true) => Some(cx.theme().selection.opacity(MENU_TARGET_TINT)),
            (Look::Cards, false) | (Look::Lines, _) => None,
        }
    }

    /// What the window whose state is in place draws this frame, as its
    /// frame resolved it (`note_window_width`).
    pub(in crate::ui) fn chrome(&self) -> Chrome {
        self.slices.window.chrome.get()
    }

    fn chrome_readings(&self, cx: &App) -> Readings {
        let theme = cx.theme();
        let backdrop = match self.window_wallpaper(cx) {
            Some(_) => BackdropKind::Picture,
            None => BackdropKind::Plain {
                card_contrast: contrast_ratio(
                    luminance(theme.sidebar),
                    luminance(theme.background),
                ),
            },
        };
        Readings {
            contrast: if super::chat_theme::high_contrast() {
                Contrast::High
            } else {
                Contrast::Normal
            },
            power: if quill::battery_saving::forcing() {
                Power::Saver
            } else {
                Power::Normal
            },
            backdrop,
        }
    }

    /// The wallpaper behind the window's chat: the open chat's own, else
    /// the account's.
    fn window_wallpaper(&self, cx: &App) -> Option<super::wallpaper::Wallpaper> {
        let chat = self.viewed_chat().map(|chat| chat.0);
        self.chat_look(chat, cx).wallpaper
    }

    /// A window's frame begins: store its width and resolve its chrome
    /// once for the frame. When the chrome changed (a resize across 600
    /// px, a wallpaper with another contrast), render the cached slices
    /// again, as their own bounds may not have changed.
    pub(in crate::ui) fn note_window_width(&self, window: &Window, cx: &mut Context<Self>) {
        self.slices.window.width.set(window.viewport_size().width);
        let look = self.look();
        let chrome = resolve(look, || self.chrome_readings(cx));
        menu_shape::note_drawn_window(window.window_handle().window_id(), look, cx);
        if self.slices.window.chrome.replace(chrome) != chrome {
            self.notify_slices(cx);
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "chrome_tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "cards_tests.rs"]
mod cards_tests;
