//! The composer's AI button (tdesktop compose_ai_button_factory.cpp and
//! history_view_compose_ai_button.cpp): "AI" with a sparkle that blinks
//! out and back in when the button appears, at the top of the attach
//! column, with a one-time hint above it.

use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::{ai_button_visible, prefs};
use quill::lang::{keys, tr};
use std::time::{Duration, Instant};

/// tdesktop `kAnimationDuration`.
const BLINK: Duration = Duration::from_millis(640);

/// The sparkle's opacity `elapsed` after the button appeared. tdesktop
/// blinks two stars out and back in one after the other; the single kit
/// sparkle dims out over the first half and back in over the second.
fn sparkle_opacity(elapsed: Duration, reduce_motion: bool) -> f32 {
    if reduce_motion || elapsed >= BLINK {
        return 1.0;
    }
    let progress = elapsed.as_secs_f32() / BLINK.as_secs_f32();
    (1.0 - 2.0 * progress).abs()
}

/// A hint that stays until it is dismissed (tdesktop `ImportantTooltip`),
/// drawn with the kit tooltip's surface.
pub(super) fn hint_bubble(text: &'static str, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .border_1()
        .border_color(theme.border)
        .shadow_md()
        .rounded(theme.radius)
        .py_1()
        .px_2()
        .text_sm()
        .child(text)
}

impl QuillApp {
    /// tdesktop `HasEnoughLinesForAi` while the recorder is hidden. The kit
    /// field reports no wrapped line count, so lines are the draft's
    /// newline-separated lines.
    fn ai_compose_button_wanted(&self, cx: &App) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        if session.open_chat.is_none() || self.recording_active() {
            return false;
        }
        let text = self.composer.read(cx).value();
        let max = usize::try_from(session.messages.message_text_length_max).unwrap_or(0);
        ai_button_visible(
            &text,
            text.lines().count(),
            !session.messages.ai_compose.styles.is_empty(),
            max,
        )
    }

    /// Start the sparkle's blink when the button appears, forget it when
    /// the button hides (tdesktop restarts the animation on every show).
    pub(super) fn sync_ai_compose_button(&mut self, cx: &App) {
        let wanted = self.ai_compose_button_wanted(cx);
        match (wanted, self.ai_compose.button_shown_at) {
            (true, None) => self.ai_compose.button_shown_at = Some(Instant::now()),
            (false, Some(_)) => self.ai_compose.button_shown_at = None,
            _ => {}
        }
    }

    pub(in crate::ui) fn ai_compose_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shown_at = self.ai_compose.button_shown_at?;
        let reduce_motion = cx.reduce_motion();
        let elapsed = shown_at.elapsed();
        if !reduce_motion && elapsed < BLINK {
            self.request_animation_tick(60, cx);
        }
        let sparkle = cx
            .theme()
            .primary
            .opacity(sparkle_opacity(elapsed, reduce_motion));
        let hint = !prefs::current().tooltip_hidden;
        let button = Button::new("composer-ai")
            .ghost()
            .small()
            .compact()
            .label(tr(keys::Q_AI_COMPOSE_BUTTON))
            .child(
                Icon::new(IconName::Sparkles)
                    .size(px(12.))
                    .text_color(sparkle),
            )
            .accessibility_label(tr(keys::AI_COMPOSE_TITLE))
            .on_click(cx.listener(|this, _, window, cx| this.open_ai_compose(window, cx)));
        Some(
            div()
                .relative()
                .debug_selector(|| "composer-ai".into())
                .when(hint, |wrap| {
                    wrap.child(
                        deferred(
                            anchored()
                                .anchor(Anchor::BottomLeft)
                                .snap_to_window_with_margin(px(8.))
                                .child(
                                    hint_bubble(tr(keys::AI_COMPOSE_TOOLTIP), cx)
                                        .mb_1()
                                        .max_w(px(260.)),
                                ),
                        )
                        .with_priority(1),
                    )
                })
                .child(button)
                .into_any_element(),
        )
    }

    /// The attach button with the AI button above it: tdesktop puts the AI
    /// button at the top of the attach column of a grown field.
    pub(in crate::ui) fn ai_compose_attach_column(
        &self,
        attach: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .self_stretch()
            .items_center()
            .children(self.ai_compose_button(cx))
            .child(div().flex_1())
            .child(attach)
            .into_any_element()
    }
}

#[cfg(test)]
mod sparkle_tests {
    use super::{BLINK, sparkle_opacity};
    use std::time::Duration;

    #[test]
    fn sparkle_blinks_out_and_back_once() {
        assert_eq!(sparkle_opacity(Duration::ZERO, false), 1.0);
        assert!(sparkle_opacity(BLINK / 2, false) < 0.01);
        assert!((sparkle_opacity(BLINK / 4, false) - 0.5).abs() < 0.01);
        assert_eq!(sparkle_opacity(BLINK, false), 1.0);
        assert_eq!(sparkle_opacity(BLINK * 3, false), 1.0);
    }

    #[test]
    fn reduced_motion_keeps_the_sparkle_lit() {
        assert_eq!(sparkle_opacity(BLINK / 2, true), 1.0);
    }
}
