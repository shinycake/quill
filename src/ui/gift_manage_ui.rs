//! Owner actions on received gifts: pin, upgrade, transfer, resale price
//! and collections (tdesktop `info/peer_gifts/*` and the gift view box in
//! `boxes/star_gift_box.cpp`). Shown in the received-gifts dialog for the
//! signed-in user's own gifts only. Upgrade and transfer spend Stars, so
//! each opens an inline confirmation that names the price.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_manage::{GiftFlow, GiftManageState};
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::group_digits;
use quill::state::Session;

pub(super) fn stars(n: i64) -> String {
    tr_with(
        keys::Q_MONEY_STARS_4,
        &[("group_digits", &(group_digits(n)).to_string())],
    )
}

pub(super) fn panel(cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_2()
        .rounded_md()
        .bg(cx.theme().secondary)
}

pub(super) fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

impl QuillApp {
    pub(super) fn gift_manage_mut(&mut self) -> Option<&mut GiftManageState> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.payments.gifts_manage)
        } else {
            self.demo_session
                .as_mut()
                .map(|s| &mut s.payments.gifts_manage)
        }
    }

    /// Show `flow` (or close the open one) and prefill its input.
    pub(super) fn set_gift_flow(
        &mut self,
        flow: Option<GiftFlow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (price, name) = {
            let session = self.session();
            let manage = session.map(|s| &s.payments.gifts_manage);
            let price = match (&flow, session) {
                (Some(GiftFlow::Resale { id }), Some(s)) => s
                    .payments
                    .hub
                    .gifts
                    .iter()
                    .find(|g| g.id == *id)
                    .filter(|g| g.manage.listed && !g.manage.resale_gram_only)
                    .map(|g| g.manage.resale_star_count.to_string()),
                _ => None,
            };
            let name = match (&flow, manage) {
                (Some(GiftFlow::RenameCollection(id)), Some(m)) => {
                    m.collection(*id).map(|c| c.name.clone())
                }
                _ => None,
            };
            (price.unwrap_or_default(), name.unwrap_or_default())
        };
        self.payments
            .gift_price_input
            .update(cx, |input, cx| input.set_value(price, window, cx));
        self.payments
            .gift_collection_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
        if let Some(manage) = self.gift_manage_mut() {
            manage.flow = flow;
        }
        cx.notify();
    }

    pub(super) fn note_gift_refused(&mut self, text: &str) {
        self.connection.set_note(StatusNote::error(text));
    }

    fn pick_gift_collection(&mut self, id: i32, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.select_gift_collection(id));
        if let Some(Err(_)) = sent {
            self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_OPEN_THE_COLLECTION));
        }
        cx.notify();
    }

    pub(super) fn pin_received_gift(&mut self, id: &str, pinned: bool, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.set_gift_pinned(id, pinned));
        if let Some(Err(_)) = sent {
            self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_CHANGE_THE_PINNED));
        }
        cx.notify();
    }

    pub(super) fn toggle_keep_details(&mut self, cx: &mut Context<Self>) {
        if let Some(GiftFlow::Upgrade { keep_details, .. }) =
            self.gift_manage_mut().and_then(|m| m.flow.as_mut())
        {
            *keep_details = !*keep_details;
        }
        cx.notify();
    }

    pub(super) fn confirm_upgrade(&mut self, id: &str, keep_details: bool, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.upgrade_received_gift(id, keep_details));
        if let Some(Err(_)) = sent {
            self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_UPGRADE_THE_GIFT));
        }
        cx.notify();
    }

    pub(super) fn confirm_resale(&mut self, id: &str, price: Option<i64>, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.set_received_gift_resale_price(id, price));
        if let Some(Err(_)) = sent {
            self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_CHANGE_THE_RESALE));
        }
        cx.notify();
    }

    pub(super) fn change_collection_membership(
        &mut self,
        collection_id: i32,
        gift_id: &str,
        add: bool,
        cx: &mut Context<Self>,
    ) {
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .change_gift_collection(collection_id, gift_id, add)
        });
        if let Some(Err(_)) = sent {
            self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_CHANGE_THE_COLLECTION));
        }
        cx.notify();
    }

    fn submit_collection_name(&mut self, rename: Option<i32>, cx: &mut Context<Self>) {
        let name = self
            .payments
            .gift_collection_input
            .read(cx)
            .value()
            .to_string();
        let sent = self.live.as_mut().map(|live| match rename {
            Some(id) => live.driver.rename_gift_collection(id, &name),
            None => live.driver.create_gift_collection(&name),
        });
        if let Some(Err(_)) = sent {
            self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_SAVE_THE_COLLECTION));
        }
        cx.notify();
    }

    /// "Delete Gift": tdesktop's confirm box (`AddDeleteGiftMenuButton`).
    pub(super) fn ask_delete_gift(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = cx.entity().downgrade();
        let id = id.to_string();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, id) = (app.clone(), id.clone());
            alert
                .title(tr(keys::GIFT_MENU_DELETE))
                .description(tr(keys::GIFT_DELETE_SURE_TEXT))
                .ok_text(tr(keys::BOX_DELETE))
                .ok_variant(ButtonVariant::Danger)
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| this.confirm_delete_gift(&id, cx));
                    true
                })
        });
    }

    pub(super) fn confirm_delete_gift(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.delete_received_gift(id).is_err() {
                self.note_gift_refused(tr(keys::Q_MONEY_DELETE_GIFT_NOT_SENT));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            // The demo has no connection: answer as the server would.
            session.payments.gifts_manage.deleting = Some(id.to_string());
            session.finish_gift_delete();
        }
        cx.notify();
    }

    fn confirm_delete_collection(&mut self, id: i32, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.delete_gift_collection(id));
        if let Some(Err(_)) = sent {
            self.note_gift_refused(tr(keys::Q_MONEY_COULD_NOT_DELETE_THE_COLLECTION));
        }
        cx.notify();
    }
}

