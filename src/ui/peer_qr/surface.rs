//! The box itself (tdesktop `FillPeerQrBox` `relayout` and
//! `PeerQrSurface::paintEvent`): the backdrop, the emoji pattern, the
//! card, the round buttons, the pill, the open panel and the toast,
//! laid out from [`quill::peer_qr::layout`] for the panel's progress.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use super::card::{card, fit, hsla};
use super::pattern_view::{PatternArgs, pattern_canvas};
use super::view_model::QrView;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::peer_qr::color::Rgba;
use quill::peer_qr::layout::{
    BOX_WIDTH, BUTTON, BUTTON_SKIP, CARD_SKIP, CARD_WIDTH, ISLAND_BUTTON, PILL_SKIP, box_layout,
    card_layout, placement,
};
use quill::peer_qr::pattern::Rect;
use quill::peer_qr::state::{QrAction, QrPanel};
use std::time::Instant;

/// Frames a second while something in the box moves.
const FPS: u32 = 60;

pub(super) fn rgba(color: Rgba) -> Hsla {
    gpui_kit::rgba(color.to_rgba_hex()).into()
}

fn at(value: f64) -> Pixels {
    px(value as f32)
}

/// A round 40-point button on the backdrop (tdesktop `peerQrButton`).
pub(super) fn round_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    view: &QrView,
    cx: &App,
) -> Button {
    let control = rgba(view.colors.control);
    Button::new(id)
        .icon(icon)
        .rounded_full()
        .custom(
            ButtonCustomVariant::new(cx)
                .color(control)
                .foreground(hsla(view.colors.foreground))
                .hover(control.opacity(0.8))
                .active(control.opacity(0.6)),
        )
        .w(px(BUTTON as f32))
        .h(px(BUTTON as f32))
        .tooltip(label)
        .accessibility_label(label)
}

