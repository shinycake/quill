//! Profile layer opened by clicking a sender's avatar in a group.
//!
//! Telegram Desktop's userpic click is `Element::fromLink` ->
//! `showPeerInfo(from)` -> `Info::Memento`, whose `createLayer` yields an
//! `Info::LayerWidget` (a dimmed modal layer over the window) whenever the
//! window is wide enough for it, and a main-column section otherwise.
//! Quill's windows are always layer-wide, so the same profile content the
//! right-hand info panel shows (`info_panel_parts`) is presented as a
//! centered modal here: Escape or a click outside closes it.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::bg_canvas;
use super::motion::{Glide, ease_out_circ};
use crate::ui::dismiss_layer::DismissLayer;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
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
    /// What the info column showed before the modal took it over: the
    /// modal's close puts it back.
    previous: Option<InfoPanelTarget>,
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
        // Only the main window draws the modal (chat windows show no info
        // panel): a click in a chat window opens it there, brought forward,
        // and its search field is built with the main window.
        if self.hand_to_main_window(cx, move |app, window, cx| {
            app.open_avatar_profile(sender, window, cx);
        }) {
            return;
        }
        let Some(target) = self
            .session()
            .and_then(|session| session.avatar_profile_target(sender))
        else {
            return;
        };
        let now = Instant::now();
        let mut glide = Glide::settled(0., now, ease_out_circ);
        glide.go(1., OPEN_FADE, self.frame.window_active.get(), now);
        // A modal opened from inside another keeps the column it found.
        let previous = match self.dialogs.profile_modal.take() {
            Some(modal) => modal.previous,
            None => self
                .session()
                .and_then(|session| session.users_state.open_info_panel),
        };
        self.dialogs.profile_modal = Some(ProfileModal {
            target,
            previous,
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
            let previous = modal.previous;
            self.dialogs.profile_modal = None;
            self.restore_info_panel(previous, cx);
            return;
        }
        let target = modal.target;
        self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(CLOSE_FADE).await;
            this.update(cx, |this, cx| {
                if let Some(modal) = this
                    .dialogs
                    .profile_modal
                    .take_if(|modal| modal.closing && modal.target == target)
                {
                    this.restore_info_panel(modal.previous, cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The modal's close: the info column goes back to what it showed
    /// before (closed, or the chat's own info), not closed whatever it was.
    fn restore_info_panel(&mut self, previous: Option<InfoPanelTarget>, cx: &mut Context<Self>) {
        let Some(previous) = previous else {
            self.close_info_panel(cx);
            return;
        };
        let Some(main) = self.main_window.handle else {
            self.close_info_panel(cx);
            return;
        };
        let screen = self.screen_epoch();
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = app.update(cx, |this, cx| {
                    this.in_screen_scope(screen, cx, |this, cx| {
                        this.in_window_scope(None, cx, |this, cx| {
                            this.open_info_panel_target(previous, window, cx);
                        });
                    });
                });
            });
        });
    }

    /// The main window's chat changed under the modal: the modal belonged
    /// to the old chat's member and goes, and the info column is what it
    /// was before the modal, so the chat open's "the column follows the
    /// chat" sees a column only when the user had one open.
    pub(super) fn profile_modal_leaves_chat(&mut self, cx: &mut Context<Self>) {
        if self.windows.scope.is_some() {
            return;
        }
        self.dismiss_profile_modal();
        cx.notify();
    }

    /// A button inside the modal navigated elsewhere (Message, a group in
    /// common, Mute's Sound..., Share): the modal goes and the info column
    /// is what it was before the modal, at once, so what the button opens
    /// next (a chat whose column follows the old one) finds the user's
    /// column, not the member's profile. Also on a switch: the column is
    /// the session's and comes back with the account.
    pub(super) fn dismiss_profile_modal(&mut self) {
        let Some(modal) = self.dialogs.profile_modal.take() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.set_info_panel(modal.previous);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.users_state.open_info_panel = modal.previous;
        }
        if modal.previous.is_none() {
            self.common_groups.close();
        }
    }

    /// Another info target opens while the modal is up (a member row inside
    /// a group's modal, Statistics): it takes the column, and the modal,
    /// which shows only its own target, goes instead of coming back later
    /// with a stale column to give back.
    pub(super) fn profile_modal_replaced_by(&mut self, target: InfoPanelTarget) {
        self.dialogs
            .profile_modal
            .take_if(|modal| modal.target != target);
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
        self.profile_modal_overlay_body(window, cx)
    }

    /// `profile_modal_overlay` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn profile_modal_overlay_body(
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
                            .accessibility_label(tr(keys::Q_PEOPLE_CLOSE_PROFILE))
                            .icon(IconName::X)
                            .ghost()
                            .tooltip(tr(keys::CLOSE))
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
            DismissLayer::new(
                "profile-modal",
                cx.listener(|this, _, _, cx| this.close_profile_modal(cx)),
            )
            .layer(|layer| {
                layer
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui_kit::black().opacity(0.45 * shown))
            })
            .content(card)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "profile_modal_tests.rs"]
mod profile_modal_tests;
