//! "Gift Premium or Stars" (tdesktop `ChooseStarGiftRecipient`,
//! boxes/star_gift_box.cpp): the account itself, contacts with a birthday
//! around today, frequent contacts and the other contacts. A row opens the
//! gift picker for that person. Opened by the birthday suggestion when
//! several contacts celebrate, and by Settings > Premium Gifting. Who is
//! listed, and in what order, is `quill::gift_recipients`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::message_text::kit_avatar_element;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_recipients::{Heading, People, RowStatus, Section, sections};
use quill::lang::{keys, tr, tr_with};
use quill::state::Session;
use quill::telegram::envelope::ChatKind;

/// The avatar beside each name.
const AVATAR: f32 = 32.;
/// Rows drawn at once; the search narrows the rest.
const ROWS_MAX: usize = 200;

impl QuillApp {
    pub(super) fn open_gift_recipients(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Payments,
        ) {
            return;
        }
        self.payments
            .gift_recipient_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(live) = self.live.as_mut()
            && live.driver.load_gift_recipients().is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULDNT_OPEN_THE_GIFTS)));
            cx.notify();
            return;
        }
        self.payments.gift_recipients_open = true;
        cx.notify();
    }

    fn close_gift_recipients(&mut self, cx: &mut Context<Self>) {
        self.payments.gift_recipients_open = false;
        cx.notify();
    }

    fn gift_recipient_picked(&mut self, user_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.close_gift_recipients(cx);
        self.close_kit_dialog_if_done(DialogKind::GiftRecipients, window, cx);
        self.open_send_gift_to_user(user_id, window, cx);
    }

    fn build_gift_recipients_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GiftRecipients, |this, _, cx| {
                this.close_gift_recipients(cx);
            });
        app.update(cx, |this, cx| {
            let query = this
                .payments
                .gift_recipient_search
                .read(cx)
                .value()
                .to_string();
            let mut body = div().flex().flex_col().gap_1().child(
                div().pb_2().child(
                    Input::new(&this.payments.gift_recipient_search)
                        .aria_label(tr(keys::GIFT_RECIPIENT_SEARCH)),
                ),
            );
            if let Some(session) = this.session() {
                let mut shown = 0;
                for section in recipient_sections(session, &query) {
                    if let Some(heading) = section.heading {
                        body = body.child(heading_label(heading, cx));
                    }
                    for row in section.rows {
                        if shown == ROWS_MAX {
                            break;
                        }
                        shown += 1;
                        body = body.child(recipient_row(session, row.user_id, row.status, cx));
                    }
                }
            }
            let footer = div().flex().justify_end().child(
                Button::new("gift-recipients-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_gift_recipients(cx);
                        this.close_kit_dialog_if_done(DialogKind::GiftRecipients, window, cx);
                    })),
            );
            finish_dialog(
                dialog,
                tr(keys::GIFT_PREMIUM_OR_STARS).to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

/// The chooser's sections from what the session knows today.
fn recipient_sections(session: &Session, query: &str) -> Vec<Section> {
    let today = quill::local_time::civil_local(quill::local_time::now_unix());
    let frequent: Vec<i64> = if session.search.search.top_chats_disabled {
        Vec::new()
    } else {
        session
            .search
            .search
            .top_chats
            .iter()
            .filter_map(|chat| match session.chats.get(&chat.0)?.kind {
                ChatKind::Private { user_id } => Some(user_id.0),
                _ => None,
            })
            .collect()
    };
    let contacts = session.users_state.contacts.as_deref().unwrap_or_default();
    let people = People {
        me: session.my_user_id,
        birthdays: &session.chat_list.suggestions.close_birthdays,
        frequent: &frequent,
        contacts,
        month: today.month,
        day: today.day,
    };
    sections(
        &people,
        query,
        |id| session.gift_recipient_user(id).is_some(),
        |id| {
            session
                .user(id)
                .map(|u| u.display_name())
                .unwrap_or_default()
        },
    )
}

fn heading_label(heading: Heading, cx: &App) -> impl IntoElement {
    let key = match heading {
        Heading::Birthdays => keys::GIFT_SUBTITLE_BIRTHDAYS,
        Heading::Frequent => keys::SETTINGS_TOP_PEERS_TITLE,
        Heading::Contacts => keys::CONTACTS_HEADER,
    };
    div()
        .pt_3()
        .px_2()
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(tr(key))
}

fn recipient_row(
    session: &Session,
    user_id: i64,
    status: RowStatus,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let name = session
        .user(user_id)
        .map(|user| user.display_name())
        .unwrap_or_default();
    let status = match status {
        RowStatus::Yourself => Some(tr(keys::GIFT_SELF_STATUS).to_string()),
        RowStatus::BirthdayToday => Some(tr_with(
            keys::GIFT_LIST_BIRTHDAY_STATUS_TODAY,
            &[("emoji", "\u{1F382}")],
        )),
        RowStatus::BirthdayYesterday => {
            Some(tr(keys::GIFT_LIST_BIRTHDAY_STATUS_YESTERDAY).to_string())
        }
        RowStatus::BirthdayTomorrow => {
            Some(tr(keys::GIFT_LIST_BIRTHDAY_STATUS_TOMORROW).to_string())
        }
        RowStatus::Plain => None,
    };
    let label = match status.as_deref() {
        Some(status) => format!("{name}, {status}"),
        None => name.clone(),
    };
    let photo = session.user_photo_path(user_id).map(std::path::Path::new);
    let muted = cx.theme().muted_foreground;
    Button::new(("gift-recipient", user_id.unsigned_abs()))
        .ghost()
        .w_full()
        .accessibility_label(label)
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .w_full()
                .py_0p5()
                .child(kit_avatar_element(&name, photo, px(AVATAR)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .min_w_0()
                        .child(div().text_sm().truncate().child(name))
                        .children(
                            status.map(|status| div().text_xs().text_color(muted).child(status)),
                        ),
                ),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.gift_recipient_picked(user_id, window, cx);
        }))
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    /// "Gift Premium or Stars": pick who gets a gift.
    GiftRecipients => DialogSpec::new(
        3760,
        |app| app.payments.gift_recipients_open,
        QuillApp::build_gift_recipients_dialog,
    )
    .closes(|app, _, cx| app.close_gift_recipients(cx)),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "gift_recipients_tests.rs"]
mod tests;
