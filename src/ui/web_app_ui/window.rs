//! The open helper window's own methods: writing to the helper, the popups
//! it shows and the menu it offers.

use super::*;

impl Drop for MiniAppWindow {
    fn drop(&mut self) {
        // Ask it to close, then reap on a detached thread: neither the
        // queue nor the wait can hold up the UI thread.
        let _ = self.writer.send(encode(&HostCommand::Close).into_bytes());
        if let Some(child) = self.child.take() {
            quill::child_reap::reap_after(child, Duration::from_millis(200));
        }
    }
}

impl MiniAppWindow {
    /// Close it as `Drop` does, and hear when the helper is gone.
    pub(super) fn close_with_notice(mut self) -> Option<std::sync::mpsc::Receiver<()>> {
        let _ = self.writer.send(encode(&HostCommand::Close).into_bytes());
        let child = self.child.take()?;
        Some(quill::child_reap::reap_after_notice(
            child,
            Duration::from_millis(200),
        ))
    }

    pub(super) fn send(&mut self, command: &HostCommand) {
        if self.writer.send(encode(command).into_bytes()) == helper_pipe::SendResult::Queued
            || self.helper == launch_flow::HelperState::Wedged
        {
            return;
        }
        self.helper = launch_flow::HelperState::Wedged;
        eprintln!("quill: the mini app helper stopped reading its input; closing it");
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }

    pub(super) fn emit(&mut self, reply: BridgeReply) {
        self.send(&HostCommand::Emit {
            event: reply.event,
            data: reply.data,
        });
    }

    /// Every popup the helper is still showing: the plain ones and the ones
    /// a stage waits on.
    pub(super) fn open_popups(&self) -> Vec<PopupId> {
        let stages = [
            match self.write_access {
                launch_flow::WriteAccess::Asking { popup } => Some(popup),
                _ => None,
            },
            self.emoji.access_popup(),
            self.emoji.status_popup(),
            self.phone.popup(),
        ];
        self.popups
            .keys()
            .copied()
            .chain(stages.into_iter().flatten())
            .collect()
    }

    /// The stage that holds `popup`, asked stage by stage.
    pub(super) fn popup_owner(&self, popup: PopupId) -> Option<PopupOwner> {
        if matches!(self.write_access, launch_flow::WriteAccess::Asking { popup: held } if held == popup)
        {
            Some(PopupOwner::WriteAccess)
        } else if self.emoji.access_popup() == Some(popup) {
            Some(PopupOwner::EmojiAccess)
        } else if self.emoji.status_popup() == Some(popup) {
            Some(PopupOwner::EmojiStatus)
        } else if self.phone.popup() == Some(popup) {
            Some(PopupOwner::Phone)
        } else {
            None
        }
    }

    /// Show a popup; the stage that waits for its answer holds the id, and
    /// the ones it replaced are in the result.
    pub(super) fn show_popup(&mut self, popup_without_id: Popup) -> Shown {
        let replaced = self.open_popups();
        self.popup_seq += 1;
        let popup_id = PopupId::new(self.popup_seq);
        self.send(&HostCommand::ShowPopup {
            popup: Popup {
                id: popup_id.wire(),
                ..popup_without_id
            },
        });
        Shown {
            id: popup_id,
            replaced,
        }
    }

    /// Show a popup no stage waits on, remembering what it was for.
    pub(super) fn show_purpose_popup(&mut self, purpose: PopupPurpose, popup: Popup) -> Shown {
        let shown = self.show_popup(popup);
        self.popups.insert(shown.id, purpose);
        shown
    }

    pub(super) fn menu(&self) -> Vec<MenuItem> {
        menu_items(
            self.settings_button,
            self.privacy_policy_url.is_some(),
            self.in_attachment_menu,
        )
        .into_iter()
        .map(|(id, label, attention)| MenuItem {
            id: id.to_string(),
            label: label.to_string(),
            attention,
        })
        .collect()
    }

    /// `#rrggbb` for a bridge color, or the theme's color for a key.
    pub(super) fn resolve_color(&self, spec: &ColorSpec) -> String {
        match spec {
            ColorSpec::Rgb(hex) => hex.clone(),
            ColorSpec::Key(key) => self.theme.key_color(*key).hex(),
        }
    }
}
