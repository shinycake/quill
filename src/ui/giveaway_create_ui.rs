//! The "Boosts via Gifts" form (tdesktop `CreateGiveawayBox` in
//! `info/channel_statistics/boosts/create_giveaway_box.cpp`): prize,
//! channels, who may take part, an additional prize, the end date and
//! whether winners are shown.
//!
//! A giveaway that is already paid for starts from here
//! (`launchPrepaidGiveaway`, after an "are you sure" step). A new one opens
//! the payment checkout with the chosen option's price
//! (`ui/invoice_checkout.rs`); the giveaway is created when it is paid.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::giveaway_state::{GiveawayForm, GiveawayPage};
use super::link_info_boxes::finish_tracked;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::InputState;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::giveaway::{
    GiveawayDraft, GiveawayKind, premium_options_for, premium_winner_counts,
    settle_gift_code_months, settle_premium, settle_stars,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_time::now_unix;
use quill::premium_hub::group_digits;
use quill::state::{GiveawayLaunch, GiveawayOptions, Session};
use quill::telegram::envelope::{
    ChatKind, GiveawayPrize, ParsedPrepaidGiveaway, PremiumGiveawayOption, StarGiveawayOption,
    format_payment_price,
};

fn title() -> &'static str {
    tr(keys::GIVEAWAY_NEW_TITLE)
}

/// "5 Telegram Premium subscriptions for 3 months", or the Stars prize.
pub(super) fn prepaid_summary(prize: GiveawayPrize, winners: i32) -> String {
    match prize {
        GiveawayPrize::Premium { month_count } => format!(
            "{winners} Telegram Premium {} for {}",
            if winners == 1 {
                "subscription"
            } else {
                "subscriptions"
            },
            quill::deep_link_types::gift_duration(i64::from(month_count), 0)
        ),
        GiveawayPrize::Stars { star_count } => format!(
            "{} Stars among {winners} {}",
            group_digits(star_count),
            if winners == 1 { "winner" } else { "winners" }
        ),
    }
}

/// "12 boosts" / "1 boost".
pub(super) fn boosts_text(count: i32) -> String {
    trn(keys::BOOSTS_LIST_TITLE, count)
}

pub(super) fn choice(id: impl Into<ElementId>, label: String, selected: bool) -> Button {
    let button = Button::new(id).label(label).small();
    if selected {
        button.primary()
    } else {
        button.ghost()
    }
}

pub(super) fn section_title(text: &'static str) -> Div {
    div()
        .pt_3()
        .text_xs()
        .font_semibold()
        .text_color(text_muted())
        .child(text)
}

pub(super) fn about_line(text: impl Into<SharedString>) -> Div {
    div().text_xs().text_color(text_muted()).child(text.into())
}

