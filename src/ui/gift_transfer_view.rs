//! The content of each step of the transfer picker (`gift_transfer_ui`):
//! title, body and footer, after tdesktop `boxes/transfer_gift_box.cpp`
//! and `boxes/passcode_box.cpp` (`PasscodeBox` in check mode,
//! `TransferPasswordError`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::fragment_password::{PasswordBox, SecurityCheck};
use super::gift_market_ui::collectible_tile;
use super::gift_resale_buy::attribute_row;
use super::gift_transfer_ui::{balance_hint, transfer_price};
use super::message_text::kit_avatar_element;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_manage::{
    ExportError, ExportState, TransferCandidate, TransferGate, TransferPicker, TransferStage,
    UnlockIn, transfer_candidates,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_time::now_unix;
use quill::premium_hub::ReceivedGift;
use quill::state::Session;
use quill::telegram::envelope::ChatKind;
use std::rc::Rc;

/// Rows drawn at once; the search narrows the rest.
const ROWS_MAX: usize = 150;
/// The row avatar (tdesktop `peerListItem.photoSize`).
const AVATAR: f32 = 40.;

/// One step of the picker as the dialog shows it.
pub(super) struct StageView {
    /// `None` for the confirmation, which tdesktop shows without a title.
    pub(super) title: Option<String>,
    pub(super) body: AnyElement,
    pub(super) footer: AnyElement,
}

fn footer() -> Div {
    div().flex().justify_end().gap_2()
}

fn cancel_button(id: &'static str, cx: &mut Context<QuillApp>) -> Button {
    Button::new(id)
        .label(tr(keys::CANCEL))
        .ghost()
        .on_click(cx.listener(|this, _, window, cx| this.transfer_step_back(window, cx)))
}

fn text_block(text: impl Into<SharedString>) -> Div {
    div().text_sm().child(text.into())
}

/// The step of the open picker, `None` when no picker is open.
pub(super) fn stage_view(app: &QuillApp, cx: &mut Context<QuillApp>) -> Option<StageView> {
    let session = app.session()?;
    let picker = session.payments.gifts_manage.transfer()?;
    let gift = session
        .payments
        .hub
        .gifts
        .iter()
        .find(|g| g.id == picker.id);
    let name = picker.gift_name.clone();
    Some(match &picker.stage {
        TransferStage::Pick => pick_view(app, session, picker, gift, cx),
        TransferStage::ConfirmPeer { user_id, name: to } => {
            confirm_peer_view(app, session, gift, &name, *user_id, to, cx)
        }
        TransferStage::Locked { hours } => locked_view(*hours, cx),
        TransferStage::ConfirmExport => confirm_export_view(&name, false, cx),
        TransferStage::Probing => confirm_export_view(&name, true, cx),
        TransferStage::Password { sending, error } => {
            password_view(app, session, *sending, error.as_ref(), cx)
        }
        TransferStage::SecurityCheck { later } => security_check_view(*later, cx),
        TransferStage::ActionLocked => action_locked_view(cx),
    })
}

/// The people of the list, built once per query and session revision
/// rather than on every frame.
pub(super) struct CandidateCache {
    revision: u64,
    query: String,
    open_chat: Option<ChatId>,
    people: Rc<Vec<TransferCandidate>>,
}

fn cached_candidates(app: &QuillApp, session: &Session, query: &str) -> Rc<Vec<TransferCandidate>> {
    // Never panics: a busy cache (it cannot be, render is not re-entrant)
    // just rebuilds.
    let Ok(mut cache) = app.payments.transfer_people.try_borrow_mut() else {
        return Rc::new(candidates(session, query));
    };
    if let Some(hit) = cache.as_ref().filter(|c| {
        c.revision == session.revision && c.query == query && c.open_chat == session.open_chat
    }) {
        return hit.people.clone();
    }
    let people = Rc::new(candidates(session, query));
    *cache = Some(CandidateCache {
        revision: session.revision,
        query: query.to_string(),
        open_chat: session.open_chat,
        people: people.clone(),
    });
    people
}

/// The people the list shows for `query`: the user of the open private
/// chat first (the design's preselected row: one click reaches the
/// confirmation), the contacts, and once a search starts the other
/// people the user has a private chat with.
pub(super) fn candidates(session: &Session, query: &str) -> Vec<TransferCandidate> {
    let is_bot = |id: i64| session.users.get(&id).is_some_and(|u| u.is_bot);
    let contacts = session.contact_rows();
    let open_user = session
        .open_chat
        .and_then(|id| session.chats.get(&id.0))
        .and_then(|chat| match chat.kind {
            ChatKind::Private { user_id } => session.users.get(&user_id.0),
            _ => None,
        });
    let mut rows: Vec<(i64, String, String, bool)> = open_user
        .map(|user| {
            (
                user.id,
                user.display_name(),
                user.status.display(),
                user.is_bot,
            )
        })
        .into_iter()
        .collect();
    rows.extend(contacts.iter().map(|r| {
        (
            r.user_id,
            r.name.clone(),
            r.status_text.clone(),
            is_bot(r.user_id),
        )
    }));
    if !query.trim().is_empty() {
        let mut others: Vec<(i64, String, String, bool)> = session
            .chats
            .values()
            .filter_map(|chat| match chat.kind {
                ChatKind::Private { user_id } => session.users.get(&user_id.0),
                _ => None,
            })
            .filter(|user| !contacts.iter().any(|r| r.user_id == user.id))
            .map(|user| {
                (
                    user.id,
                    user.display_name(),
                    user.status.display(),
                    user.is_bot,
                )
            })
            .collect();
        others.sort_by_key(|(_, name, _, _)| name.to_lowercase());
        rows.extend(others);
    }
    transfer_candidates(
        rows.iter()
            .map(|(id, name, status, bot)| (*id, name.as_str(), status.as_str(), *bot)),
        session.my_user_id,
        query,
    )
}

fn pick_view(
    app: &QuillApp,
    session: &Session,
    picker: &TransferPicker,
    gift: Option<&ReceivedGift>,
    cx: &mut Context<QuillApp>,
) -> StageView {
    let muted = cx.theme().muted_foreground;
    let query = app.payments.pick_search.read(cx).value().to_string();
    let mut body = v_flex()
        .gap_2()
        .child(Input::new(&app.payments.pick_search).aria_label(tr(keys::Q_MONEY_SEARCH)));
    let export = gift.map_or(ExportState::Unavailable, |g| g.export_state(now_unix()));
    if export != ExportState::Unavailable && query.trim().is_empty() {
        body = body.child(export_row(export, session.ton_wallet.can_use, cx));
    }
    body = body.child(
        div()
            .pt_1()
            .text_xs()
            .font_semibold()
            .text_color(muted)
            .child(tr(keys::CONTACTS_HEADER)),
    );
    let people = cached_candidates(app, session, &query);
    let roots = app.media_display_roots();
    if people.is_empty() {
        let empty = if session.contacts_settled() {
            tr(keys::CONTACTS_NOT_FOUND)
        } else {
            tr(keys::CONTACTS_LOADING)
        };
        body = body.child(div().py_2().text_sm().text_color(muted).child(empty));
    }
    let mut list = v_flex();
    for person in people.iter().take(ROWS_MAX).cloned() {
        let photo = session
            .user_photo_path(person.user_id)
            .and_then(|path| quill::local_path::sandboxed_display_path(path, &roots));
        let user_id = person.user_id;
        let name = person.name.clone();
        list = list.child(
            ListItem::new(("gift-transfer-contact", user_id.unsigned_abs()))
                .rounded_md()
                .px_2()
                .tab_index(0)
                .accessibility_label(person.name.clone())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pick_transfer_contact(user_id, name.clone(), cx)
                }))
                .child(
                    h_flex()
                        .gap_3()
                        .min_w_0()
                        .child(kit_avatar_element(
                            &person.name,
                            photo.as_deref(),
                            px(AVATAR),
                        ))
                        .child(
                            v_flex()
                                .min_w_0()
                                .child(div().text_sm().font_medium().truncate().child(person.name))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .truncate()
                                        .child(person.status),
                                ),
                        ),
                ),
        );
    }
    body = body.child(list);
    StageView {
        title: Some(tr_with(
            keys::GIFT_TRANSFER_TITLE,
            &[("name", &picker.gift_name)],
        )),
        body: body.into_any_element(),
        footer: footer()
            .child(
                Button::new("gift-transfer-close")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.close_gift_transfer(window, cx)),
                    ),
            )
            .into_any_element(),
    }
}

