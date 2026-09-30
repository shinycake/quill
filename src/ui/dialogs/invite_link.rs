use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;
/// Phase D3a: invite-link create dialog above the composer. Fields map
/// 1:1 to `createChatInviteLink` (schema 1.8.67 line 14097): name,
/// expiration, member limit, creates-join-request toggle. Expiration is
/// entered as whole days from now (0 = never); the submit path converts
/// to a unix timestamp. Editing an existing link stays out of this
/// slice (documented in DECISIONS.md).
pub struct InviteLinkDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) name_input: Entity<TextareaState>,
    pub(crate) expiration_days_input: Entity<TextareaState>,
    pub(crate) member_limit_input: Entity<TextareaState>,
    pub(crate) creates_join_request: bool,
}

impl InviteLinkDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, chat_id: ChatId) -> Self {
        let name_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Link name (optional)")
                .auto_grow(1, 2)
                .submit_on_enter(false)
        });
        let expiration_days_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Expires in days (0 = never)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let member_limit_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Member limit (0 = unlimited)")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            name_input,
            expiration_days_input,
            member_limit_input,
            creates_join_request: false,
        }
    }
}
