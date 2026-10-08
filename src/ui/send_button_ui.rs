//! The composer's round Send / Record / Save / Schedule / slow-mode button,
//! after Telegram Desktop's `Ui::SendButton`: one 32px button whose glyph
//! cross-fades and scales over 120 ms when the state changes, and which
//! shows the remaining slow-mode time as "m:ss" instead of an icon.
//!
//! The state choice and the timeline live in `quill::send_button` (unit
//! tested); this file only draws them. The fade is driven by a timestamp and
//! `request_animation_tick`, which stops asking once the fade has settled,
//! so an idle composer causes no redraws.

use super::app::QuillApp;
use super::recording::RecordMode;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::send_button::{
    SendButtonInputs, SendButtonKind, SendMorph, format_slowmode, morph_layers, send_button_kind,
};
use std::sync::OnceLock;
use std::time::Instant;

/// Diameter of the round button.
const BUTTON_SIZE: f32 = 32.;
/// Glyph size at full scale.
const GLYPH_SIZE: f32 = 16.;

/// Milliseconds on a monotonic clock shared by every caller.
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

impl QuillApp {
    /// What the button should be right now.
    pub(super) fn send_button_kind_now(&self, can_record: bool, sendable: bool) -> SendButtonKind {
        send_button_kind(SendButtonInputs {
            editing: self.pending_edit.is_some(),
            sendable,
            can_record,
            record_video: self.record_mode() == RecordMode::Video,
            scheduled: !matches!(
                self.composer_scheduling,
                quill::composer::ComposerScheduling::None
            ),
            slow_mode_wait: self.slow_mode_wait_secs(),
        })
    }

    /// Same guard as Enter-to-send: text, or attachments without a caption.
    fn submit_from_send_button(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        if !text.trim().is_empty() || !self.pending_attachments.is_empty() {
            self.submit_composer(text, window, cx);
        }
    }

    /// The button for the composer's input row.
    pub(super) fn composer_send_button(
        &self,
        can_record: bool,
        sendable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = self.send_button_kind_now(can_record, sendable);
        let wait = self.slow_mode_wait_secs();

        let now = now_ms();
        let mut morph = self.send_morph.get().unwrap_or(SendMorph::new(kind));
        morph.set(kind, now);
        self.send_morph.set(Some(morph));
        let progress = morph.progress(now);
        let animating = morph.is_animating(now);
        if animating {
            self.request_animation_tick(60, cx);
        }

        let theme = cx.theme();
        let primary = theme.primary;
        let fill = |k: SendButtonKind| if k.filled() { 1.0_f32 } else { 0.0 };
        let fill_alpha =
            fill(morph.source()) + (fill(morph.target()) - fill(morph.source())) * progress;
        let (hover, active) = if kind.filled() {
            (theme.primary_hover, theme.primary_active)
        } else {
            (theme.accent, theme.accent)
        };
        let variant = ButtonCustomVariant::new(cx)
            .color(primary.opacity(fill_alpha))
            .foreground(if kind.filled() {
                theme.primary_foreground
            } else {
                theme.secondary_foreground
            })
            .hover(hover)
            .active(active);

        // The glyphs: during the fade both are drawn on top of each other.
        let (outgoing, incoming) = morph_layers(progress);
        let glyphs = div()
            .relative()
            .size_full()
            .when(animating, |glyphs| {
                glyphs.child(glyph_layer(morph.source(), wait, outgoing, cx))
            })
            .child(glyph_layer(morph.target(), wait, incoming, cx));

        let button = Button::new("composer-send")
            // `custom` washes its fill out 20%, so only the fade itself uses
            // it; settled states use the kit's own solid variants.
            .when(animating, |b| b.custom(variant))
            .when(!animating && kind.filled(), |b| b.primary())
            .when(!animating && !kind.filled(), |b| b.ghost())
            .rounded_full()
            .w(px(BUTTON_SIZE))
            .h(px(BUTTON_SIZE))
            .p_0()
            .tooltip(kind.tooltip(wait))
            .accessibility_label(match kind {
                SendButtonKind::Send => "Send message",
                SendButtonKind::Save => "Save message",
                SendButtonKind::Schedule => "Schedule message",
                SendButtonKind::Slowmode => "Slow mode",
                SendButtonKind::Record { .. } => "Record",
            })
            .child(glyphs)
            .on_click(cx.listener(move |this, _, window, cx| {
                if kind.submits() {
                    this.submit_from_send_button(window, cx);
                } else {
                    this.start_recording(cx);
                }
            }));

        if kind.has_send_menu() {
            // Right-click: send options (silent, schedule, link preview).
            let owner = cx.entity().downgrade();
            div()
                .id("composer-send-wrap")
                .context_menu(move |menu, _, cx| {
                    QuillApp::send_options_menu(owner.clone(), menu, cx)
                })
                .child(button)
                .into_any_element()
        } else if !kind.submits() {
            // MED2: right-click flips audio/video mode (TGX tap-to-switch,
            // desktop-mapped).
            div()
                .id("record-mode-wrap")
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| this.toggle_record_mode(cx)),
                )
                .child(button)
                .into_any_element()
        } else {
            // Save and the slow-mode counter have no options menu.
            div()
                .id("composer-send-wrap")
                .child(button)
                .into_any_element()
        }
    }
}

/// One glyph centered in the button, at `(opacity, scale)`.
fn glyph_layer(
    kind: SendButtonKind,
    wait: Option<u64>,
    (opacity, scale): (f32, f32),
    cx: &App,
) -> impl IntoElement {
    let layer = || {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .opacity(opacity)
    };
    let size = px(GLYPH_SIZE * scale);
    let color = if kind.filled() {
        cx.theme().primary_foreground
    } else {
        cx.theme().secondary_foreground
    };
    let icon = |name| layer().child(Icon::new(name).size(size).text_color(color));
    match kind {
        SendButtonKind::Send => icon(gpui_kit::assets::IconName::Send),
        SendButtonKind::Save => icon(gpui_kit::assets::IconName::Check),
        SendButtonKind::Schedule => icon(gpui_kit::assets::IconName::Clock),
        SendButtonKind::Record { video: false } => icon(gpui_kit::assets::IconName::Mic),
        SendButtonKind::Record { video: true } => icon(gpui_kit::assets::IconName::Video),
        SendButtonKind::Slowmode => layer().child(
            div()
                .text_size(px(11. * scale))
                .text_color(color)
                .child(format_slowmode(wait.unwrap_or(0))),
        ),
    }
}
