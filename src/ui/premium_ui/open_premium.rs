//! Methods moved out of `premium_ui.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;

impl QuillApp {
    pub(in crate::ui) fn open_premium(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Payments,
        ) {
            return;
        }
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.hub.premium_open = true;
            live.driver.session.payments.hub.promo_page =
                quill::premium_hub::promo::PromoPage::Overview;
            // The subscribe button needs `premium_bot_username` / `premium_invoice_slug`.
            let _ = live.driver.ensure_earn_app_config();
            live.driver.maybe_fetch_premium()
        });
        if let Some(Err(_)) = sent {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_NOTES_COULDNT_LOAD_PREMIUM)));
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_premium(&mut self, cx: &mut Context<Self>) {
        self.premium_preview.stop();
        if let Some(hub) = self.hub_mut() {
            hub.premium_open = false;
            hub.premium_requested = None;
        }
        cx.notify();
    }

    pub(in crate::ui) fn build_premium_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PremiumFeatures, |this, _, cx| {
                this.close_premium(cx);
            });
        app.update(cx, |this, cx| {
            let title = this.promo_title();
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title));
            if this.session().is_none() {
                return dialog.on_close(on_close);
            }
            let body = this.premium_promo_body(cx);
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }
}