/// the TON userpic, "Send via Blockchain" (or "Move
/// to My Wallet" while the wallet is on for the account) and, while
/// locked, "unlocks in ..." under it.
fn export_row(state: ExportState, to_wallet: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    let status = match state {
        ExportState::Locked { hours } => Some(match UnlockIn::of(hours) {
            UnlockIn::Days(days) => trn(keys::GIFT_TRANSFER_UNLOCKS_DAYS, days),
            UnlockIn::Hours(hours) => trn(keys::GIFT_TRANSFER_UNLOCKS_HOURS, hours),
        }),
        _ => None,
    };
    let label = if to_wallet {
        tr(keys::GIFT_TRANSFER_TO_WALLET)
    } else {
        tr(keys::GIFT_TRANSFER_VIA_BLOCKCHAIN)
    };
    let a11y = match &status {
        Some(status) => format!("{label}, {status}"),
        None => label.to_string(),
    };
    let theme = cx.theme();
    let (primary, on_primary, muted) = (
        theme.primary,
        theme.primary_foreground,
        theme.muted_foreground,
    );
    ListItem::new("gift-transfer-export")
        .rounded_md()
        .px_2()
        .tab_index(0)
        .accessibility_label(a11y)
        .on_click(cx.listener(|this, _, _, cx| this.tap_transfer_export(cx)))
        .child(
            h_flex()
                .gap_3()
                .child(
                    div()
                        .size(px(AVATAR))
                        .flex_none()
                        .rounded_full()
                        .bg(primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(super::earn::ton_icon(AVATAR * 0.5, on_primary)),
                )
                .child(
                    v_flex()
                        .child(div().text_sm().font_medium().child(label))
                        .children(status.map(|s| div().text_xs().text_color(muted).child(s))),
                ),
        )
        .into_any_element()
}

/// tdesktop `ShowTransferToBox`: the gift, an arrow and the recipient, the
/// question, the gift's attributes and Transfer / Cancel.
fn confirm_peer_view(
    app: &QuillApp,
    session: &Session,
    gift: Option<&ReceivedGift>,
    gift_name: &str,
    user_id: i64,
    to: &str,
    cx: &mut Context<QuillApp>,
) -> StageView {
    let star_count = match gift.map(|g| g.transfer_gate(true, now_unix())) {
        Some(TransferGate::Ready { star_count }) => star_count,
        _ => 0,
    };
    let price = transfer_price(star_count);
    let balance = session.payments.hub.balance.map(|b| b.stars);
    let too_poor = star_count > 0 && balance.is_some_and(|b| b < star_count);
    let busy = session.payments.hub.gift_mutating;
    let roots = app.media_display_roots();
    let collectible = gift.and_then(|g| g.gift.collectible.as_deref());
    let photo = session
        .user_photo_path(user_id)
        .and_then(|path| quill::local_path::sandboxed_display_path(path, &roots));
    let mut pair = h_flex().justify_center().items_center().gap_4();
    if let Some(c) = collectible {
        pair = pair.child(collectible_tile(
            ("gift-transfer-cover", 0),
            c,
            session,
            &roots,
            64.,
        ));
    }
    pair = pair
        .child(
            Icon::new(IconName::ArrowRight)
                .with_size(px(20.))
                .text_color(cx.theme().muted_foreground),
        )
        .child(kit_avatar_element(to, photo.as_deref(), px(64.)));
    let question = match &price {
        Some(price) => tr_with(
            keys::GIFT_TRANSFER_SURE_FOR,
            &[("name", gift_name), ("recipient", to), ("price", price)],
        ),
        None => tr_with(
            keys::GIFT_TRANSFER_SURE,
            &[("name", gift_name), ("recipient", to)],
        ),
    };
    let mut body = v_flex().gap_3().child(pair).child(text_block(question));
    if too_poor {
        body = body.child(balance_hint(cx, balance));
    }
    if let Some(c) = collectible {
        body = body.child(
            v_flex()
                .gap_2()
                .child(attribute_row(
                    tr(keys::GIFT_RESALE_MODEL),
                    &c.model.name,
                    c.model.rarity,
                    cx,
                ))
                .child(attribute_row(
                    tr(keys::GIFT_RESALE_BACKDROP),
                    &c.backdrop.name,
                    c.backdrop.rarity,
                    cx,
                ))
                .child(attribute_row(
                    tr(keys::GIFT_RESALE_SYMBOL),
                    &c.symbol.name,
                    c.symbol.rarity,
                    cx,
                )),
        );
    }
    let label = match &price {
        Some(price) => tr_with(keys::GIFT_TRANSFER_BUTTON_FOR, &[("price", price)]),
        None => tr(keys::GIFT_TRANSFER_BUTTON).to_string(),
    };
    StageView {
        title: None,
        body: body.into_any_element(),
        footer: footer()
            .child(cancel_button("gift-transfer-peer-cancel", cx))
            .child(
                Button::new("gift-transfer-peer-confirm")
                    .label(label)
                    .primary()
                    .loading(busy)
                    .disabled(busy || too_poor)
                    .on_click(cx.listener(|this, _, _, cx| this.confirm_picked_transfer(cx))),
            )
            .into_any_element(),
    }
}

/// tdesktop's inform box "Unlocking in progress".
fn locked_view(hours: i64, cx: &mut Context<QuillApp>) -> StageView {
    let when = match UnlockIn::of(hours) {
        UnlockIn::Days(days) => trn(keys::GIFT_TRANSFER_UNLOCKS_WHEN_DAYS, days),
        UnlockIn::Hours(hours) => trn(keys::GIFT_TRANSFER_UNLOCKS_WHEN_HOURS, hours),
    };
    StageView {
        title: Some(tr(keys::GIFT_TRANSFER_UNLOCKS_TITLE).to_string()),
        body: text_block(tr_with(
            keys::GIFT_TRANSFER_UNLOCKS_ABOUT,
            &[("when", &when)],
        ))
        .into_any_element(),
        footer: footer()
            .child(
                Button::new("gift-transfer-locked-ok")
                    .label(tr(keys::BOX_OK))
                    .primary()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.transfer_step_back(window, cx)),
                    ),
            )
            .into_any_element(),
    }
}

