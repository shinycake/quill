//! Forwarding and the share box.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::ForwardResult;

pub(crate) struct ShareUi {
    /// The forward being built: the selection, the share box over it, the
    /// "Reply in Another Chat" chooser over it, or
    /// the forward bar it became (`ForwardStage`).
    pub(super) forward: super::forward_stage::ForwardStage,
    /// Destinations ticked in the share box.
    pub(super) selection: quill::share_box::ShareSelection,
    /// The share box's optional comment, sent before the forwards.
    pub(super) comment_input: Entity<TextareaState>,
    pub(super) search_input: Entity<TextareaState>,
    /// The forward box just opened: its search field takes focus once the
    /// dialog has mounted (the dialog focuses itself as it opens).
    pub(super) focus_search: bool,
    /// Last successful (or failed) `forwardMessages` result.
    pub(super) forward_result: Option<ForwardResult>,
    /// Text of a share link while its chat chooser is open.
    pub(super) link_text: Option<String>,
}

impl ShareUi {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        forward_search_input: Entity<TextareaState>,
    ) -> Self {
        let share_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_ADD_A_COMMENT))
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        Self {
            forward: Default::default(),
            selection: quill::share_box::ShareSelection::default(),
            comment_input: share_comment_input,
            search_input: forward_search_input,
            focus_search: false,
            forward_result: None,
            link_text: None,
        }
    }
}

/// Nothing to say: its state is the account's only app-side.
impl super::on_screen::Farewell for ShareUi {}
