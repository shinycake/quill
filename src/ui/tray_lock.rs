//! What the tray menu does while Quill is locked. A locked Quill changes no
//! setting from behind the lock screen: the menu's notification toggles are
//! left out (disabled where the menu cannot drop an item), and a click that
//! was already on its way is refused here (docs/decisions/
//! lock-is-a-render-state.md; tdesktop hides the toggle while locked).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// The tray icon and menu for the account on screen and the lock.
    pub(crate) fn sync_system_tray(&self) {
        quill::tray::sync_tray(self.session(), self.refused_while_locked());
    }

    pub(crate) fn sync_system_tray_startup(&self) {
        quill::tray::sync_tray_startup(self.session(), self.refused_while_locked());
    }

    /// The tray's "Disable/Enable notifications".
    pub(crate) fn toggle_notifications_from_tray(&mut self, cx: &mut Context<Self>) {
        if self.refused_while_locked() {
            return;
        }
        let on = self.desktop_notifications_enabled();
        self.set_desktop_notifications(!on, cx);
    }

    /// The tray's "Disable/Enable notification sounds".
    pub(crate) fn toggle_sounds_from_tray(&mut self, cx: &mut Context<Self>) {
        if self.refused_while_locked() {
            return;
        }
        let on = self.notification_sounds_enabled();
        self.set_inapp_sounds_enabled(!on, cx);
    }
}
