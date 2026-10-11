//! Methods moved out of `history.rs` to keep files under 1000 lines.

use super::*;

impl AvatarLink {
    pub(super) fn wrap(self, avatar: AnyElement) -> AnyElement {
        div()
            .id(("sender-avatar", self.id))
            .size(px(32.))
            .flex_none()
            .cursor_pointer()
            .child(avatar)
            .on_click(self.on_click)
            .into_any_element()
    }
}
