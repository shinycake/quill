//! The tuning panel's live preview: the glass Quill really draws, over a
//! loud scene (`preview_scene`) with two message bubbles, each piece built
//! the way the app builds it: the chat list's segmented tabs with their glass selection,
//! the floating date pill, a message context menu (`menu_surface`), the
//! emoji bar over the composer, the jump-to-latest button, and one empty
//! slab for reference. Each takes the glass the app draws now, so a dial
//! shows its effect on the real surfaces before the person leaves Settings.
//! Each piece can be dragged over the scene (`preview_drag`).
#![deny(clippy::indexing_slicing)]

use super::preview_drag::PreviewPiece;
use super::{GlassRole, GlassSurface as _};
use crate::ui::QuillApp;
use crate::ui::chat_theme::{
    accent_strong, bg_bubble_incoming, danger_bright, text_menu, text_on_fill, text_primary,
};
use crate::ui::menu_surface::MenuSurface as _;
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};

const HEIGHT: f32 = 380.0;
/// The emoji bar's suggestions.
const EMOJI: [&str; 7] = ["👍", "❤️", "😂", "🔥", "🎉", "😮", "🙏"];

/// A message bubble with `text`, outgoing in the accent colour.
fn bubble(text: Key, outgoing: bool) -> Div {
    let (bg, fg) = if outgoing {
        (accent_strong(), text_on_fill())
    } else {
        (bg_bubble_incoming(), text_primary())
    };
    div()
        .max_w(px(230.))
        .px_3()
        .py_1p5()
        .rounded_xl()
        .bg(bg)
        .text_color(fg)
        .text_sm()
        .child(tr(text))
}

/// Where a piece starts: an edge of the preview and the distance from it,
/// on each axis.
#[derive(Clone, Copy)]
enum Edge {
    Start(f32),
    End(f32),
}

fn start(piece: PreviewPiece) -> (Edge, Edge) {
    match piece {
        PreviewPiece::Tabs => (Edge::Start(12.), Edge::Start(10.)),
        PreviewPiece::Date => (Edge::End(150.), Edge::Start(14.)),
        PreviewPiece::Slab => (Edge::End(16.), Edge::Start(110.)),
        PreviewPiece::Menu => (Edge::Start(160.), Edge::Start(64.)),
        PreviewPiece::Emoji => (Edge::Start(12.), Edge::End(12.)),
        PreviewPiece::Jump => (Edge::End(16.), Edge::End(12.)),
    }
}

/// `piece` at its start, moved by `offset`.
fn placed(piece: PreviewPiece, offset: Point<Pixels>) -> Stateful<Div> {
    let (x, y) = start(piece);
    let el = div().id(piece.id()).absolute();
    let el = match x {
        Edge::Start(left) => el.left(px(left) + offset.x),
        Edge::End(right) => el.right(px(right) - offset.x),
    };
    match y {
        Edge::Start(top) => el.top(px(top) + offset.y),
        Edge::End(bottom) => el.bottom(px(bottom) - offset.y),
    }
}