/// tdesktop `ConfirmExportBox`; `probing` while the password-less try
/// after "Open Fragment" is out.
fn confirm_export_view(gift_name: &str, probing: bool, cx: &mut Context<QuillApp>) -> StageView {
    StageView {
        title: Some(tr(keys::GIFT_TRANSFER_CONFIRM_TITLE).to_string()),
        body: text_block(tr_with(
            keys::GIFT_TRANSFER_CONFIRM_TEXT,
            &[("name", gift_name)],
        ))
        .into_any_element(),
        footer: footer()
            .child(cancel_button("gift-transfer-export-cancel", cx).disabled(probing))
            .child(
                Button::new("gift-transfer-export-open")
                    .label(tr(keys::GIFT_TRANSFER_CONFIRM_BUTTON))
                    .primary()
                    .loading(probing)
                    .disabled(probing)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.confirm_transfer_fragment(window, cx)
                    })),
            )
            .into_any_element(),
    }
}

/// tdesktop `PasscodeBox` with the gift's custom title, description and
/// Submit (`fragment_password::PasswordBox`).
fn password_view(
    app: &QuillApp,
    session: &Session,
    sending: bool,
    error: Option<&ExportError>,
    cx: &mut Context<QuillApp>,
) -> StageView {
    let (body, footer) = PasswordBox {
        id: "gift-transfer",
        input: &app.payments.gift_export_password,
        description: tr(keys::GIFT_TRANSFER_PASSWORD_DESCRIPTION),
        sending,
        error,
        on_submit: QuillApp::submit_gift_export_password,
        on_cancel: QuillApp::transfer_step_back,
        on_forgot: QuillApp::forgot_gift_export_password,
    }
    .render(session, cx);
    StageView {
        title: Some(tr(keys::GIFT_TRANSFER_PASSWORD_TITLE).to_string()),
        body,
        footer,
    }
}

