//! The earn pages' window layer (tdesktop `Info::LayerWidget`): a page
//! opened with `showSection` on a window wider than
//! `LayerWidget::MinimalSupportedWidth` is a centered card over a dimmed
//! window, `infoDesiredWidth` wide, with the 56 px top bar holding the
//! back arrow, the title and the close button. Pages opened from it stack
//! inside it; the info panel it replaced comes back when it closes.

use super::*;
use crate::ui::chat_theme::bg_canvas;
use crate::ui::dismiss_layer::DismissLayer;
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

/// The layer's fade in and out.
pub(crate) struct LayerFade {
    glide: Glide,
    stage: FadeStage,
}

enum FadeStage {
    Shown,
    /// Fading out; the timer that drops the layer is owned by this stage,
    /// so reopening or dropping the layer cancels it.
    Closing {
        _timer: Task<()>,
    },
}

impl LayerFade {
    fn fresh(animate: bool, now: Instant) -> Self {
        let mut glide = Glide::settled(0., now, ease_out_circ);
        glide.go(1., OPEN_FADE, animate, now);
        Self {
            glide,
            stage: FadeStage::Shown,
        }
    }

    fn glide(&self) -> &Glide {
        &self.glide
    }

    fn is_closing(&self) -> bool {
        matches!(self.stage, FadeStage::Closing { .. })
    }

    /// Fade back in (the layer was reopened while it faded out).
    fn show(&mut self, animate: bool, now: Instant) {
        if self.is_closing() {
            self.glide.go(1., OPEN_FADE, animate, now);
            self.stage = FadeStage::Shown;
        }
    }
}

/// What the earn layer keeps while a page is open: the pages it stacked,
/// the info panel it covered, its fade and the page whose first send was
/// refused. Whether the layer is open is the session's info column
/// (`earn_layer_open`), never a second copy here; leaving the layer or
/// opening it over another panel replaces the whole value, so none of
/// these outlives the page that set it.
pub(crate) struct EarnLayer {
    /// The earn pages the open one was opened from (tdesktop's back
    /// arrow).
    back: Vec<InfoPanelTarget>,
    /// The info panel the layer covered; it comes back on close.
    restore: Option<InfoPanelTarget>,
    fade: LayerFade,
    /// The page whose load was refused before it left (the client
    /// could not send), shown with a Retry. The next open replaces it.
    refused: Option<EarnPanel>,
}

impl EarnLayer {
    /// A layer with nothing stacked, covering nothing: what no page
    /// is open leaves.
    pub(crate) fn idle() -> Self {
        Self::covering(None, false, Instant::now())
    }

    fn covering(restore: Option<InfoPanelTarget>, animate: bool, now: Instant) -> Self {
        Self {
            back: Vec::new(),
            restore,
            fade: LayerFade::fresh(animate, now),
            refused: None,
        }
    }

    /// A page opens in the layer while the info panel shows `current`.
    /// Pages opened from another earn page stack for the back arrow; any
    /// other panel is what the layer covers and gives back.
    fn enter(
        &mut self,
        current: Option<InfoPanelTarget>,
        panel: EarnPanel,
        animate: bool,
        now: Instant,
    ) {
        match current {
            Some(InfoPanelTarget::Earn(open)) => {
                if open != panel {
                    self.back.push(InfoPanelTarget::Earn(open));
                }
                self.fade.show(animate, now);
            }
            // A layer replaced without closing starts afresh.
            other => *self = Self::covering(other, animate, now),
        }
        self.refused = None;
    }

    pub(super) fn back_is_empty(&self) -> bool {
        self.back.is_empty()
    }

    pub(super) fn back_pop(&mut self) -> Option<InfoPanelTarget> {
        self.back.pop()
    }

    /// Leave the layer, giving back the info panel it covered.
    fn leave(&mut self) -> Option<InfoPanelTarget> {
        std::mem::replace(self, Self::idle()).restore
    }

    /// Whether `panel`'s first send was refused.
    pub(super) fn refused(&self, panel: EarnPanel) -> bool {
        self.refused == Some(panel)
    }

    /// `panel`'s first send was refused: its page shows the error.
    pub(super) fn refuse(&mut self, panel: EarnPanel) {
        self.refused = Some(panel);
    }

    /// A retry of the refused page starts over.
    pub(super) fn retry(&mut self) {
        self.refused = None;
    }
}

impl QuillApp {
    /// Whether an earn page is open (it is always shown in the layer).
    pub(in crate::ui) fn earn_layer_open(&self) -> bool {
        matches!(
            self.session().and_then(|s| s.users_state.open_info_panel),
            Some(InfoPanelTarget::Earn(_))
        )
    }

    /// Open `panel` in the layer: fade in unless it is already up.
    pub(super) fn enter_earn_layer(&mut self, panel: EarnPanel) {
        let current = self.session().and_then(|s| s.users_state.open_info_panel);
        let animate = self.frame.window_active.get();
        self.earn
            .layer
            .enter(current, panel, animate, Instant::now());
    }

