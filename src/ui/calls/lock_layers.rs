//! The call windows under the passcode lock: tdesktop's per-layer rule.
//! The call panel stays up and answerable (`LockRole::Unlocked` in
//! `passcode::lock_layer`), but `Application::lockByPasscode` hides its
//! layers (`Calls::Instance::hidePanelLayers`), and Add People and the share
//! link actions ask for the passcode first (`Panel::addPeople`,
//! `GroupPanel::addMembers`, `shareConferenceLink`). So at the lock every
//! box over a call window closes, the stream key is hidden again, and while
//! locked the openers of those boxes refuse and the stream URL and key are
//! not drawn. Mute, video, screen share, hang up and answer keep working.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// Called from `close_for_lock`.
    pub(in crate::ui) fn close_call_layers_for_lock(&mut self, cx: &mut Context<Self>) {
        if self.call_boxes.add_people.take().is_some()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.search_add_people("");
        }
        self.group_call.stream.overlay = None;
        self.group_call.stream.key_shown = false;
        self.group_call_screen.title_dialog = None;
        self.group_call_screen.settings_open = false;
        self.screen_share.privacy_for = None;
        cx.notify();
    }
}
