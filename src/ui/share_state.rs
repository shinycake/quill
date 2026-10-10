//! Forwarding and the share box.

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::composer::ForwardDraft;
use quill::ids::ChatId;
use quill::state::ForwardResult;

pub(crate) struct ShareUi {
    /// tdesktop `Data::ForwardDraft` / history multi-select.
    pub(super) pending_forward: Option<ForwardDraft>,
    /// ShareBox / `ShowForwardMessagesBox` dest picker overlay.
    pub(super) forward_picker_open: bool,
    /// "Reply in Another Chat": the chat chooser for the composer's reply.
    pub(super) reply_elsewhere_open: bool,
    /// "Update Quote": the picker for the part of the message to quote.
    pub(super) reply_quote_open: bool,
    /// Destinations ticked in the share box.
    pub(super) selection: quill::share_box::ShareSelection,
    /// The share box's optional comment, sent before the forwards.
    pub(super) comment_input: Entity<TextareaState>,
    pub(super) search_input: Entity<TextareaState>,
    /// The forward bar is showing above this chat's composer
    /// (`pending_forward` follows the user into the destination).
    pub(super) forward_bar_dest: Option<ChatId>,
    /// Last successful (or failed) `forwardMessages` result.
    pub(super) forward_result: Option<ForwardResult>,
    /// Text of a share link while its chat chooser is open.
    pub(super) link_text: Option<String>,
    /// The attach menu's Contact / Location panel.
    pub(super) content_dialog: Option<ShareContentDialog>,
    /// Phase S1: "New secret chat" contact-picker overlay (sidebar).
    pub(super) new_secret_picker_open: bool,
}

impl ShareUi {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        forward_search_input: Entity<TextareaState>,
    ) -> Self {
        let share_comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Add a comment")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        Self {
            pending_forward: None,
            forward_picker_open: false,
            reply_elsewhere_open: false,
            reply_quote_open: false,
            selection: quill::share_box::ShareSelection::default(),
            comment_input: share_comment_input,
            search_input: forward_search_input,
            forward_bar_dest: None,
            forward_result: None,
            link_text: None,
            content_dialog: None,
            new_secret_picker_open: false,
        }
    }
}