/// tdesktop `TransferPasswordError` with the gift's about line.
fn security_check_view(later: bool, cx: &mut Context<QuillApp>) -> StageView {
    let (body, footer) = SecurityCheck {
        id: "gift-transfer",
        about: tr(keys::GIFT_TRANSFER_PASSWORD_ABOUT),
        later,
        on_close: QuillApp::transfer_step_back,
        on_set_password: QuillApp::set_password_for_export,
    }
    .render(cx);
    StageView {
        title: Some(tr(keys::Q_MONEY_SECURITY_CHECK).to_string()),
        body,
        footer,
    }
}

/// tdesktop `ShowActionLocked`.
fn action_locked_view(cx: &mut Context<QuillApp>) -> StageView {
    StageView {
        title: Some(tr(keys::GIFT_TRANSFER_LOCKED_TITLE).to_string()),
        body: text_block(tr(keys::GIFT_TRANSFER_LOCKED_TEXT)).into_any_element(),
        footer: footer()
            .child(
                Button::new("gift-transfer-locked-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.close_gift_transfer(window, cx)),
                    ),
            )
            .child(
                Button::new("gift-transfer-locked-open")
                    .label(tr(keys::GIFT_TRANSFER_CONFIRM_BUTTON))
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_locked_gift_on_fragment(window, cx)
                    })),
            )
            .into_any_element(),
    }
}
