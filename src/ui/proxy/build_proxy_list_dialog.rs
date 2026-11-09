//! The proxy list box over the sign-in screen (tdesktop's `ProxiesBox`):
//! the same body as Settings → Connection.

use super::*;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(super) fn close_proxy_list_box(&mut self, cx: &mut Context<Self>) {
        self.settings.proxy.list_box = false;
        cx.notify();
    }

    pub(in crate::ui) fn build_proxy_list_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProxyList, |this, _, cx| {
                this.close_proxy_list_box(cx);
            });
        app.update(cx, |this, cx| {
            let body = this.proxy_settings_body(cx).into_any_element();
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::PROXY_SETTINGS)))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .on_close(on_close)
        })
    }
}
