//! What opens inside the box: the colors and emoji panels (tdesktop
//! `PeerQrColorsPanel`, `PeerQrEmojiPanel`), the free-message pill with
//! its menu (`PeerQrFreeController`) and the toast.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::card::hsla;
use super::surface::rgba;
use super::view_model::QrView;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::gpui::Size as ItemSize;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with, trn};
use quill::peer_qr::free::{FreeMenuItem, FreeMode};
use quill::peer_qr::layout::{
    BOX_WIDTH, BUTTON, BUTTON_SKIP, PANEL_MARGIN, PANEL_PADDING, PANEL_RADIUS, PILL_HEIGHT,
    SWATCH_COLUMNS, SWATCH_SKIP, content_in, panel_height, swatch_size,
};
use quill::peer_qr::link::countdown_text;
use quill::peer_qr::pattern::Rect;
use quill::peer_qr::state::{QrAction, QrPanel, QrToast};
use std::rc::Rc;
use std::sync::OnceLock;

fn at(value: f64) -> Pixels {
    px(value as f32)
}

/// Emoji per row of the emoji panel.
const EMOJI_COLUMNS: usize = 8;

/// Every plain emoji, in catalog order.
fn emoji_list() -> &'static [&'static str] {
    static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
    LIST.get_or_init(|| {
        quill::emoji_catalog::catalog()
            .map(|entry| entry.emoji)
            .collect()
    })
}

/// `lng_seconds` under a minute, else whole `lng_minutes`.
fn duration_text(seconds: i64) -> String {
    if seconds < 60 {
        trn(keys::SECONDS, seconds.max(0))
    } else {
        trn(keys::MINUTES, seconds / 60)
    }
}

/// The toast's text.
pub(super) fn toast_text(toast: QrToast) -> String {
    match toast {
        QrToast::CodeExpires(seconds) => tr_with(
            keys::NO_PAID_TOKEN_TOAST_EXPIRES,
            &[("duration", &duration_text(seconds))],
        ),
        QrToast::CodePermanent => tr(keys::NO_PAID_TOKEN_TOAST_PERMANENT).into(),
        QrToast::CodeReset => tr(keys::NO_PAID_TOKEN_TOAST_RESET).into(),
        QrToast::FeeRestored => tr(keys::NO_PAID_TOKEN_TOAST_RESTORED).into(),
        QrToast::CodeExpired => tr(keys::NO_PAID_TOKEN_TOAST_EXPIRED).into(),
        QrToast::Expiring { free_code: true } => tr(keys::NO_PAID_TOKEN_TOAST_EXPIRING).into(),
        QrToast::Expiring { free_code: false } => tr(keys::PEER_QR_LINK_EXPIRING).into(),
        QrToast::Unavailable => tr(keys::PEER_QR_LINK_UNAVAILABLE).into(),
        QrToast::Copied => tr(keys::GROUP_INVITE_QR_COPIED).into(),
    }
}