    /// Put back what the info panel showed before the layer, or nothing.
    /// Needs no window, so the poll loop can call it.
    pub(in crate::ui) fn drop_earn_layer(&mut self, cx: &mut Context<Self>) {
        let restore = self.earn.layer.leave();
        if let Some(earn) = self.earn_mut() {
            earn.recipients.drop_parked();
        }
        if let Some(live) = self.live.as_mut() {
            // A withdrawal still checking has no box to cancel it from.
            live.driver.cancel_withdrawal();
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
        if !self.earn_layer_open() {
            self.drop_earn_layer(cx);
            return;
        }
        let fade = &mut self.earn.layer.fade;
        if fade.is_closing() {
            return;
        }
        fade.glide.go(0., CLOSE_FADE, animate, now);
        if !fade.glide.animating(now) {
            self.drop_earn_layer(cx);
            return;
        }
        let timer = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(CLOSE_FADE).await;
            this.update(cx, |this, cx| this.drop_earn_layer(cx)).ok();
        });
        self.earn.layer.fade.stage = FadeStage::Closing { _timer: timer };
        cx.notify();
    }

    /// Escape: back to the page this one was opened from, or close
    /// (tdesktop `WrapWidget::keyPressEvent`).
    pub(in crate::ui) fn earn_layer_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.earn.layer.back_is_empty() {
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
        let fade = &self.earn.layer.fade;
        let shown = fade.glide().value(now).clamp(0., 1.);
        if fade.glide().animating(now) {
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
        if !self.earn.layer.back_is_empty() {
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
            .child(bar)
            .children(self.earn_panel(panel, cx));
        Some(
            DismissLayer::new(
                "earn-layer",
                cx.listener(|this, _, _, cx| this.close_earn_layer(cx)),
            )
            .layer(|layer| layer.bg(gpui_kit::black().opacity(0.45 * shown)))
            .content(card)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod unit {
    use super::{EarnLayer, layer_top, layer_width};
    use quill::earn::{EarnPanel, EarnPeer};
    use quill::state::InfoPanelTarget;
    use std::time::Instant;

    fn setup(bot: i64) -> EarnPanel {
        EarnPanel::Setup { bot_user_id: bot }
    }

    #[test]
    fn the_layer_follows_tdesktop_geometry() {
        assert_eq!(layer_top(480.), 20.);
        assert_eq!(layer_top(720.), 30.);
        assert_eq!(layer_top(1200.), 40.);
        assert_eq!(layer_width(1280.), 392.);
        assert_eq!(layer_width(400.), 368.);
        assert_eq!(layer_width(10.), 0.);
    }

    #[test]
    fn a_page_opened_from_a_page_stacks_and_a_close_gives_the_panel_back() {
        let now = Instant::now();
        let mut layer = EarnLayer::idle();
        let covered = Some(InfoPanelTarget::User(7));
        layer.enter(covered, setup(1), false, now);
        assert!(layer.back_is_empty());
        let join = EarnPanel::Join {
            peer: EarnPeer::User(2),
        };
        layer.enter(Some(InfoPanelTarget::Earn(setup(1))), join, false, now);
        assert_eq!(layer.back_pop(), Some(InfoPanelTarget::Earn(setup(1))));
        assert!(layer.back_is_empty());
        assert_eq!(layer.leave(), covered);
        assert!(layer.back_is_empty(), "nothing is left of a closed layer");
        assert_eq!(layer.leave(), None);
    }

    #[test]
    fn a_layer_replaced_without_closing_starts_afresh() {
        let now = Instant::now();
        let mut layer = EarnLayer::idle();
        layer.enter(Some(InfoPanelTarget::User(7)), setup(1), false, now);
        layer.enter(Some(InfoPanelTarget::Earn(setup(1))), setup(2), false, now);
        assert!(!layer.back_is_empty());
        // The info panel moved to a chat and an earn page opens again.
        layer.enter(Some(InfoPanelTarget::User(9)), setup(3), false, now);
        assert!(layer.back_is_empty(), "the old stack is gone");
        assert_eq!(layer.leave(), Some(InfoPanelTarget::User(9)));
    }

    #[test]
    fn a_refused_page_is_the_pages_own_and_the_next_open_clears_it() {
        let now = Instant::now();
        let mut layer = EarnLayer::idle();
        layer.enter(None, setup(1), false, now);
        layer.refuse(setup(1));
        assert!(layer.refused(setup(1)));
        assert!(!layer.refused(setup(2)), "another page is not refused");
        layer.retry();
        assert!(!layer.refused(setup(1)));
        layer.refuse(setup(1));
        layer.enter(Some(InfoPanelTarget::Earn(setup(1))), setup(1), false, now);
        assert!(!layer.refused(setup(1)), "a fresh open asks again");
    }
}
