//! The corner buttons beside a bubble exist for the row under the pointer
//! alone, as in tdesktop, where they belong to the one hovered item.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::ids::MessageId;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Default)]
pub(in crate::ui) struct RowHover {
    under: Rc<Cell<Option<MessageId>>>,
    painted: Rc<Cell<Vec<(MessageId, Hitbox)>>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum CornerActions {
    Absent,
    Shown,
    TreeOnly,
}

impl CornerActions {
    pub(in crate::ui) fn mount(self, build: impl FnOnce() -> AnyElement) -> Option<AnyElement> {
        (self != Self::Absent).then(|| {
            div()
                .when(self == Self::TreeOnly, |this| this.invisible())
                .child(build())
                .into_any_element()
        })
    }
}

fn row_under(rows: &[(MessageId, Hitbox)], window: &Window) -> Option<MessageId> {
    if window.last_input_was_touch() {
        return None;
    }
    rows.iter()
        .find(|(_, hitbox)| hitbox.is_hovered(window))
        .map(|(id, _)| *id)
}

impl RowHover {
    pub(in crate::ui) fn corner_actions(
        &self,
        row: Option<MessageId>,
        tree_wanted: bool,
    ) -> CornerActions {
        if row.is_some() && self.under.get() == row {
            CornerActions::Shown
        } else if tree_wanted {
            CornerActions::TreeOnly
        } else {
            CornerActions::Absent
        }
    }

    pub(in crate::ui) fn area(&self, id: MessageId, row: AnyElement) -> AnyElement {
        let painted = self.painted.clone();
        let hitbox = canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, _, _| {
                let mut rows = painted.take();
                rows.push((id, hitbox));
                painted.set(rows);
            },
        )
        .absolute()
        .inset_0()
        .size_full();
        div()
            .relative()
            .flex()
            .flex_col()
            .child(hitbox)
            .child(row)
            .into_any_element()
    }

    pub(in crate::ui) fn rows_painted(&self, window: &mut Window, cx: &mut App) {
        let rows = self.painted.take();
        let view = window.current_view();
        let under = row_under(&rows, window);
        if self.under.replace(under) != under {
            // A notify inside the draw would be lost.
            cx.defer(move |cx| cx.notify(view));
        }
        let hovered = self.under.clone();
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                let under = row_under(&rows, window);
                if hovered.replace(under) != under {
                    cx.notify(view);
                }
            }
        });
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "row_hover_tests.rs"]
mod tests;
