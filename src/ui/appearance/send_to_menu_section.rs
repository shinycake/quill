//! General settings: the Windows "Send to" menu entry
//! (`quill::send_to_menu`, tdesktop's "Place Telegram in \"Send to\" menu").

use super::*;

impl QuillApp {
    /// The switch that puts Quill in Explorer's "Send to" menu; none where
    /// there is no such menu (tdesktop shows the row on Windows only).
    pub(super) fn general_send_to_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !quill::send_to_menu::supported() {
            return None;
        }
        let control = Switch::new("general-send-to-switch")
            .checked(quill::send_to_menu::is_enabled())
            .accessibility_label("Place Quill in the Send to menu")
            .on_click(cx.listener(|_, &on, _, cx| {
                // The shell writes or deletes a shortcut file: not on the UI thread.
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { quill::send_to_menu::set_enabled(on) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(error) = result {
                            this.connection.status_note =
                                format!("could not update the Send to menu: {error}");
                        }
                        cx.notify();
                    });
                })
                .detach();
            }));
        Some(self.appearance_section(
            cx,
            "Place Quill in \"Send to\" menu",
            "Right-click files in Explorer, choose Send to, then Quill to pick a chat for them.",
            control.into_any_element(),
        ))
    }
}
