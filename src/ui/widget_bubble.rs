//! A widget message's card in its bubble, drawn from `WidgetsUi` alone:
//! the runtime's last frame at its CSS size, a placeholder of the height
//! it will take until the first frame, the "Widget stopped" pill over the
//! last frame once it stopped, and the fallback card when it never drew.
//! The mouse over it goes to the runtime (`widgets_ui.rs`); the wheel
//! scrolls the history, and a right-click opens the message menu unless
//! the widget holds the press for a menu of its own.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::widget_runtime::Input;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::message_widget::{Lifecycle, MAX_CRASHES, WidgetKey};
use quill::state::HistoryMessage;
use quill::telegram::envelope::MessageContent;

/// What the card shows.
enum Look {
    Placeholder,
    Frame { stopped: bool },
    Fallback,
}

impl QuillApp {
    /// The card of the widget message `message` in a bubble whose pictures
    /// are at most `media_max` CSS px wide; `None` for other messages.
    pub(super) fn widget_bubble(
        &self,
        message: &HistoryMessage,
        media_max: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let MessageContent::Widget(content) = &message.content else {
            return None;
        };
        let key = WidgetKey {
            chat_id: message.chat_id,
            message_id: message.id,
        };
        let width = Self::widget_card_width(content, media_max);
        self.note_widget(key, width);
        let entry = self.widgets.entries.get(&key);
        let id = message.id.0 as u64;
        let look = match entry {
            None => Look::Placeholder,
            Some(entry) => match (&entry.lifecycle, entry.frame.is_some()) {
                (Lifecycle::Failed { .. }, _) => Look::Fallback,
                (Lifecycle::Stopped { .. }, true) => Look::Frame {
                    stopped: entry.crashes >= MAX_CRASHES || entry.runtime.is_none(),
                },
                (Lifecycle::Stopped { .. }, false) => Look::Fallback,
                (_, true) => Look::Frame { stopped: false },
                (_, false) => Look::Placeholder,
            },
        };
        let height = entry.map_or_else(
            || quill::message_widget::initial_height(content.height, None),
            |entry| entry.height,
        );
        let card = match (look, entry.and_then(|entry| entry.frame.as_ref())) {
            (Look::Frame { stopped }, Some((image, w, h))) => div()
                .id(("widget-frame", id))
                .relative()
                .w(px(*w))
                .h(px(*h))
                .overflow_hidden()
                .child(
                    img(ImageSource::Render((**image).clone()))
                        .w(px(*w))
                        .h(px(*h)),
                )
                .when(!stopped, |card| {
                    card.child(self.widget_input_layer(key, entry.map(|e| e.cursor.as_str()), cx))
                })
                .when(stopped, |card| card.child(stopped_pill(cx)))
                .into_any_element(),
            (Look::Fallback, _) => fallback_card(id),
            _ => div()
                .id(("widget-placeholder", id))
                .w(px(width))
                .h(px(height))
                .role(Role::Label)
                .aria_label(tr(keys::IN_DLG_WIDGET))
                .into_any_element(),
        };
        Some(card)
    }