impl QuillApp {
    pub(super) fn glass_tuner_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let dark = cx.theme().background.l < 0.5;
        let layout = &self.preferences.glass_tuner.preview;
        let pieces: Vec<AnyElement> = layout
            .order()
            .into_iter()
            .map(|piece| {
                let content = self.preview_piece(piece, cx);
                placed(piece, layout.offset(piece))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.preferences
                                .glass_tuner
                                .preview
                                .press(piece, event.position);
                            cx.notify();
                        }),
                    )
                    .test_support()
                    .child(content)
                    .into_any_element()
            })
            .collect();
        let area = layout.area().clone();
        div()
            .id("glass-tuning-preview")
            .test_support()
            .relative()
            .w_full()
            .h(px(HEIGHT))
            .rounded_lg()
            .overflow_hidden()
            .when(layout.dragging().is_some(), |this| {
                this.cursor(CursorStyle::ClosedHand)
            })
            .on_prepaint(move |bounds, _, _| area.record(bounds))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let held = event.pressed_button == Some(MouseButton::Left);
                if this
                    .preferences
                    .glass_tuner
                    .preview
                    .pointer_moved(event.position, held)
                {
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.preferences.glass_tuner.preview.release() {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.preferences.glass_tuner.preview.release() {
                        cx.notify();
                    }
                }),
            )
            .child(super::preview_scene::backdrop(dark))
            .child(
                div()
                    .absolute()
                    .left(relative(0.06))
                    .top(relative(0.62))
                    .child(bubble(keys::Q_PREFS_GLASS_SAMPLE_IN, false)),
            )
            .child(
                div()
                    .absolute()
                    .right(relative(0.2))
                    .top(relative(0.72))
                    .child(bubble(keys::Q_PREFS_GLASS_SAMPLE_OUT, true)),
            )
            .children(pieces)
            .into_any_element()
    }

    /// One surface of the preview, drawn the way the app draws it.
    fn preview_piece(&self, piece: PreviewPiece, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        match piece {
            PreviewPiece::Tabs => div()
                .w(px(240.))
                .child(
                    crate::ui::chrome::list_parts::segmented_tabs(
                        TabBar::new("glass-preview-tabs")
                            .w_full()
                            .segmented()
                            .glass_selection(),
                        self.chrome_metrics(),
                        theme,
                    )
                    .small()
                        .selected_index(0)
                        .child(Tab::new().flex_grow(1.).label(tr(keys::RECENT_CHATS)))
                        .child(Tab::new().flex_grow(1.).label(tr(keys::MENU_CONTACTS)))
                        .child(Tab::new().flex_grow(1.).label(tr(keys::MENU_CALLS))),
                )
                .into_any_element(),
            PreviewPiece::Date => {
                crate::ui::conversation::pill_label(tr(keys::Q_CHATLIST_TODAY), cx)
                    .map(|pill| match super::lens(GlassRole::Control, cx) {
                        Some(glass) => pill
                            .bg(transparent_black())
                            .text_color(cx.theme().foreground)
                            .backdrop_glass(Some(glass)),
                        None => pill,
                    })
                    .into_any_element()
            }
            PreviewPiece::Slab => div()
                .w(px(120.))
                .h(px(72.))
                .rounded_xl()
                .glass_or(GlassRole::Panel, cx, |this| {
                    this.bg(theme.popover).border_1().border_color(theme.border)
                })
                .into_any_element(),
            PreviewPiece::Menu => {
                let row = |icon: IconName, label: Key, danger: bool| {
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .text_sm()
                        .text_color(if danger { danger_bright() } else { text_menu() })
                        .child(Icon::new(icon).size(px(16.)))
                        .child(tr(label))
                };
                div()
                    .w(px(190.))
                    .flex()
                    .flex_col()
                    .p_1()
                    .menu_surface(self.look(), cx)
                    .child(row(IconName::Reply, keys::Q_CHAT_REPLY, false))
                    .child(row(IconName::Copy, keys::Q_AV_COPY, false))
                    .child(row(IconName::Forward, keys::CONTEXT_FORWARD_MSG, false))
                    .child(row(IconName::Pin, keys::CONTEXT_PIN_MSG, false))
                    .child(row(IconName::Delete, keys::BOX_DELETE, true))
                    .into_any_element()
            }
            PreviewPiece::Emoji => div()
                .flex()
                .gap_1()
                .p_1()
                .rounded_lg()
                .border_1()
                .glass_or(GlassRole::Panel, cx, |this| {
                    this.border_color(theme.border)
                        .bg(theme.popover)
                        .shadow_md()
                })
                .children(EMOJI.iter().map(|emoji| {
                    div()
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(20.))
                        .child(*emoji)
                }))
                .into_any_element(),
            PreviewPiece::Jump => super::glass_button(
                Button::new("glass-preview-jump")
                    .secondary()
                    .icon(IconName::ChevronDown)
                    .rounded_full()
                    .border_1(),
                super::lens(GlassRole::Control, cx),
            )
            .into_any_element(),
        }
    }
}
