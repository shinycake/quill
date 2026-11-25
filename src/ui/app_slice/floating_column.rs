//! The conversation when its bars float (docs/decisions/floating-layout.md):
//! the history fills the pane, and the header slice and the composer
//! slice sit over it as pills, centred in the conversation column. The
//! header slice comes after the history's animation layer, so the pills
//! cover animated rows without an occluder, as the composer already does.

use super::super::app::QuillApp;
use super::super::conversation::ConversationPart;
use super::slice_elements::MeasureHeight;
use super::{SliceParts, full};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl QuillApp {
    /// `column` with the history filling it and the two pills over it.
    /// `history` and `composer` are the stacked column's boxes.
    pub(super) fn floating_column(
        &self,
        column: Stateful<Div>,
        history: Stateful<Div>,
        header: &SliceParts,
        composer: Stateful<Div>,
        chat: Option<String>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let slices = &self.slices.window;
        super::super::image_budget::slice_shown(header.slice.entity_id(), self.windows.scope);
        let metrics = self.chrome_metrics();
        let gutter = px(metrics.gutter);
        let width = metrics.chat_column.map(px);
        let lane = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .absolute()
                .left_0()
                .right_0()
                .px(gutter)
                .flex()
                .justify_center()
        };
        let pill_box = || {
            div()
                .w_full()
                .when_some(width, |this, width| this.max_w(width))
        };
        let mut top = pill_box()
            .id("conversation-top-bars")
            .relative()
            .h(slices.top_height.get())
            .child(header.slice.clone().cached(full()))
            .child(header.painter.clone().live());
        if let Some(tag) = chat {
            top = top.a11y_tag(tag);
        }
        let hint = self.pull_hint_overlay(cx).map(|hint| {
            lane("conversation-pull-hint")
                .bottom(gutter)
                .child(pill_box().h(slices.composer_height.get()).child(hint))
        });
        column
            .relative()
            .child(history.absolute().top_0().left_0().size_full())
            .child(
                lane("conversation-top")
                    .role(gpui_kit::Role::Group)
                    .top(gutter)
                    .child(top),
            )
            .child(
                lane("conversation-composer-lane").bottom(gutter).child(
                    pill_box().child(
                        composer
                            .debug_selector(|| "conversation-composer".into())
                            .block_mouse_except_scroll(),
                    ),
                ),
            )
            .children(
                self.media_panel_over_composer()
                    .then(|| self.panel_painter())
                    .flatten(),
            )
            .children(hint)
    }

    /// The header slice: the header and the bars above the history, laid
    /// out from the top of their box, which takes their measured height.
    pub(super) fn header_slice(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(MeasureHeight {
                height: self.slices.window.top_height.clone(),
                redraw: self.height_redraw(),
                child: div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .child(self.conversation(ConversationPart::Bars, false, cx))
                    .into_any_element(),
            })
            .into_any_element()
    }

    /// What a measured slice height redraws when it changes: the view that
    /// sizes the slice's box (`AppOnly`, or the chat window).
    pub(super) fn height_redraw(&self) -> Option<EntityId> {
        self.windows.view_in_place().or(self.slices.app_only_id())
    }
}
