//! The transfer picker of a collectible (tdesktop
//! `boxes/transfer_gift_box.cpp`): "Transfer {name}" with the search,
//! "Send via Blockchain" and the contacts; the confirmation for a contact;
//! and the export chain "Unlocking in progress" / "Manage with Fragment" /
//! "Two-step verification" / "Security check", plus "Action Locked" for a
//! gift held in a TON wallet. One dialog shows the current step.
//!
//! Only two buttons send anything: Transfer in the confirmation
//! (`transferGift`) and Submit in the password box
//! (`getUpgradedGiftWithdrawalUrl`, which only returns Fragment's page).
//! The password is read from the masked field once, handed to that one
//! request and the field is emptied at once; it is never stored.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gift_manage_ui::{hint, stars};
use super::gift_transfer_view::{StageView, stage_view};
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_manage::{TransferPicker, TransferStage, fragment_gift_url};
use quill::lang::{keys, tr, tr_with};
use quill::link_policy::{OpenDecision, open_decision};
use quill::local_time::now_unix;
use quill::text::LinkTarget;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// "Transfer" on one of the user's collectibles.
    pub(super) fn open_gift_transfer(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picker = self
            .session()
            .and_then(|s| s.payments.hub.gifts.iter().find(|g| g.id == id))
            .map(TransferPicker::for_gift);
        let Some(picker) = picker else {
            return;
        };
        self.clear_transfer_inputs(window, cx);
        if let Some(live) = self.live.as_mut()
            && !live.driver.session.contacts_settled()
        {
            let _ = live.driver.fetch_contacts();
        }
        if let Some(manage) = self.gift_manage_mut() {
            manage.flow = Some(quill::gift_manage::GiftFlow::Transfer(picker));
        }
        cx.notify();
    }

    fn clear_transfer_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.payments
            .pick_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.clear_export_password(window, cx);
    }

    fn clear_export_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.payments
            .gift_export_password
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Close the picker and every step on it.
    pub(super) fn close_gift_transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(manage) = self.gift_manage_mut()
            && manage.transfer().is_some()
        {
            manage.flow = None;
        }
        self.clear_transfer_inputs(window, cx);
        cx.notify();
    }

    /// Edit the open picker (live or demo session).
    fn edit_transfer(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut TransferPicker)) {
        if let Some(picker) = self.gift_manage_mut().and_then(|m| m.transfer_mut()) {
            edit(picker);
        }
        cx.notify();
    }

    /// Cancel or OK on a step: back to the list, the way closing a box
    /// in tdesktop shows the picker under it.
    pub(super) fn transfer_step_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_export_password(window, cx);
        self.edit_transfer(cx, TransferPicker::back);
    }

    pub(super) fn pick_transfer_contact(
        &mut self,
        user_id: i64,
        name: String,
        cx: &mut Context<Self>,
    ) {
        self.edit_transfer(cx, |picker| picker.choose(user_id, name));
    }

    /// "Send via Blockchain".
    pub(super) fn tap_transfer_export(&mut self, cx: &mut Context<Self>) {
        let now = now_unix();
        let state = self.session().and_then(|s| {
            let id = &s.payments.gifts_manage.transfer()?.id;
            let gift = s.payments.hub.gifts.iter().find(|g| g.id == *id)?;
            Some(gift.export_state(now))
        });
        if let Some(state) = state {
            self.edit_transfer(cx, |picker| picker.tap_export(state));
        }
    }

    /// "Open Fragment" in "Manage with Fragment": the password box. The
    /// password state gives the hint (tdesktop `cloudPassword().reload()`).
    pub(super) fn confirm_transfer_fragment(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_password_state();
        }
        self.clear_export_password(window, cx);
        self.edit_transfer(cx, TransferPicker::open_fragment);
    }

    /// Transfer in the confirmation.
    pub(super) fn confirm_picked_transfer(&mut self, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.transfer_picked_gift());
        match sent {
            Some(Ok(_)) => {}
            Some(Err(_)) => self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_TRANSFER_THE_GIFT)),
            None => self.note_gift_refused(tr(keys::Q_MONEY_DEMO_MODE_THIS_NEEDS_A)),
        }
        cx.notify();
    }

    /// Submit in the password box. The field is read once and emptied
    /// as the request goes out.
    pub(super) fn submit_gift_export_password(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.payments.gift_export_password.clone();
        let password = input.read(cx).value().to_string();
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.export_picked_gift(&password));
        drop(password);
        input.update(cx, |input, cx| input.set_value("", window, cx));
        match sent {
            Some(Ok(_)) => {}
            Some(Err(_)) => self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_OPEN_FRAGMENT)),
            None => self.note_gift_refused(tr(keys::Q_MONEY_DEMO_MODE_THIS_NEEDS_A)),
        }
        cx.notify();
    }

    /// "Forgot password?" in the password box: the two-step verification
    /// page's recovery, as the revenue withdrawal does.
    pub(super) fn forgot_gift_export_password(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_email = self
            .session()
            .and_then(|s| s.auth_state.password_state.as_ref())
            .is_some_and(|state| state.has_recovery_email_address);
        self.close_gift_transfer(window, cx);
        self.open_twofa(cx);
        self.goto_twofa_view(crate::ui::dialogs::callback_password::TwofaView::Recover);
        if has_email
            && let Some(live) = self.live.as_mut()
            && live.driver.request_password_recovery().is_err()
        {
            self.connection.status_note = tr(keys::Q_MONEY_COULDNT_SEND_THE_RECOVERY_CODE).into();
        }
        cx.notify();
    }

    /// "Set password" in the security check.
    pub(super) fn set_password_for_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_gift_transfer(window, cx);
        self.open_twofa(cx);
    }

    /// "Open Fragment" in "Action Locked": the gift's page on Fragment.
    pub(super) fn open_locked_gift_on_fragment(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let slug = self
            .session()
            .and_then(|s| s.payments.gifts_manage.transfer())
            .map(|p| p.slug.clone())
            .filter(|slug| !slug.is_empty());
        if let Some(slug) = slug {
            self.open_fragment_url(&fragment_gift_url(&slug), cx);
        }
        self.close_gift_transfer(window, cx);
    }

    /// Open a Fragment page in the browser, only when the link policy
    /// opens it without asking (an https address with nothing suspicious).
    fn open_fragment_url(&mut self, url: &str, cx: &mut Context<Self>) {
        let link = LinkTarget::Url {
            url: url.to_string(),
            label: None,
        };
        if url.starts_with("https://") && open_decision(&link) == OpenDecision::Open {
            cx.open_url(url);
        } else {
            self.connection.status_note = tr(keys::Q_MONEY_COULDNT_LOAD_THE_PAGE).into();
        }
    }

    /// Per frame: open Fragment's export page, toast a finished transfer
    /// and open the recipient's chat, and keep the password field empty
    /// whenever the password box is not on screen.
    pub(in crate::ui) fn gift_transfer_frame_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (url, done, in_password) = match self.gift_manage_mut() {
            Some(manage) => (
                manage.export_url.take(),
                manage.transferred.take(),
                matches!(
                    manage.transfer().map(|p| &p.stage),
                    Some(TransferStage::Password { .. })
                ),
            ),
            None => (None, None, false),
        };
        if !in_password
            && !self
                .payments
                .gift_export_password
                .read(cx)
                .value()
                .is_empty()
        {
            self.clear_export_password(window, cx);
        }
        if let Some(url) = url {
            self.open_fragment_url(&url, cx);
            self.clear_transfer_inputs(window, cx);
            cx.notify();
        }
        if let Some(done) = done {
            let text = tr_with(
                keys::GIFT_TRANSFERRED_ABOUT,
                &[("name", &done.gift_name), ("recipient", &done.recipient)],
            );
            cx.defer_in(window, move |_, window, cx| {
                window.push_notification(
                    Notification::new()
                        .title(tr(keys::GIFT_TRANSFERRED_TITLE))
                        .message(text),
                    cx,
                );
            });
            // tdesktop `showPeerHistory(peer)` after a transfer from the
            // user's own gifts.
            self.clear_transfer_inputs(window, cx);
            self.close_gifts(cx);
            if self.live.is_some() {
                self.open_user_chat(done.user_id, window, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn build_gift_transfer_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        // Escape or the close button on a step goes back to the list;
        // on the list it closes the picker.
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GiftTransfer, |this, window, cx| {
                let on_list = this
                    .session()
                    .and_then(|s| s.payments.gifts_manage.transfer())
                    .is_none_or(|p| p.stage == TransferStage::Pick);
                if on_list {
                    this.close_gift_transfer(window, cx);
                } else {
                    this.transfer_step_back(window, cx);
                }
            });
        app.update(cx, |this, cx| {
            let Some(StageView {
                title,
                body,
                footer,
            }) = stage_view(this, cx)
            else {
                return dialog.overlay(true).on_close(on_close);
            };
            let body = Rc::new(RefCell::new(Some(body)));
            let dialog = dialog.overlay(true).w(px(420.));
            let dialog = match title {
                Some(title) => dialog.title(crate::ui::shell::dialog_title(title)),
                None => dialog,
            };
            dialog
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

/// "{stars}" for the confirmation and its button, or `None` when free.
pub(super) fn transfer_price(star_count: i64) -> Option<String> {
    (star_count > 0).then(|| stars(star_count))
}

/// The low-balance line under the confirmation.
pub(super) fn balance_hint(cx: &App, balance: Option<i64>) -> Div {
    hint(
        tr_with(
            keys::Q_MONEY_YOUR_BALANCE_IS_TOO_LOW_3,
            &[("unwrap_or_default", &balance.map(stars).unwrap_or_default())],
        ),
        cx,
    )
}

crate::ui::shell::register_dialogs! {
    /// The transfer picker of a collectible and its export steps. Above
    /// the gifts dialog it opens from, which shows again when it closes.
    GiftTransfer => DialogSpec::new(
        3870,
        |app| {
            app.session().is_some_and(|s| {
                s.payments.hub.gifts_open && s.payments.gifts_manage.transfer().is_some()
            })
        },
        QuillApp::build_gift_transfer_dialog,
    ),
}