    /// The layer over a running widget's frame that hands it the mouse.
    fn widget_input_layer(
        &self,
        key: WidgetKey,
        cursor: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cursor = cursor.and_then(cursor_style);
        let app = cx.weak_entity();
        canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, _| {
                if let Some(cursor) = cursor {
                    window.set_cursor_style(cursor, &hitbox);
                }
                let at = move |position: Point<Pixels>| {
                    (
                        f64::from(f32::from(position.x - bounds.origin.x)),
                        f64::from(f32::from(position.y - bounds.origin.y)),
                    )
                };
                let down = (app.clone(), hitbox.clone());
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || !down.1.is_hovered(window) {
                        return;
                    }
                    let (x, y) = at(event.position);
                    let modifiers = modifier_bits(event.modifiers);
                    let mut taken = false;
                    let _ = down.0.update(cx, |this, _| {
                        taken = this.widget_pointer_down(key, event.button, x, y, modifiers);
                    });
                    if taken {
                        cx.stop_propagation();
                    }
                });
                let moved = (app.clone(), hitbox.clone());
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let (x, y) = at(event.position);
                    let over = moved.1.is_hovered(window);
                    let modifiers = modifier_bits(event.modifiers);
                    let _ = moved.0.update(cx, |this, _| {
                        this.widget_pointer_move(key, over, x, y, modifiers)
                    });
                });
                let up = app.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let (x, y) = at(event.position);
                    let modifiers = modifier_bits(event.modifiers);
                    let button = event.button;
                    let _ = up.update(cx, |this, _| {
                        this.widget_pointer_up(key, button, x, y, modifiers)
                    });
                });
            },
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    /// A press on a widget: the primary button always goes to it, the
    /// secondary only while it holds presses. True when the press is the
    /// widget's alone.
    fn widget_pointer_down(
        &mut self,
        key: WidgetKey,
        button: MouseButton,
        x: f64,
        y: f64,
        modifiers: i32,
    ) -> bool {
        let Some(entry) = self.widgets.entries.get_mut(&key) else {
            return false;
        };
        let Some(runtime) = &entry.runtime else {
            return false;
        };
        let button = match button {
            MouseButton::Left => 0,
            MouseButton::Right if entry.hold => 2,
            _ => return false,
        };
        entry.pressed = Some(button);
        runtime.input(Input::Pointer {
            action: 0,
            x,
            y,
            modifiers,
            button,
        });
        button == 2
    }

    fn widget_pointer_move(&mut self, key: WidgetKey, over: bool, x: f64, y: f64, modifiers: i32) {
        let Some(entry) = self.widgets.entries.get_mut(&key) else {
            return;
        };
        let Some(runtime) = &entry.runtime else {
            return;
        };
        if let Some(button) = entry.pressed {
            runtime.input(Input::Pointer {
                action: 1,
                x,
                y,
                modifiers,
                button,
            });
        } else if over {
            entry.hovered = true;
            runtime.input(Input::Hover {
                action: 0,
                x,
                y,
                modifiers,
            });
        } else if std::mem::take(&mut entry.hovered) {
            runtime.input(Input::Hover {
                action: 1,
                x,
                y,
                modifiers,
            });
        }
    }

    /// The release of the press the widget took, whichever button it was.
    fn widget_pointer_up(
        &mut self,
        key: WidgetKey,
        button: MouseButton,
        x: f64,
        y: f64,
        modifiers: i32,
    ) {
        let Some(entry) = self.widgets.entries.get_mut(&key) else {
            return;
        };
        let Some(runtime) = &entry.runtime else {
            return;
        };
        let released = match button {
            MouseButton::Left => 0,
            MouseButton::Right => 2,
            _ => return,
        };
        if entry.pressed != Some(released) {
            return;
        }
        entry.pressed = None;
        runtime.input(Input::Pointer {
            action: 2,
            x,
            y,
            modifiers,
            button: released,
        });
        if released == 0 {
            // What the press selected, for the message menu's copy.
            runtime.selection(false);
        }
    }
}

/// twidget's modifier bits: alt 1, ctrl 2, meta 4, shift 8.
fn modifier_bits(modifiers: Modifiers) -> i32 {
    i32::from(modifiers.alt)
        | i32::from(modifiers.control) << 1
        | i32::from(modifiers.platform) << 2
        | i32::from(modifiers.shift) << 3
}

/// A CSS cursor as GPUI's; `None` leaves the history's.
fn cursor_style(css: &str) -> Option<CursorStyle> {
    Some(match css {
        "pointer" => CursorStyle::PointingHand,
        "text" => CursorStyle::IBeam,
        "vertical-text" => CursorStyle::IBeamCursorForVerticalLayout,
        "crosshair" => CursorStyle::Crosshair,
        "grab" => CursorStyle::OpenHand,
        "grabbing" => CursorStyle::ClosedHand,
        "not-allowed" | "no-drop" => CursorStyle::OperationNotAllowed,
        "ew-resize" | "e-resize" | "w-resize" => CursorStyle::ResizeLeftRight,
        "ns-resize" | "n-resize" | "s-resize" => CursorStyle::ResizeUpDown,
        "col-resize" => CursorStyle::ResizeColumn,
        "row-resize" => CursorStyle::ResizeRow,
        "nwse-resize" => CursorStyle::ResizeUpLeftDownRight,
        "nesw-resize" => CursorStyle::ResizeUpRightDownLeft,
        "default" => CursorStyle::Arrow,
        _ => return None,
    })
}

/// "Widget stopped", centered over the last frame.
fn stopped_pill(cx: &App) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .px_3()
                .py_1()
                .rounded_full()
                .bg(cx.theme().secondary.opacity(0.85))
                .text_color(cx.theme().secondary_foreground)
                .text_sm()
                .child(tr(keys::TGW_STOPPED)),
        )
        .into_any_element()
}

/// The card of a widget that never drew: its name and that it could not start.
fn fallback_card(id: u64) -> AnyElement {
    div()
        .id(("widget-card", id))
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .role(Role::Label)
        .aria_label(tr(keys::Q_CHAT_WIDGET_FAILED))
        .child(div().font_semibold().child(tr(keys::IN_DLG_WIDGET)))
        .child(
            div()
                .text_sm()
                .opacity(0.8)
                .child(tr(keys::Q_CHAT_WIDGET_FAILED)),
        )
        .into_any_element()
}
