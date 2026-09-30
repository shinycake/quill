use quill::ids::ChatId;
use quill::telegram::envelope::ChatPermissions;
/// Slice G1: default chat permissions editor (`setChatPermissions`,
/// schema 1.8.67, line 13464). The staged copy starts from the chat's
/// current block; TDLib only lets the new block loosen the old one
/// when `can_restrict_members` is held, otherwise the call fails and
/// the error surfaces in `status_note`.
pub struct PermissionsDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) permissions: ChatPermissions,
}