impl QuillApp {
    /// The glass growing out of the island, `rect` for this frame, with
    /// the panel's content fading in once it has room.
    pub(super) fn qr_panel(
        &mut self,
        view: &QrView,
        panel: QrPanel,
        rect: Rect,
        progress: f64,
        open_elapsed_ms: f64,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let radius = BUTTON / 2. + (PANEL_RADIUS - BUTTON / 2.) * progress;
        let width = BOX_WIDTH - 2. * PANEL_MARGIN;
        let shown = content_in(open_elapsed_ms).min(progress) as f32;
        let content = match panel {
            QrPanel::Colors => self.qr_colors_panel(view, width, cx).into_any_element(),
            QrPanel::Emoji => self.qr_emoji_panel(view, width, cx).into_any_element(),
        };
        div()
            .id("peer-qr-panel")
            .absolute()
            .left(at(rect.x))
            .top(at(rect.y))
            .w(at(rect.width))
            .h(at(rect.height))
            .rounded(at(radius))
            .bg(rgba(view.colors.glass))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(at(width))
                    .h(at(panel_height(width)))
                    .opacity(shown)
                    .child(content),
            )
    }

    /// The color circles, six to a row; the chosen one is ringed.
    fn qr_colors_panel(
        &mut self,
        view: &QrView,
        width: f64,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let circle = swatch_size(width);
        let chosen = self.peer_qr.open.as_ref().map(|open| open.state.color);
        let ring = hsla(view.colors.foreground);
        div()
            .id("peer-qr-colors-grid")
            .size_full()
            .overflow_y_scroll()
            .p(at(PANEL_PADDING))
            .flex()
            .flex_wrap()
            .gap(at(SWATCH_SKIP))
            .children(view.swatches.iter().map(|swatch| {
                let key = swatch.key;
                let id = match key {
                    quill::peer_qr::appearance::SwatchKey::Default => -1,
                    quill::peer_qr::appearance::SwatchKey::Backdrop(id) => i64::from(id),
                };
                let selected = chosen == Some(key);
                div()
                    .id(("peer-qr-swatch", id as u64))
                    .debug_selector(move || format!("peer-qr-swatch-{id}"))
                    .size(at(circle))
                    .rounded_full()
                    .p(px(3.))
                    .border_2()
                    .border_color(if selected {
                        ring
                    } else {
                        gpui_kit::transparent_black()
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.qr_act(QrAction::PickColor(key), cx);
                    }))
                    .child(
                        div()
                            .size_full()
                            .rounded_full()
                            .bg(hsla(swatch.edge))
                            .p(at(circle / 6.))
                            .child(div().size_full().rounded_full().bg(hsla(swatch.center))),
                    )
            }))
            .when(view.swatches.len() < SWATCH_COLUMNS, |grid| {
                grid.justify_center()
            })
    }

    /// Every emoji, picked ones highlighted; up to six make the pattern.
    fn qr_emoji_panel(
        &mut self,
        view: &QrView,
        width: f64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cell = ((width - 2. * PANEL_PADDING) / EMOJI_COLUMNS as f64).floor();
        let rows = emoji_list().len().div_ceil(EMOJI_COLUMNS);
        let sizes: Rc<Vec<ItemSize<Pixels>>> = Rc::new(vec![ItemSize::new(px(0.), at(cell)); rows]);
        let picked = self
            .peer_qr
            .open
            .as_ref()
            .map(|open| open.state.emoji.clone())
            .unwrap_or_default();
        let highlight = rgba(view.colors.control);
        v_virtual_list(
            cx.entity(),
            "peer-qr-emoji-grid",
            sizes,
            move |_this: &mut QuillApp, range, _window, cx| {
                range
                    .map(|row| {
                        let start = row * EMOJI_COLUMNS;
                        let cells = emoji_list()
                            .iter()
                            .skip(start)
                            .take(EMOJI_COLUMNS)
                            .enumerate()
                            .map(|(column, emoji)| {
                                let emoji: &'static str = emoji;
                                let index = (start + column) as u64;
                                let selected = picked.iter().any(|pick| pick == emoji);
                                let glyph =
                                    super::super::emoji_sprites::glyph(emoji, at(cell * 0.7))
                                        .unwrap_or_else(|| {
                                            div()
                                                .text_size(at(cell * 0.6))
                                                .child(emoji)
                                                .into_any_element()
                                        });
                                div()
                                    .id(("peer-qr-emoji", index))
                                    .debug_selector(move || format!("peer-qr-emoji-{index}"))
                                    .size(at(cell))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_lg()
                                    .cursor_pointer()
                                    .when(selected, |cell| cell.bg(highlight))
                                    .hover(move |cell| cell.bg(highlight.opacity(0.6)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.qr_act(QrAction::ToggleEmoji(emoji.to_string()), cx);
                                    }))
                                    .child(glyph)
                            });
                        div().px(at(PANEL_PADDING)).flex().children(cells)
                    })
                    .collect::<Vec<_>>()
            },
        )
        .pt(at(PANEL_PADDING))
        .size_full()
    }

    /// The pill under your own card while messages to you cost Stars.
    pub(super) fn qr_pill(
        &mut self,
        view: &QrView,
        top: f64,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !view.own || view.env.price <= 0 {
            return None;
        }
        let open = self.peer_qr.open.as_ref()?;
        let foreground = hsla(view.colors.foreground);
        let label = |text: String| div().child(text).into_any_element();
        let content: Vec<AnyElement> = match &open.state.free.mode {
            FreeMode::Fee => {
                let template = tr(keys::NO_PAID_TOKEN_PILL_REMOVE_FEE);
                let amount = view.env.price.to_string();
                match template.split_once("{emoji}") {
                    Some((before, after)) => vec![
                        label(before.trim_end().to_string()),
                        crate::ui::earn::star_icon(13.),
                        label(after.trim_start().replace("{amount}", &amount)),
                    ],
                    None => vec![label(tr_with(
                        keys::NO_PAID_TOKEN_PILL_REMOVE_FEE,
                        &[("emoji", ""), ("amount", &amount)],
                    ))],
                }
            }
            FreeMode::Temporary { code } => vec![label(tr_with(
                keys::NO_PAID_TOKEN_PILL_TEMPORARY,
                &[("time", &countdown_text(code.link.expires_at - view.env.now))],
            ))],
            FreeMode::Permanent { .. } => {
                vec![label(tr(keys::NO_PAID_TOKEN_PILL_PERMANENT).to_string())]
            }
        };
        let busy = open.state.free.requesting.is_some();
        let control = rgba(view.colors.control);
        let pill = div()
            .id("peer-qr-pill")
            .h(at(PILL_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_1()
            .rounded_full()
            .bg(control)
            .hover(move |pill| pill.bg(control.opacity(0.8)))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(foreground)
            .cursor_pointer()
            .when(busy, |pill| {
                pill.child(
                    gpui_kit::component::spinner::Spinner::new()
                        .xsmall()
                        .color(foreground),
                )
            })
            .children(content)
            .on_click(cx.listener(|this, _, _, cx| this.qr_act(QrAction::PillClicked, cx)));
        Some(
            div()
                .absolute()
                .top(at(top))
                .left_0()
                .w_full()
                .flex()
                .justify_center()
                .child(pill),
        )
    }

    /// The pill's menu over it (the box clips anything below), and a
    /// cover that closes it.
    pub(super) fn qr_free_menu(
        &mut self,
        view: &QrView,
        pill_top: f64,
        box_height: f64,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(open) = self
            .peer_qr
            .open
            .as_ref()
            .filter(|open| open.state.free.menu_open)
        else {
            return Vec::new();
        };
        let permanent = matches!(open.state.free.mode, FreeMode::Permanent { .. });
        let items = open.state.free.menu_items(view.env.temporary_only);
        let theme = cx.theme();
        let (popover, border, muted) = (theme.popover, theme.border, theme.muted_foreground);
        let entries = items.into_iter().map(|item| {
            let (id, icon, text, about) = match item {
                FreeMenuItem::Reset => (
                    "peer-qr-menu-reset",
                    IconName::RefreshCw,
                    tr(keys::NO_PAID_TOKEN_MENU_RESET),
                    None,
                ),
                FreeMenuItem::ToggleTimer if permanent => (
                    "peer-qr-menu-timer",
                    IconName::Timer,
                    tr(keys::NO_PAID_TOKEN_MENU_ENABLE_TIMER),
                    Some(tr(keys::NO_PAID_TOKEN_MENU_ENABLE_TIMER_ABOUT)),
                ),
                FreeMenuItem::ToggleTimer => (
                    "peer-qr-menu-timer",
                    IconName::TimerOff,
                    tr(keys::NO_PAID_TOKEN_MENU_DISABLE_TIMER),
                    Some(tr(keys::NO_PAID_TOKEN_MENU_DISABLE_TIMER_ABOUT)),
                ),
                FreeMenuItem::RestoreFee => (
                    "peer-qr-menu-restore",
                    IconName::Star,
                    tr(keys::NO_PAID_TOKEN_MENU_RESTORE),
                    None,
                ),
            };
            Button::new(id)
                .ghost()
                .icon(icon)
                .w_full()
                .justify_start()
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_start()
                        .child(text)
                        .children(
                            about.map(|about| div().text_xs().text_color(muted).child(about)),
                        ),
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.qr_act(QrAction::MenuChose(item), cx)),
                )
        });
        let cover = div()
            .id("peer-qr-menu-dismiss")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.qr_act(QrAction::MenuDismissed, cx)),
            )
            .into_any_element();
        let menu = div()
            .absolute()
            .bottom(at(box_height - pill_top + 6.))
            .left_0()
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("peer-qr-menu")
                    .min_w(px(220.))
                    .p_1()
                    .flex()
                    .flex_col()
                    .rounded_lg()
                    .border_1()
                    .border_color(border)
                    .bg(popover)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(entries),
            )
            .into_any_element();
        vec![cover, menu]
    }

    /// The box's toast, over its bottom edge.
    pub(super) fn qr_toast_element(&self) -> Option<Div> {
        let (toast, _) = self.peer_qr.open.as_ref()?.toast?;
        Some(
            div()
                .absolute()
                .bottom(at(BUTTON_SKIP * 2. + BUTTON))
                .left_0()
                .w_full()
                .px_4()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("peer-qr-toast")
                        .debug_selector(|| "peer-qr-toast".into())
                        .max_w(px(320.))
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .bg(gpui_kit::black().opacity(0.78))
                        .text_sm()
                        .text_color(gpui_kit::white())
                        .child(toast_text(toast)),
                ),
        )
    }
}
