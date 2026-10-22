//! The earn pages' window layer (tdesktop `Info::LayerWidget`): a page
//! opened with `showSection` on a window wider than
//! `LayerWidget::MinimalSupportedWidth` is a centered card over a dimmed
//! window, `infoDesiredWidth` wide, with the 56 px top bar holding the
//! back arrow, the title and the close button. Pages opened from it stack
//! inside it; the info panel it replaced comes back when it closes.

use super::*;
use crate::ui::chat_theme::bg_canvas;
use crate::ui::motion::{Glide, ease_out_circ};
use gpui_kit::assets::IconName;
use quill::lang::{keys, tr};
use std::time::{Duration, Instant};

/// tdesktop `infoDesiredWidth`.
const LAYER_WIDTH: f32 = 392.;
/// tdesktop `infoMinimalLayerMargin`.
const LAYER_MARGIN: f32 = 16.;
/// tdesktop `infoLayerTopMinimal` and `infoLayerTopMaximal`.
const LAYER_TOP_MIN: f32 = 20.;
const LAYER_TOP_MAX: f32 = 40.;
/// tdesktop `infoLayerTopBarHeight`.
const TOP_BAR_HEIGHT: f32 = 56.;
const OPEN_FADE: Duration = Duration::from_millis(160);
const CLOSE_FADE: Duration = Duration::from_millis(110);

/// The layer's top inset for a window `height` tall (tdesktop
/// `LayerWidget::countGeometry`).
pub(super) fn layer_top(height: f32) -> f32 {
    (height / 24.).clamp(LAYER_TOP_MIN, LAYER_TOP_MAX)
}

/// The layer's width in a window `width` wide.
pub(super) fn layer_width(width: f32) -> f32 {
    LAYER_WIDTH.min((width - 2. * LAYER_MARGIN).max(0.))
}

/// The layer's fade, whether it is on its way out, and which close it
/// is (a timer only drops the layer for the close that started it).
pub(crate) struct LayerFade {
    glide: Glide,
    closing: bool,
    close_id: u64,
}

impl QuillApp {
    /// Whether an earn page is open (it is always shown in the layer).
    pub(in crate::ui) fn earn_layer_open(&self) -> bool {
        matches!(
            self.session().and_then(|s| s.users_state.open_info_panel),
            Some(InfoPanelTarget::Earn(_))
        )
    }

    /// Fade the layer in, unless it is already up.
    pub(super) fn show_earn_layer(&mut self) {
        if !self.earn_layer_open() {
            // A layer that was replaced without closing starts afresh.
            self.earn.fade = None;
        }
        let now = Instant::now();
        let animate = self.frame.window_active.get();
        match self.earn.fade.as_mut() {
            Some(fade) if !fade.closing => {}
            Some(fade) => {
                fade.closing = false;
                fade.glide.go(1., OPEN_FADE, animate, now);
            }
            None => {
                let mut glide = Glide::settled(0., now, ease_out_circ);
                glide.go(1., OPEN_FADE, animate, now);
                self.earn.fade = Some(LayerFade {
                    glide,
                    closing: false,
                    close_id: 0,
                });
            }
        }
    }

    /// Put back what the info panel showed before the layer, or nothing.
    /// Needs no window, so the poll loop can call it.
    pub(super) fn drop_earn_layer(&mut self, cx: &mut Context<Self>) {
        self.earn.fade = None;
        self.earn.back.clear();
        let restore = self.earn.restore.take();
        if let Some(live) = self.live.as_mut() {
            live.driver.set_info_panel(restore);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.users_state.open_info_panel = restore;
        }
        cx.notify();
    }

