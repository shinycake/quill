//! The shared "not supported in Quill" dialog, driven by
//! `TonWalletState::notice`. See docs/decisions/wallet-not-supported-notice.md.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::ton_wallet::{OFFICIAL_APPS_URL, WalletSurface};
use std::cell::RefCell;
use std::rc::Rc;

pub(in crate::ui) fn notice_button(
    id: impl Into<ElementId>,
    selector: String,
    label: impl Into<SharedString>,
    surface: WalletSurface,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .debug_selector(move || selector.clone())
        .child(
            Button::new(id)
                .label(label)
                .outline()
                .small()
                .on_click(cx.listener(move |this, _, _, cx| this.show_wallet_notice(surface, cx))),
        )
        .into_any_element()
}

impl QuillApp {
    pub(in crate::ui) fn show_wallet_notice(
        &mut self,
        surface: WalletSurface,
        cx: &mut Context<Self>,
    ) {
        if let Some(wallet) = self.ton_wallet_mut() {
            wallet.show_notice(surface);
        }
        cx.notify();
    }

    pub(in crate::ui) fn dismiss_wallet_notice(&mut self, cx: &mut Context<Self>) {
        if let Some(wallet) = self.ton_wallet_mut() {
            wallet.dismiss_notice();
        }
        cx.notify();
    }

    pub(in crate::ui) fn wallet_notice_open(&self) -> bool {
        self.session()
            .is_some_and(|session| session.ton_wallet.notice.is_some())
    }

    pub(super) fn build_wallet_notice_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::WalletNotice, |this, _, cx| {
                this.dismiss_wallet_notice(cx);
            });
        app.update(cx, |_, cx| {
            let body = div()
                .id("wallet-notice")
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    div()
                        .id("wallet-notice-text")
                        .text_sm()
                        .debug_selector(|| "wallet-notice-text".to_string())
                        .child(tr(keys::Q_MONEY_WALLET_NOT_SUPPORTED)),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            div()
                                .debug_selector(|| "wallet-notice-ok".to_string())
                                .child(
                                    Button::new("wallet-notice-ok")
                                        .label(tr(keys::BOX_OK))
                                        .ghost()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.dismiss_wallet_notice(cx);
                                            this.close_kit_dialog_if_done(
                                                DialogKind::WalletNotice,
                                                window,
                                                cx,
                                            );
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .debug_selector(|| "wallet-notice-open".to_string())
                                .child(
                                    Button::new("wallet-notice-open")
                                        .label(tr(keys::Q_MONEY_GET_TELEGRAM_APPS))
                                        .primary()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            cx.open_url(OFFICIAL_APPS_URL);
                                            this.dismiss_wallet_notice(cx);
                                            this.close_kit_dialog_if_done(
                                                DialogKind::WalletNotice,
                                                window,
                                                cx,
                                            );
                                        })),
                                ),
                        ),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_MONEY_MONEY)))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "wallet_notice_tests.rs"]
mod tests;

crate::ui::shell::register_dialogs! {
    WalletNotice => DialogSpec::new(
        3790,
        |app| app.wallet_notice_open(),
        QuillApp::build_wallet_notice_dialog,
    ),
}
