//! Opening and closing the Premium dialog: the Premium page, or the
//! preview of one feature (tdesktop `Settings::ShowPremium`,
//! `ShowPremiumPreviewBox`, `ShowPremiumPreviewToBuy`).

use super::*;
use crate::ui::status_note::StatusNote;
use quill::premium_hub::promo::{PreviewOpen, PromoFeature, PromoPage};

impl QuillApp {
    /// The Premium page (tdesktop `Settings::ShowPremium` with no ref).
    pub(in crate::ui) fn open_premium(&mut self, cx: &mut Context<Self>) {
        self.open_premium_from("", cx);
    }

    /// The Premium page; its subscribe button sends `start_ref` to the
    /// Premium bot (tdesktop `Settings::ShowPremium(controller, ref)`).
    pub(in crate::ui) fn open_premium_from(&mut self, start_ref: &str, cx: &mut Context<Self>) {
        self.open_premium_on(PromoPage::Overview, start_ref, cx);
    }

    /// The preview of `feature` with one button to the Premium page
    /// (tdesktop `ShowPremiumPreviewBox`).
    pub(in crate::ui) fn open_premium_at(
        &mut self,
        feature: impl Into<PromoFeature>,
        cx: &mut Context<Self>,
    ) {
        let page = PromoPage::for_feature(feature.into(), PreviewOpen::Box);
        self.open_premium_on(page, "", cx);
    }

    /// The preview of `feature` with the plans and Subscribe (tdesktop
    /// `ShowPremiumPreviewToBuy`).
    pub(in crate::ui) fn open_premium_to_buy(
        &mut self,
        feature: impl Into<PromoFeature>,
        cx: &mut Context<Self>,
    ) {
        let page = PromoPage::for_feature(feature.into(), PreviewOpen::ToBuy);
        self.open_premium_on(page, "", cx);
    }

    fn open_premium_on(&mut self, page: PromoPage, start_ref: &str, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.premium_open = true;
            hub.premium_ref = start_ref.into();
        }
        let sent = self.live.as_mut().map(|live| {
            // The subscribe button needs `premium_bot_username` / `premium_invoice_slug`.
            let _ = live.driver.ensure_earn_app_config();
            if matches!(
                page,
                PromoPage::Preview {
                    feature: PromoFeature::Business(_),
                    ..
                }
            ) {
                let _ = live.driver.maybe_fetch_business_features();
            }
            live.driver.maybe_fetch_premium()
        });
        if let Some(Err(_)) = sent {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_NOTES_COULDNT_LOAD_PREMIUM)));
        }
        self.set_promo_page(page, cx);
    }

    /// Open the preview the reducer asked for (a refused "Hide ads").
    pub(in crate::ui) fn pump_promo_request(&mut self, cx: &mut Context<Self>) {
        if let Some(feature) = self.hub_mut().and_then(|hub| hub.promo_request.take()) {
            self.open_premium_at(feature, cx);
        }
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