impl QuillApp {
    /// Open the form. `prepaid` is the giveaway picked from the list.
    pub(super) fn open_giveaway_form(
        &mut self,
        chat_id: ChatId,
        prepaid: Option<ParsedPrepaidGiveaway>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = now_unix() as i32;
        let new_giveaway = prepaid.is_none();
        let prize_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::GIVEAWAY_ADDITIONAL_PRIZES_PH))
                .submit_on_enter(false)
        });
        if let Some(live) = self.live.as_mut() {
            live.driver.session.payments.giveaway.launch = GiveawayLaunch::Idle;
            if new_giveaway {
                let _ = live.driver.request_giveaway_options(chat_id.0);
            }
        }
        self.giveaway.form = Some(GiveawayForm {
            draft: GiveawayDraft::new(chat_id.0, prepaid, now),
            prize_input,
            note: None,
            confirming: false,
            page: GiveawayPage::Form,
        });
        cx.notify();
    }

    fn close_giveaway_form(&mut self, cx: &mut Context<Self>) {
        let boosted = self.giveaway.form.take().map(|f| f.draft.boosted_chat_id);
        if let Some(live) = self.live.as_mut() {
            let launched = live.driver.session.payments.giveaway.launch == GiveawayLaunch::Launched;
            if !matches!(
                live.driver.session.payments.giveaway.launch,
                GiveawayLaunch::Launching { .. }
            ) {
                live.driver.session.payments.giveaway.launch = GiveawayLaunch::Idle;
            }
            if launched && let Some(chat) = boosted {
                let _ = live.driver.fetch_chat_boost_status(ChatId(chat));
            }
        }
        cx.notify();
    }

    /// The draft with its option fields settled on what is offered.
    fn settled_draft(&self) -> Option<GiveawayDraft> {
        let form = self.giveaway.form.as_ref()?;
        let options = &self.session()?.payments.giveaway.options;
        let mut draft = form.draft.clone();
        draft.limits = self.session()?.payments.giveaway.limits;
        if draft.prepaid.is_none() {
            match draft.kind {
                GiveawayKind::Premium => settle_premium(&mut draft, &options.premium),
                GiveawayKind::Stars => settle_stars(&mut draft, &options.stars),
            }
            if draft.is_specific_users() {
                settle_gift_code_months(&mut draft, &options.premium);
            }
        }
        Some(draft)
    }

    /// Change the draft through `edit`, then settle it again.
    pub(super) fn edit_giveaway(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut GiveawayDraft),
    ) {
        let Some(mut draft) = self.settled_draft() else {
            return;
        };
        edit(&mut draft);
        if draft.prepaid.is_none()
            && let Some(options) = self.session().map(|s| s.payments.giveaway.options.clone())
        {
            match draft.kind {
                GiveawayKind::Premium => settle_premium(&mut draft, &options.premium),
                GiveawayKind::Stars => settle_stars(&mut draft, &options.stars),
            }
            if draft.is_specific_users() {
                settle_gift_code_months(&mut draft, &options.premium);
            }
        }
        if let Some(form) = self.giveaway.form.as_mut() {
            form.draft = draft;
            form.note = None;
        }
        cx.notify();
    }

    pub(super) fn toggle_giveaway_chat(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let limits = self.giveaway_limits();
        let mut refused = None;
        self.edit_giveaway(cx, |draft| {
            refused = draft.toggle_chat(chat_id).err();
        });
        if let (Some(error), Some(form)) = (refused, self.giveaway.form.as_mut()) {
            form.note = Some(error.text_for(&limits));
        }
    }

    /// The draft as the user sees it, with the prize text from the field.
    pub(super) fn giveaway_draft_now(&self, cx: &App) -> Option<GiveawayDraft> {
        let mut draft = self.settled_draft()?;
        let form = self.giveaway.form.as_ref()?;
        draft.prize_description = form.prize_input.read(cx).value().to_string();
        Some(draft)
    }

    /// "Start Giveaway" on a prepaid giveaway: check, then ask once more.
    fn ask_to_start_giveaway(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.giveaway_draft_now(cx) else {
            return;
        };
        let verdict = draft.validate(now_unix() as i32);
        let limits = draft.limits;
        if let Some(form) = self.giveaway.form.as_mut() {
            match verdict {
                Ok(()) => form.confirming = true,
                Err(error) => form.note = Some(error.text_for(&limits)),
            }
        }
        cx.notify();
    }

    /// The confirmed start. The only place that sends
    /// `launchPrepaidGiveaway`.
    fn start_prepaid_giveaway(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.giveaway_draft_now(cx) else {
            return;
        };
        let note = match self.live.as_mut() {
            Some(live) => live
                .driver
                .launch_prepaid_giveaway(&draft, now_unix() as i32)
                .err()
                .map(|_| tr(keys::Q_MONEY_COULDNT_SEND_THE_REQUEST_TRY).to_string()),
            None => Some(tr(keys::Q_MONEY_STARTING_A_GIVEAWAY_NEEDS_A).to_string()),
        };
        if let Some(form) = self.giveaway.form.as_mut() {
            form.confirming = false;
            form.note = note;
        }
        cx.notify();
    }

    pub(super) fn giveaway_group(session: &Session, chat_id: i64) -> bool {
        session.chats.get(&chat_id).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: false,
                    ..
                }
            )
        })
    }

    fn giveaway_prize_section(
        &self,
        draft: &GiveawayDraft,
        options: &GiveawayOptions,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut section = div().flex().flex_col().gap_1();
        if let Some(prepaid) = &draft.prepaid {
            return section
                .child(section_title(tr(keys::BOOSTS_PREPAID_GIVEAWAY_SINGLE)))
                .child(
                    div()
                        .id("giveaway-prepaid")
                        .text_sm()
                        .child(prepaid_summary(prepaid.prize, prepaid.winner_count)),
                )
                .child(about_line(tr(keys::Q_MONEY_YOU_ALREADY_PAID_FOR_THIS)));
        }
        let have_stars = !options.stars.is_empty();
        if have_stars {
            section = section.child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        choice(
                            "giveaway-kind-premium",
                            tr(keys::BOTS_CREATE_LIMIT_LINK).into(),
                            draft.kind == GiveawayKind::Premium,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_giveaway(cx, |d| d.kind = GiveawayKind::Premium);
                        })),
                    )
                    .child(
                        choice(
                            "giveaway-kind-stars",
                            tr(keys::Q_MONEY_STARS).into(),
                            draft.kind == GiveawayKind::Stars,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_giveaway(cx, |d| d.kind = GiveawayKind::Stars);
                        })),
                    ),
            );
        }
        let loading = options.loading() && options.premium.is_empty() && options.stars.is_empty();
        if loading {
            return section.child(about_line(tr(keys::Q_MONEY_LOADING_PRICES)));
        }
        if draft.kind == GiveawayKind::Premium {
            if options.premium.is_empty() {
                let error = options.premium_error.clone().unwrap_or_else(|| {
                    tr(keys::Q_MONEY_PREMIUM_GIVEAWAYS_ARE_NOT_AVAILABLE).to_string()
                });
                return section.child(about_line(error));
            }
            if draft.is_specific_users() {
                return self.giveaway_gift_code_months(section, draft, options, cx);
            }
            section = section.child(section_title(tr(keys::GIVEAWAY_QUANTITY_TITLE)));
            let mut counts = div().flex().flex_wrap().gap_1();
            for winners in premium_winner_counts(&options.premium) {
                counts = counts.child(
                    choice(
                        ("giveaway-winners", winners as u64),
                        winners.to_string(),
                        winners == draft.premium_winners,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_giveaway(cx, |d| d.premium_winners = winners);
                    })),
                );
            }
            section = section
                .child(counts)
                .child(about_line(tr(keys::GIVEAWAY_QUANTITY_ABOUT)))
                .child(section_title(tr(
                    keys::Q_MONEY_DURATION_OF_PREMIUM_SUBSCRIPTIONS,
                )));
            let mut months = div().flex().flex_col().gap_1();
            for option in premium_options_for(&options.premium, draft.premium_winners) {
                let month_count = option.month_count;
                months = months.child(
                    choice(
                        ("giveaway-months", month_count as u64),
                        format!(
                            "{} · {}",
                            quill::deep_link_types::gift_duration(i64::from(month_count), 0),
                            format_payment_price(&option.currency, option.amount)
                        ),
                        month_count == draft.premium_months,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_giveaway(cx, |d| d.premium_months = month_count);
                    })),
                );
            }
            return section.child(months);
        }
        if options.stars.is_empty() {
            let error = options
                .stars_error
                .clone()
                .unwrap_or_else(|| tr(keys::Q_MONEY_STARS_GIVEAWAYS_ARE_NOT_AVAILABLE).to_string());
            return section.child(about_line(error));
        }
        section = section.child(section_title(tr(keys::GIVEAWAY_CREDITS_OPTIONS_TITLE)));
        let mut rows = div().flex().flex_col().gap_1();
        for (index, option) in options.stars.iter().enumerate() {
            rows = rows.child(
                choice(
                    ("giveaway-stars", index as u64),
                    format!(
                        "{} Stars · {} · {}",
                        group_digits(option.star_count),
                        format_payment_price(&option.currency, option.amount),
                        boosts_text(option.yearly_boost_count)
                    ),
                    index == draft.star_option,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit_giveaway(cx, |d| {
                        d.star_option = index;
                        d.star_winners = 0;
                    });
                })),
            );
        }
        section = section
            .child(rows)
            .child(section_title(tr(keys::GIVEAWAY_CREDITS_QUANTITY_TITLE)));
        let mut winners = div().flex().flex_wrap().gap_1();
        for count in options
            .stars
            .get(draft.star_option)
            .map(|o| o.winner_counts.clone())
            .unwrap_or_default()
        {
            winners = winners.child(
                choice(
                    ("giveaway-star-winners", count as u64),
                    count.to_string(),
                    count == draft.star_winners,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit_giveaway(cx, |d| d.star_winners = count);
                })),
            );
        }
        section
            .child(winners)
            .child(about_line(tr(keys::GIVEAWAY_CREDITS_QUANTITY_ABOUT)))
    }

    /// The price line of a giveaway that is not paid for yet.
    fn giveaway_price(draft: &GiveawayDraft, options: &GiveawayOptions) -> Option<String> {
        if draft.prepaid.is_some() {
            return None;
        }
        if draft.is_specific_users() {
            return Self::gift_code_price_line(draft, options);
        }
        match draft.kind {
            GiveawayKind::Premium => options
                .premium
                .iter()
                .find(|o| {
                    o.winner_count == draft.premium_winners && o.month_count == draft.premium_months
                })
                .map(price_of_premium),
            GiveawayKind::Stars => options.stars.get(draft.star_option).map(price_of_stars),
        }
    }

    fn giveaway_form_body(&self, form: &GiveawayForm, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let draft = self.settled_draft().unwrap_or_else(|| form.draft.clone());
        let options = &session.payments.giveaway.options;
        let giveaway = &session.payments.giveaway;
        let group = Self::giveaway_group(session, draft.boosted_chat_id);
        let place = if group { "group" } else { "channel" };
        let chat_name = session
            .chats
            .get(&draft.boosted_chat_id)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| tr(keys::Q_MONEY_THIS_CHAT).into());
        let boosts = draft.boost_count(&options.stars);

        if giveaway.launch == GiveawayLaunch::Launched {
            return div()
                .id("giveaway-launched")
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .font_semibold()
                        .child(tr(keys::GIVEAWAY_CREATED_TITLE)),
                )
                .child(about_line(tr_with(
                    keys::Q_MONEY_CHECK_YOUR_S_BOOSTS_TO,
                    &[("place", place)],
                )))
                .into_any_element();
        }
        if form.confirming {
            return div()
                .id("giveaway-confirm")
                .text_sm()
                .child(tr(keys::GIVEAWAY_START_SURE))
                .into_any_element();
        }

        if form.page != GiveawayPage::Form {
            return self.giveaway_pick_page(form, &draft, cx);
        }
        let mut body = div().flex().flex_col().gap_1();
        body = body.child(about_line(match (draft.kind, group) {
            (GiveawayKind::Stars, true) => tr(keys::GIVEAWAY_CREDITS_NEW_ABOUT_GROUP),
            (GiveawayKind::Stars, false) => tr(keys::GIVEAWAY_CREDITS_NEW_ABOUT),
            (_, true) => tr(keys::GIVEAWAY_NEW_ABOUT_GROUP),
            (_, false) => tr(keys::GIVEAWAY_NEW_ABOUT),
        }));
        body = body.child(self.giveaway_prize_section(&draft, options, cx));
        body = self.giveaway_award_row(body, &draft, options, cx);

        if draft.is_specific_users() {
            body = body.child(about_line(tr_with(
                keys::Q_MONEY_THIS_WILL_RECEIVE,
                &[
                    ("chat_name", &chat_name.to_string()),
                    ("place", place),
                    ("boosts_text", &(boosts_text(boosts)).to_string()),
                ],
            )));
        } else {
            body = self.giveaway_random_sections(body, form, &draft, session, cx);
        }

        // Price and notes.
        if let Some(price) = Self::giveaway_price(&draft, options) {
            body = body
                .child(section_title(tr(keys::Q_MONEY_PRICE)))
                .child(div().id("giveaway-price").text_sm().child(price))
                .child(about_line(tr(
                    keys::Q_MONEY_START_GIVEAWAY_OPENS_THE_CHECKOUT,
                )));
        }
        let note = form.note.clone().or_else(|| match &giveaway.launch {
            GiveawayLaunch::Failed(text) => Some(text.clone()),
            _ => None,
        });
        if let Some(note) = note {
            body = body.child(
                div()
                    .id("giveaway-note")
                    .pt_2()
                    .text_sm()
                    .text_color(danger_dark())
                    .child(note),
            );
        }
        body.into_any_element()
    }

    pub(super) fn build_giveaway_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Giveaway, |this, _, cx| {
                this.close_giveaway_form(cx);
            });
        app.update(cx, |this, cx| {
            let Some(form) = this.giveaway.form.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(title()))
                    .on_close(on_close.clone());
            };
            let draft = this.settled_draft();
            let prepaid = draft.as_ref().is_some_and(|d| d.prepaid.is_some());
            let confirming = form.confirming;
            let launch = this
                .session()
                .map(|s| s.payments.giveaway.launch.clone())
                .unwrap_or_default();
            let launching = matches!(launch, GiveawayLaunch::Launching { .. });
            let launched = launch == GiveawayLaunch::Launched;
            let body = this.giveaway_form_body(form, cx);
            let close = |label: &'static str| {
                Button::new("giveaway-close")
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_giveaway_form(cx);
                        this.close_kit_dialog_if_done(DialogKind::Giveaway, window, cx);
                    }))
            };
            let footer = div().flex().justify_end().gap_2();
            let page = form.page;
            let footer = if page != GiveawayPage::Form {
                footer.child(
                    Button::new("giveaway-page-done")
                        .label(tr(keys::BOX_DONE))
                        .on_click(cx.listener(|this, _, _, cx| this.close_giveaway_page(cx))),
                )
            } else if launched {
                footer.child(close(tr(keys::BOX_DONE)))
            } else if confirming {
                footer
                    .child(
                        Button::new("giveaway-confirm-no")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(form) = this.giveaway.form.as_mut() {
                                    form.confirming = false;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("giveaway-confirm-yes")
                            .label(tr(keys::GIVEAWAY_START))
                            .loading(launching)
                            .disabled(launching)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_prepaid_giveaway(cx);
                            })),
                    )
            } else {
                footer.child(close(tr(keys::CLOSE))).child(
                    Button::new("giveaway-start")
                        .label(tr(keys::GIVEAWAY_START))
                        .loading(launching)
                        .disabled(launching)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            // A prepaid giveaway asks once more; a new one
                            // opens the checkout, which is the confirmation.
                            if prepaid {
                                this.ask_to_start_giveaway(cx);
                            } else {
                                this.pay_new_giveaway(window, cx);
                            }
                        })),
                )
            };
            let scroll = Some(this.payments.pick_scroll.clone());
            finish_tracked(
                scroll,
                dialog,
                title(),
                body,
                footer.into_any_element(),
                on_close,
            )
        })
    }
}

fn price_of_premium(option: &PremiumGiveawayOption) -> String {
    format!(
        "{} for {} Premium {}",
        format_payment_price(&option.currency, option.amount),
        option.winner_count,
        if option.winner_count == 1 {
            "subscription"
        } else {
            "subscriptions"
        }
    )
}

fn price_of_stars(option: &StarGiveawayOption) -> String {
    tr_with(
        keys::Q_MONEY_FOR_STARS,
        &[
            (
                "amount",
                &(format_payment_price(&option.currency, option.amount)).to_string(),
            ),
            ("star_count", &(group_digits(option.star_count)).to_string()),
        ],
    )
}

crate::ui::shell::register_dialogs! {
    /// "Boosts via Gifts": start a prepaid giveaway or see a new one's price.
    Giveaway => DialogSpec::new(
        2943,
        // The checkout of a new giveaway takes the screen; the form comes
        // back when it closes.
        |app| app.giveaway.form.is_some() && !app.payment_dialog_visible(),
        QuillApp::build_giveaway_dialog,
    ),
}
