//! Profile layer opened by clicking a sender's avatar in a group.
//!
//! Telegram Desktop's userpic click is `Element::fromLink` ->
//! `showPeerInfo(from)` -> `Info::Memento`, whose `createLayer` yields an
//! `Info::LayerWidget` (a dimmed modal layer over the window) whenever the
//! window is wide enough for it, and a main-column section otherwise.
//! Quill's windows are always layer-wide, so the same profile content the
//! right-hand info panel shows (`info_panel_parts`) is presented as a
//! centered modal here: Escape or a click outside closes it.

use super::app::QuillApp;
use super::chat_theme::bg_canvas;
use super::motion::{Glide, ease_out_circ};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::MessageSender;
use std::time::{Duration, Instant};

const MODAL_WIDTH: f32 = 380.;
const OPEN_FADE: Duration = Duration::from_millis(160);
const CLOSE_FADE: Duration = Duration::from_millis(110);
/// How far the card rises while fading in.
const RISE: f32 = 14.;

/// The open profile modal: which panel it shows and its fade.
pub(super) struct ProfileModal {
    pub(super) target: InfoPanelTarget,
    /// 0 = hidden, 1 = shown.
    glide: Glide,
    closing: bool,
}

impl QuillApp {
    /// Avatar click: open the sender's profile as a modal; senders with
    /// no profile (unknown chat) do nothing, like tdesktop.
    pub(super) fn open_avatar_profile(
        &mut self,
        sender: MessageSender,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .session()
            .and_then(|session| session.avatar_profile_target(sender))
        else {
            return;
        };
        let now = Instant::now();
        let mut glide = Glide::settled(0., now, ease_out_circ);
        glide.go(1., OPEN_FADE, self.frame.window_active.get(), now);
        self.dialogs.profile_modal = Some(ProfileModal {
            target,
            glide,
            closing: false,
        });
        self.open_info_panel_target(target, window, cx);
    }

    /// Whether the modal is the presenter of the open info panel.
    pub(super) fn profile_modal_active(&self) -> bool {
        self.dialogs.profile_modal.as_ref().is_some_and(|modal| {
            self.session()
                .is_some_and(|session| session.users_state.open_info_panel == Some(modal.target))
        })
    }

    /// Fade the modal out, then close its panel (snaps when the window is
    /// in the background).
    pub(super) fn close_profile_modal(&mut self, cx: &mut Context<Self>) {
        let animate = self.frame.window_active.get();
        let Some(modal) = self.dialogs.profile_modal.as_mut() else {
            return;
        };
        if modal.closing {
            return;
        }
        let now = Instant::now();
        modal.closing = true;
        modal.glide.go(0., CLOSE_FADE, animate, now);
        if !animate || !modal.glide.animating(now) {
            self.dialogs.profile_modal = None;
            self.close_info_panel(cx);
            return;
        }
        let target = modal.target;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CLOSE_FADE).await;
            this.update(cx, |this, cx| {
                if this
                    .dialogs
                    .profile_modal
                    .as_ref()
                    .is_some_and(|modal| modal.closing && modal.target == target)
                {
                    this.dialogs.profile_modal = None;
                    this.close_info_panel(cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Drop the modal without touching the info panel (a button inside it
    /// navigated elsewhere).
    pub(super) fn dismiss_profile_modal(&mut self) {
        self.dialogs.profile_modal = None;
    }

    /// The modal layer, above the window content.
    pub(super) fn profile_modal_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.profile_modal_active() {
            return None;
        }
        let modal = self.dialogs.profile_modal.as_ref()?;
        let now = Instant::now();
        let shown = modal.glide.value(now).clamp(0., 1.);
        if modal.glide.animating(now) {
            self.request_animation_tick(60, cx);
        }
        let (title, content) = self.info_panel_parts(cx)?;
        let max_height = (window.viewport_size().height * 0.86).max(px(240.));
        let card = div()
            .id("profile-modal-card")
            .relative()
            .top(px((1. - shown) * RISE))
            .opacity(shown)
            .w(px(MODAL_WIDTH))
            .max_h(max_height)
            .flex()
            .flex_col()
            .rounded_xl()
            .border_1()
            .border_color(cx.theme().border)
            .bg(bg_canvas())
            .shadow_lg()
            .overflow_hidden()
            // Clicks inside the card never reach the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().font_semibold().child(title))
                    .child(
                        Button::new("profile-modal-close")
                            .accessibility_label("Close profile")
                            .icon(IconName::X)
                            .ghost()
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_profile_modal(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .id("profile-modal-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            );
        Some(
            div()
                .id("profile-modal")
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui_kit::black().opacity(0.45 * shown))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.close_profile_modal(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .child(card)
                .into_any_element(),
        )
    }
}