    /// The close button, a click outside, or Escape on the first page:
    /// fade out, then drop the layer.
    pub(in crate::ui) fn close_earn_layer(&mut self, cx: &mut Context<Self>) {
        let animate = self.frame.window_active.get();
        let now = Instant::now();
        let Some(fade) = self.earn.fade.as_mut() else {
            self.drop_earn_layer(cx);
            return;
        };
        if fade.closing {
            return;
        }
        fade.closing = true;
        fade.close_id = fade.close_id.wrapping_add(1);
        let close_id = fade.close_id;
        fade.glide.go(0., CLOSE_FADE, animate, now);
        if !fade.glide.animating(now) {
            self.drop_earn_layer(cx);
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CLOSE_FADE).await;
            this.update(cx, |this, cx| {
                if this
                    .earn
                    .fade
                    .as_ref()
                    .is_some_and(|fade| fade.closing && fade.close_id == close_id)
                {
                    this.drop_earn_layer(cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Escape: back to the page this one was opened from, or close
    /// (tdesktop `WrapWidget::keyPressEvent`).
    pub(in crate::ui) fn earn_layer_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.earn.back.is_empty() {
            self.close_earn_layer(cx);
        } else {
            self.earn_back(window, cx);
        }
    }

    /// The layer over the window, while an earn page is open.
    pub(in crate::ui) fn earn_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Some(InfoPanelTarget::Earn(_)) =
            self.session().and_then(|s| s.users_state.open_info_panel)
        else {
            return None;
        };
        self.earn_layer_body(window, cx)
    }

    /// `earn_layer` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn earn_layer_body(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some(InfoPanelTarget::Earn(panel)) =
            self.session().and_then(|s| s.users_state.open_info_panel)
        else {
            return None;
        };
        let now = Instant::now();
        let shown = self
            .earn
            .fade
            .as_ref()
            .map_or(1., |fade| fade.glide.value(now).clamp(0., 1.));
        if self
            .earn
            .fade
            .as_ref()
            .is_some_and(|fade| fade.glide.animating(now))
        {
            self.request_animation_tick(60, cx);
        }
        let viewport = window.viewport_size();
        let top = layer_top(f32::from(viewport.height));
        let width = layer_width(f32::from(viewport.width));
        let max_height = (viewport.height - px(2. * top)).max(px(TOP_BAR_HEIGHT));
        let mut bar = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .h(px(TOP_BAR_HEIGHT))
            .px_2();
        if !self.earn.back.is_empty() {
            bar = bar.child(
                Button::new("earn-back")
                    .debug_selector(|| "earn-back".into())
                    .accessibility_label(tr(keys::Q_MONEY_BACK))
                    .icon(IconName::ArrowLeft)
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| this.earn_back(window, cx))),
            );
        } else {
            bar = bar.child(div().w_2());
        }
        bar = bar
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_lg()
                    .font_semibold()
                    .child(panel.title()),
            )
            .child(
                Button::new("earn-layer-close")
                    .debug_selector(|| "earn-layer-close".into())
                    .accessibility_label(tr(keys::CLOSE))
                    .icon(IconName::X)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_earn_layer(cx))),
            );
        let card = div()
            .id("earn-layer-card")
            .debug_selector(|| "earn-layer".into())
            .absolute()
            .top(px(top))
            .left((viewport.width - px(width)) / 2.)
            .opacity(shown)
            .w(px(width))
            .max_h(max_height)
            .flex()
            .flex_col()
            .rounded_xl()
            .border_1()
            .border_color(cx.theme().border)
            .bg(bg_canvas())
            .shadow_lg()
            .overflow_hidden()
            .role(gpui_kit::Role::Dialog)
            .aria_label(panel.title())
            // Clicks inside the card never reach the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(bar)
            .children(self.earn_panel(panel, cx));
        Some(
            div()
                .id("earn-layer")
                .occlude()
                .absolute()
                .inset_0()
                .bg(gpui_kit::black().opacity(0.45 * shown))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.close_earn_layer(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .child(card)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod unit {
    use super::{layer_top, layer_width};

    #[test]
    fn the_layer_follows_tdesktop_geometry() {
        assert_eq!(layer_top(480.), 20.);
        assert_eq!(layer_top(720.), 30.);
        assert_eq!(layer_top(1200.), 40.);
        assert_eq!(layer_width(1280.), 392.);
        assert_eq!(layer_width(400.), 368.);
        assert_eq!(layer_width(10.), 0.);
    }
}