/// The tabs above the gifts grid: All gifts and one per collection, plus the
/// owner's "New" button and the open collection's rename and delete.
pub(super) fn gift_collection_tabs(
    session: &Session,
    mine: bool,
    name_input: &Entity<InputState>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let manage = &session.payments.gifts_manage;
    let busy = session.payments.hub.gift_mutating;
    if manage.collections.is_empty() && !mine {
        return div().into_any_element();
    }
    let active = manage.active_collection;
    let mut tabs = div().flex().flex_wrap().gap_1();
    tabs = tabs.child(
        Button::new("gift-collection-all")
            .label(tr(keys::GIFT_COLLECTION_ALL))
            .small()
            .selected(active == 0)
            .on_click(cx.listener(|this, _, _, cx| this.pick_gift_collection(0, cx))),
    );
    for collection in &manage.collections {
        let id = collection.id;
        tabs = tabs.child(
            Button::new(format!("gift-collection-{id}"))
                .label(format!("{} ({})", collection.name, collection.gift_count))
                .small()
                .selected(active == id)
                .on_click(cx.listener(move |this, _, _, cx| this.pick_gift_collection(id, cx))),
        );
    }
    let can_add = (manage.collections.len() as i64) < manage.limits.collections();
    if mine && can_add {
        tabs = tabs.child(
            Button::new("gift-collection-new")
                .label(tr(keys::Q_MONEY_NEW_COLLECTION))
                .small()
                .ghost()
                .disabled(busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_gift_flow(Some(GiftFlow::NewCollection), window, cx)
                })),
        );
    }
    let mut col = div().flex().flex_col().gap_2().child(tabs);
    if mine && active != 0 && manage.flow.is_none() {
        col = col.child(
            div()
                .flex()
                .gap_1()
                .child(
                    Button::new("gift-collection-rename")
                        .label(tr(keys::Q_MONEY_RENAME))
                        .small()
                        .ghost()
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_gift_flow(Some(GiftFlow::RenameCollection(active)), window, cx)
                        })),
                )
                .child(
                    Button::new("gift-collection-delete")
                        .label(tr(keys::GIFT_COLLECTION_DELETE))
                        .small()
                        .ghost()
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_gift_flow(Some(GiftFlow::DeleteCollection(active)), window, cx)
                        })),
                ),
        );
    }
    match &manage.flow {
        Some(GiftFlow::NewCollection) => {
            col = col.child(collection_name_form(None, busy, name_input, cx));
        }
        Some(GiftFlow::RenameCollection(id)) => {
            col = col.child(collection_name_form(Some(*id), busy, name_input, cx));
        }
        Some(GiftFlow::DeleteCollection(id)) => {
            let id = *id;
            let name = manage
                .collection(id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            col = col.child(
                panel(cx)
                    .child(div().text_sm().child(tr_with(
                        keys::Q_LEFTOVERS_DELETE_COLLECTION_SURE,
                        &[("name", &name.to_string())],
                    )))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("gift-collection-delete-yes")
                                    .label(tr(keys::GIFT_COLLECTION_DELETE_BUTTON))
                                    .danger()
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.confirm_delete_collection(id, cx)
                                    })),
                            )
                            .child(
                                Button::new("gift-collection-delete-no")
                                    .label(tr(keys::Q_MONEY_KEEP))
                                    .ghost()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_gift_flow(None, window, cx)
                                    })),
                            ),
                    ),
            );
        }
        _ => {}
    }
    if mine && !can_add {
        col = col.child(hint(
            tr_with(
                keys::Q_MONEY_YOU_CAN_HAVE_UP_TO,
                &[("collections", &(manage.limits.collections()).to_string())],
            ),
            cx,
        ));
    }
    col.into_any_element()
}

fn collection_name_form(
    rename: Option<i32>,
    busy: bool,
    name_input: &Entity<InputState>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    panel(cx)
        .child(div().text_sm().child(if rename.is_some() {
            tr(keys::Q_MONEY_RENAME_THE_COLLECTION)
        } else {
            tr(keys::Q_MONEY_NAME_THE_NEW_COLLECTION)
        }))
        .child(Input::new(name_input).aria_label(tr(keys::Q_MONEY_COLLECTION_NAME)))
        .child(hint(tr(keys::Q_MONEY_UP_TO_12_CHARACTERS_ADD), cx))
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("gift-collection-save")
                        .label(if rename.is_some() {
                            tr(keys::SETTINGS_SAVE)
                        } else {
                            tr(keys::GIFT_COLLECTION_NEW_CREATE)
                        })
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.submit_collection_name(rename, cx)
                        })),
                )
                .child(
                    Button::new("gift-collection-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(
                            cx.listener(|this, _, window, cx| this.set_gift_flow(None, window, cx)),
                        ),
                ),
        )
}
