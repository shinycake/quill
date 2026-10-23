use quill::ids::ChatId;
use quill::permissions_box::{BoxContext, Permission};
use quill::telegram::envelope::ChatPermissions;

/// The group permissions box (tdesktop `ShowEditPeerPermissionsBox`):
/// the staged default permissions, slow mode and boosters values. Save
/// sends only what differs from the chat (`setChatPermissions`,
/// `setChatSlowModeDelay`, `setSupergroupUnrestrictBoostCount`).
pub struct PermissionsDialog {
    pub(crate) chat_id: ChatId,
    /// The chat's shape and the viewer's own rights, read on open.
    pub(crate) context: BoxContext,
    pub(crate) permissions: ChatPermissions,
    /// Whether the "Send media" group is expanded.
    pub(crate) media_open: bool,
    pub(crate) slow_mode_delay: i32,
    /// 0 when boosters are restricted like everyone else.
    pub(crate) boosts: i32,
    /// The locked row the viewer last clicked; its reason shows under it
    /// (tdesktop shows it as a toast).
    pub(crate) lock_hint: Option<Permission>,
}
