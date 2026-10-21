//! General settings: the Windows "Send to" menu entry
//! (`quill::send_to_menu`, tdesktop's "Place Telegram in \"Send to\" menu").

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// The switch that puts Quill in Explorer's "Send to" menu; none where
    /// there is no such menu (tdesktop shows the row on Windows only).
    pub(in crate::ui) fn general_send_to_section(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !quill::send_to_menu::supported() {
            return None;
        }
        let control = Switch::new("general-send-to-switch")
            .checked(quill::send_to_menu::is_enabled())
            .accessibility_label(tr(keys::Q_PREFS_PLACE_QUILL_IN_THE_SEND))
            .on_click(cx.listener(|_, &on, _, cx| {
                // The shell writes or deletes a shortcut file: not on the UI thread.
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { quill::send_to_menu::set_enabled(on) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(error) = result {
                            this.connection.set_note(StatusNote::error(tr_with(
                                keys::Q_PREFS_COULD_NOT_UPDATE_THE_SEND,
                                &[("error", &error.to_string())],
                            )));
                        }
                        cx.notify();
                    });
                })
                .detach();
            }));
        Some(self.appearance_section(
            cx,
            tr(keys::Q_PREFS_PLACE_QUILL_IN_SEND_TO),
            tr(keys::Q_PREFS_RIGHT_CLICK_FILES_IN_EXPLORER),
            control.into_any_element(),
        ))
    }
}