impl QuillApp {
    pub(in crate::ui) fn build_peer_qr_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::PeerQr, |this, _, cx| {
            this.close_qr_box(cx);
        });
        let edge = app.read(cx).qr_view().map(|view| view.appearance.edge);
        let edge = hsla(edge.unwrap_or(quill::peer_qr::appearance::DEFAULT_EDGE));
        let cancel_app = app.clone();
        let app = app.clone();
        dialog
            .overlay(true)
            .close_button(false)
            .w(px(BOX_WIDTH as f32))
            .p_0()
            .bg(edge)
            .border_color(edge)
            .overflow_hidden()
            .content(move |content, window, cx| {
                let body = app.update(cx, |this, cx| this.qr_surface(window, cx));
                content.child(body)
            })
            .on_ok(|_, _, _| false)
            .on_cancel(move |_, _, cx| {
                cancel_app.update(cx, |this, cx| {
                    let panel_open = this
                        .peer_qr
                        .open
                        .as_ref()
                        .is_some_and(|open| open.state.panel.is_some());
                    if panel_open {
                        this.qr_act(QrAction::ClosePanel, cx);
                    }
                    !panel_open
                })
            })
            .on_close(on_close)
    }

    /// The box for this frame. Esc reaches it as the dialog's cancel, and
    /// closes an open panel before the box.
    fn qr_surface(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.qr_view() else {
            return div().into_any_element();
        };
        let body = if view.peer_box() {
            self.qr_peer_surface(&view, window, cx)
        } else {
            self.qr_custom_surface(&view, cx)
        };
        div()
            .id("peer-qr-box")
            .debug_selector(|| "peer-qr-box".into())
            .child(body)
            .into_any_element()
    }

    /// The backdrop: the edge color, and the gradient centered on the
    /// card fading in over the previous one, clipped to the box's rounded
    /// corners.
    fn qr_backdrop(&mut self, view: &QrView, card: Rect, cx: &mut Context<Self>) -> Div {
        let radius = super::art::backdrop_radius();
        let (cx0, cy0) = card.center();
        let layers =
            self.qr_backdrops(view.appearance.center, view.appearance.edge, Instant::now());
        let fading = layers.iter().any(|(_, opacity)| *opacity < 1.);
        if fading {
            self.request_animation_tick(FPS, cx);
        }
        let corners = Corners::all(cx.theme().radius_lg);
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(hsla(view.appearance.edge))
            .children(layers.into_iter().map(move |(image, opacity)| {
                div().absolute().size_full().opacity(opacity).child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, (), window, _| {
                            let side = at(2. * radius);
                            let placed = Bounds::new(
                                bounds.origin + point(at(cx0 - radius), at(cy0 - radius)),
                                size(side, side),
                            );
                            let _ = window.paint_image(bounds, placed, corners, image, 0, false);
                        },
                    )
                    .size_full(),
                )
            }))
    }

    /// A peer's box, laid out for the panel's progress.
    fn qr_peer_surface(
        &mut self,
        view: &QrView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = Instant::now();
        let animate = !cx.reduce_motion();
        let Some(open) = self.peer_qr.open.as_ref() else {
            return div().into_any_element();
        };
        let motion = open.motion;
        let opened = open.opened;
        let picks = open.state.emoji.clone();
        let swap = open.swap.clone();
        let progress = motion.progress(now, animate);
        let layout = box_layout(true, if view.can_style { 2 } else { 0 });
        let place = placement(&layout, progress);
        if motion.running(now, animate) {
            self.request_animation_tick(FPS, cx);
        }
        let card_rest = card_layout(CARD_WIDTH, true);
        let name = view
            .label
            .as_ref()
            .and_then(|label| fit(label, &card_rest, window));
        let code = view
            .shown
            .as_ref()
            .and_then(|shown| self.qr_code_image(&shown.link, view.colors.ink));
        let mut glass = vec![layout.close, layout.copy, layout.island, layout.share];
        if progress > 0. {
            glass.push(place.panel);
        }
        let frame = self.qr_pattern_frame(PatternArgs {
            picks: &picks,
            swap: swap.as_ref(),
            opened,
            now,
            animate,
            width: layout.width,
            height: layout.height,
            card: place.card,
            glass: &glass,
        });
        if frame.moving {
            self.request_animation_tick(FPS, cx);
        }
        let controls = (1. - progress).clamp(0., 1.) as f32;
        let mut root = div()
            .relative()
            .w(at(layout.width))
            .h(at(layout.height))
            .overflow_hidden()
            .child(self.qr_backdrop(view, place.card, cx))
            .child(pattern_canvas(frame))
            .child(
                div()
                    .absolute()
                    .left(at(place.card.x))
                    .top(at(place.card.y))
                    .child(card(
                        &card_rest,
                        place.scale,
                        code,
                        view.colors.ink,
                        name.as_ref(),
                    )),
            )
            .children(self.qr_pill(view, place.pill_top, cx))
            .child(
                div()
                    .absolute()
                    .left(at(layout.close.x))
                    .top(at(layout.close.y))
                    .child(self.qr_close_button(view, cx)),
            );
        if controls > 0. {
            root = root
                .child(
                    div()
                        .absolute()
                        .left(at(layout.copy.x))
                        .top(at(layout.copy.y))
                        .opacity(controls)
                        .child(self.qr_copy_button(view, cx)),
                )
                .children(view.can_share.then(|| {
                    div()
                        .absolute()
                        .left(at(layout.share.x))
                        .top(at(layout.share.y))
                        .opacity(controls)
                        .child(self.qr_share_button(view, cx))
                }))
                .children(view.can_style.then(|| {
                    div()
                        .absolute()
                        .left(at(layout.island.x))
                        .top(at(layout.island.y))
                        .opacity(controls)
                        .child(self.qr_island(view, cx))
                }));
        }
        if let Some(panel) = motion.shown.filter(|_| progress > 0.) {
            root = root
                .child(
                    div()
                        .id("peer-qr-panel-dismiss")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| this.qr_act(QrAction::ClosePanel, cx)),
                        ),
                )
                .child(self.qr_panel(
                    view,
                    panel,
                    place.panel,
                    progress,
                    motion.open_elapsed_ms(now),
                    cx,
                ));
        }
        root.children(self.qr_free_menu(view, place.pill_top, window, cx))
            .children(self.qr_toast_element())
            .into_any_element()
    }

    /// An invite, chat or proxy link: a title by the close button, the
    /// square card, the about line, then copy and share.
    fn qr_custom_surface(&mut self, view: &QrView, cx: &mut Context<Self>) -> AnyElement {
        let card_rest = card_layout(CARD_WIDTH, false);
        let card_left = ((BOX_WIDTH - CARD_WIDTH) / 2.).floor();
        let card_top = BUTTON_SKIP + BUTTON + CARD_SKIP;
        let card_rect = Rect::new(card_left, card_top, card_rest.width, card_rest.height);
        let code = view
            .shown
            .as_ref()
            .and_then(|shown| self.qr_code_image(&shown.link, view.colors.ink));
        let foreground = hsla(view.colors.foreground);
        let header = div()
            .h(at(BUTTON_SKIP * 2. + BUTTON))
            .pl(px(24.))
            .pr(at(BUTTON_SKIP))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(foreground)
                    .children(view.title.map(tr)),
            )
            .child(self.qr_close_button(view, cx));
        let about = view.about.map(|about| {
            div()
                .w(at(CARD_WIDTH))
                .mx_auto()
                .mt(at(PILL_SKIP))
                .text_sm()
                .text_center()
                .text_color(foreground)
                .child(tr(about))
        });
        let buttons = div()
            .mt(at(CARD_SKIP))
            .px(at(BUTTON_SKIP))
            .pb(at(BUTTON_SKIP))
            .flex()
            .justify_between()
            .child(self.qr_copy_button(view, cx))
            .children(view.can_share.then(|| self.qr_share_button(view, cx)));
        div()
            .relative()
            .w(at(BOX_WIDTH))
            .overflow_hidden()
            .child(self.qr_backdrop(view, card_rect, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(
                        div()
                            .mt(at(card_top - (BUTTON_SKIP * 2. + BUTTON)))
                            .mx_auto()
                            .child(card(&card_rest, 1., code, view.colors.ink, None)),
                    )
                    .children(about)
                    .child(buttons),
            )
            .children(self.qr_toast_element())
            .into_any_element()
    }

    /// Colors and Emoji, one pill between copy and share.
    fn qr_island(&mut self, view: &QrView, cx: &mut Context<Self>) -> Div {
        let control = rgba(view.colors.control);
        let button = |id: &'static str,
                      icon: IconName,
                      label: &'static str,
                      panel: QrPanel,
                      cx: &mut Context<Self>| {
            tagged(
                id,
                Button::new(id)
                    .icon(icon)
                    .ghost()
                    .rounded_full()
                    .w(at(ISLAND_BUTTON))
                    .h(at(BUTTON))
                    .text_color(hsla(view.colors.foreground))
                    .tooltip(label)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.qr_act(QrAction::OpenPanel(panel), cx)
                    })),
            )
        };
        div()
            .flex()
            .rounded_full()
            .bg(control)
            .child(button(
                "peer-qr-colors",
                IconName::Palette,
                tr(keys::QR_BOX_COLORS),
                QrPanel::Colors,
                cx,
            ))
            .child(button(
                "peer-qr-emoji",
                IconName::FaceSlightlySmiling,
                tr(keys::QR_BOX_EMOJI),
                QrPanel::Emoji,
                cx,
            ))
    }

    fn qr_close_button(&mut self, view: &QrView, cx: &mut Context<Self>) -> Div {
        tagged(
            "peer-qr-close",
            round_button("peer-qr-close", IconName::X, tr(keys::CLOSE), view, cx).on_click(
                cx.listener(|this, _, window, cx| {
                    this.close_qr_box(cx);
                    this.close_kit_dialog_if_done(DialogKind::PeerQr, window, cx);
                }),
            ),
        )
    }

    fn qr_copy_button(&mut self, view: &QrView, cx: &mut Context<Self>) -> Div {
        let busy = self
            .peer_qr
            .open
            .as_ref()
            .is_some_and(|open| open.exporting);
        tagged(
            "peer-qr-copy",
            round_button(
                "peer-qr-copy",
                IconName::Copy,
                tr(keys::GROUP_INVITE_CONTEXT_COPY),
                view,
                cx,
            )
            .loading(busy)
            .on_click(cx.listener(|this, _, window, cx| this.qr_copy(window, cx))),
        )
    }

    fn qr_share_button(&mut self, view: &QrView, cx: &mut Context<Self>) -> Div {
        tagged(
            "peer-qr-share",
            round_button(
                "peer-qr-share",
                IconName::Share2,
                tr(keys::GROUP_INVITE_CONTEXT_SHARE),
                view,
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| this.qr_share(window, cx))),
        )
    }
}

/// `element` under a test selector named like its id.
fn tagged(selector: &'static str, element: impl IntoElement) -> Div {
    div().debug_selector(move || selector.into()).child(element)
}
