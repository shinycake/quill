//! "Share proxy with QR code" box (`parity:data-proxy-extras`). tdesktop
//! shows the proxy's `tg://` link as a QR code in `ShowProxyQrBox`
//! (`boxes/connection_box.cpp`); Quill reuses the QR renderer of the
//! login and invite-link screens.
use super::app::QuillApp;
use super::auth_ui::render_qr_image;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::proxy_extras::qr_share_link;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// The open QR box: the encoded link and its rendered bitmap.
pub(crate) struct ProxyQr {
    pub(crate) link: String,
    image: Arc<RenderImage>,
}

impl QuillApp {
    /// Row action "Show QR code": opens the box for a listed proxy.
    pub(super) fn open_proxy_qr(&mut self, id: i32, cx: &mut Context<Self>) {
        let link = self
            .session()
            .and_then(|s| s.settings.proxy.find(id))
            .and_then(qr_share_link);
        let qr = link.and_then(|link| {
            let image = render_qr_image(&link)?;
            Some(ProxyQr { link, image })
        });
        match qr {
            Some(qr) => self.settings.proxy.qr = Some(qr),
            None => {
                self.settings.proxy.notice = Some("HTTP proxies can't be shared as a link.".into())
            }
        }
        cx.notify();
    }

    fn close_proxy_qr(&mut self, cx: &mut Context<Self>) {
        self.settings.proxy.qr = None;
        cx.notify();
    }

    pub(in crate::ui) fn build_proxy_qr_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::ProxyQr, |this, _, cx| {
            this.close_proxy_qr(cx);
        });
        app.update(cx, |this, cx| {
            let title = crate::ui::shell::dialog_title("Share proxy with QR code");
            let Some(qr) = this.settings.proxy.qr.as_ref() else {
                return dialog.overlay(true).title(title).on_close(on_close);
            };
            let (link, image) = (qr.link.clone(), qr.image.clone());
            let body = div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div().p_2().rounded_lg().bg(gpui_kit::white()).child(
                        img(ImageSource::from(image))
                            .size(px(240.))
                            .aspect_square()
                            .object_fit(ObjectFit::Contain),
                    ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Scan this code with another device to add the proxy."),
                );
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("proxy-qr-copy")
                        .label("Copy link")
                        .ghost()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                        })),
                )
                .child(
                    Button::new("proxy-qr-done")
                        .label("Done")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_proxy_qr(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyQr, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(title)
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Opens over the proxy list, so it ranks before it.
    ProxyQr => DialogSpec::new(
        3050,
        |app| app.settings.proxy.qr.is_some(),
        QuillApp::build_proxy_qr_dialog,
    ),
}
