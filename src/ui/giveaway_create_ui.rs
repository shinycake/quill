//! The "Boosts via Gifts" form (tdesktop `CreateGiveawayBox` in
//! `info/channel_statistics/boosts/create_giveaway_box.cpp`): prize,
//! channels, who may take part, an additional prize, the end date and
//! whether winners are shown.
//!
//! A giveaway that is already paid for starts from here
//! (`launchPrepaidGiveaway`, after an "are you sure" step). A new one
//! stops at its price: paying needs the card checkout, which Quill has only
//! for invoice messages.

use super::app::QuillApp;
use super::giveaway_info_ui::day_text;
use super::giveaway_state::GiveawayForm;
use super::link_info_boxes::finish;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::giveaway::{
    ADDITIONAL_CHATS_MAX, GiveawayDraft, GiveawayKind, PERIOD_MAX_SECS, PRIZE_DESCRIPTION_MAX,
    premium_options_for, premium_winner_counts, settle_premium, settle_stars,
};
use quill::ids::ChatId;
use quill::local_time::now_unix;
use quill::premium_hub::group_digits;
use quill::state::{GiveawayLaunch, GiveawayOptions, Session};
use quill::telegram::envelope::{
    ChatKind, GiveawayPrize, ParsedPrepaidGiveaway, PremiumGiveawayOption, StarGiveawayOption,
    format_payment_price,
};

const TITLE: &str = "Boosts via Gifts";

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
fn boosts_text(count: i32) -> String {
    if count == 1 {
        "1 boost".into()
    } else {
        format!("{count} boosts")
    }
}

fn choice(id: impl Into<ElementId>, label: String, selected: bool) -> Button {
    let button = Button::new(id).label(label).small();
    if selected {
        button.primary()
    } else {
        button.ghost()
    }
}

fn section_title(text: &'static str) -> Div {
    div()
        .pt_3()
        .text_xs()
        .font_semibold()
        .text_color(text_muted())
        .child(text)
}

fn about_line(text: impl Into<SharedString>) -> Div {
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
                .placeholder("Enter your prize")
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
        if draft.prepaid.is_none() {
            match draft.kind {
                GiveawayKind::Premium => settle_premium(&mut draft, &options.premium),
                GiveawayKind::Stars => settle_stars(&mut draft, &options.stars),
            }
        }
        Some(draft)
    }

    /// Change the draft through `edit`, then settle it again.
    fn edit_giveaway(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut GiveawayDraft)) {
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
        }
        if let Some(form) = self.giveaway.form.as_mut() {
            form.draft = draft;
            form.note = None;
        }
        cx.notify();
    }

    fn toggle_giveaway_chat(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let mut refused = None;
        self.edit_giveaway(cx, |draft| {
            refused = draft.toggle_chat(chat_id).err();
        });
        if let (Some(error), Some(form)) = (refused, self.giveaway.form.as_mut()) {
            form.note = Some(error.text());
        }
    }

    /// The draft as the user sees it, with the prize text from the field.
    fn giveaway_draft_now(&self, cx: &App) -> Option<GiveawayDraft> {
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
        if let Some(form) = self.giveaway.form.as_mut() {
            match verdict {
                Ok(()) => form.confirming = true,
                Err(error) => form.note = Some(error.text()),
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
                .map(|_| "Couldn't send the request. Try again.".to_string()),
            None => Some("Starting a giveaway needs a signed-in account.".to_string()),
        };
        if let Some(form) = self.giveaway.form.as_mut() {
            form.confirming = false;
            form.note = note;
        }
        cx.notify();
    }

    fn giveaway_group(session: &Session, chat_id: i64) -> bool {
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
                .child(section_title("Prepaid giveaway"))
                .child(
                    div()
                        .id("giveaway-prepaid")
                        .text_sm()
                        .child(prepaid_summary(prepaid.prize, prepaid.winner_count)),
                )
                .child(about_line("You already paid for this giveaway."));
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
                            "Premium".into(),
                            draft.kind == GiveawayKind::Premium,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_giveaway(cx, |d| d.kind = GiveawayKind::Premium);
                        })),
                    )
                    .child(
                        choice(
                            "giveaway-kind-stars",
                            "Stars".into(),
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
            return section.child(about_line("Loading prices…"));
        }
        if draft.kind == GiveawayKind::Premium {
            if options.premium.is_empty() {
                let error = options.premium_error.clone().unwrap_or_else(|| {
                    "Premium giveaways are not available right now.".to_string()
                });
                return section.child(about_line(error));
            }
            section = section.child(section_title("Quantity of prizes"));
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
                .child(about_line(
                    "Choose how many Premium subscriptions to give away and boosts to receive.",
                ))
                .child(section_title("Duration of Premium subscriptions"));
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
                .unwrap_or_else(|| "Stars giveaways are not available right now.".to_string());
            return section.child(about_line(error));
        }
        section = section.child(section_title("Stars to distribute"));
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
            .child(section_title("Number of winners"));
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
        section.child(winners).child(about_line(
            "Choose how many winners you want to distribute stars among.",
        ))
    }

    /// The price line of a giveaway that is not paid for yet.
    fn giveaway_price(draft: &GiveawayDraft, options: &GiveawayOptions) -> Option<String> {
        if draft.prepaid.is_some() {
            return None;
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
            .unwrap_or_else(|| "this chat".into());
        let now = now_unix() as i32;
        let boosts = draft.boost_count(&options.stars);

        if giveaway.launch == GiveawayLaunch::Launched {
            return div()
                .id("giveaway-launched")
                .flex()
                .flex_col()
                .gap_1()
                .child(div().font_semibold().child("Giveaway created"))
                .child(about_line(format!(
                    "Check your {place}'s boosts to see how this giveaway boosted it."
                )))
                .into_any_element();
        }
        if form.confirming {
            return div()
                .id("giveaway-confirm")
                .text_sm()
                .child(
                    "Are you sure you want to start this prepaid giveaway now? This action cannot be undone.",
                )
                .into_any_element();
        }

        let mut body = div().flex().flex_col().gap_1();
        body = body.child(about_line(match (draft.kind, group) {
            (GiveawayKind::Stars, true) => {
                "Get more boosts and members for your group by giving away prizes."
            }
            (GiveawayKind::Stars, false) => {
                "Get more boosts and subscribers for your channel by giving away prizes."
            }
            (_, true) => "Get more boosts for your group by gifting Premium to your members.",
            (_, false) => {
                "Get more boosts for your channel by gifting Premium to your subscribers."
            }
        }));
        body = body.child(self.giveaway_prize_section(&draft, options, cx));

        // Channels.
        body = body
            .child(section_title("Channels included in the giveaway"))
            .child(div().text_sm().child(format!(
                "{chat_name}: this {place} will receive {}",
                boosts_text(boosts)
            )));
        let mut others: Vec<(i64, String)> = session
            .chats
            .iter()
            .filter(|(id, chat)| {
                **id != draft.boosted_chat_id
                    && matches!(chat.kind, ChatKind::Supergroup { .. })
                    && !chat.title.is_empty()
            })
            .map(|(id, chat)| (*id, chat.title.clone()))
            .collect();
        others.sort_by_key(|(_, title)| title.to_lowercase());
        for (id, title) in others.into_iter().take(30) {
            let ticked = draft.additional_chat_ids.contains(&id);
            body = body.child(
                Checkbox::new(("giveaway-chat", id.unsigned_abs()))
                    .checked(ticked)
                    .label(title)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_giveaway_chat(id, cx);
                    })),
            );
        }
        body = body.child(about_line(format!(
            "Choose the groups and channels users need to join to take part in the giveaway. {}/{ADDITIONAL_CHATS_MAX} added.",
            draft.additional_chat_ids.len()
        )));

        // Who may take part.
        let who = if group { "members" } else { "subscribers" };
        body = body
            .child(section_title("Users eligible for the giveaway"))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        choice(
                            "giveaway-eligible-all",
                            format!("All {who}"),
                            !draft.only_new_members,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_giveaway(cx, |d| d.only_new_members = false);
                        })),
                    )
                    .child(
                        choice(
                            "giveaway-eligible-new",
                            format!("Only new {who}"),
                            draft.only_new_members,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_giveaway(cx, |d| d.only_new_members = true);
                        })),
                    ),
            )
            .child(about_line(format!(
                "Choose if you want to limit the giveaway only to those who joined the {place} after the giveaway started."
            )));

        // Additional prize.
        body = body
            .child(section_title("Additional prizes"))
            .child(Input::new(&form.prize_input).aria_label("Additional prize"))
            .child(about_line(format!(
                "Turn this on if you want to give the winners your own prizes in addition to {}. Up to {PRIZE_DESCRIPTION_MAX} characters.",
                if draft.kind == GiveawayKind::Stars {
                    "Stars"
                } else {
                    "Premium subscriptions"
                }
            )));

        // End date.
        let days = draft.days_ahead(now);
        let max_days = PERIOD_MAX_SECS / 86_400;
        body = body
            .child(section_title("Date when giveaway ends"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("giveaway-days-less")
                            .label("−")
                            .small()
                            .ghost()
                            .disabled(days <= 1)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_giveaway(cx, |d| d.set_days_ahead(now, days - 1));
                            })),
                    )
                    .child(div().id("giveaway-date").text_sm().child(format!(
                        "{} · in {} day{}",
                        day_text(draft.selection_date),
                        days,
                        if days == 1 { "" } else { "s" }
                    )))
                    .child(
                        Button::new("giveaway-days-more")
                            .label("+")
                            .small()
                            .ghost()
                            .disabled(days >= max_days)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_giveaway(cx, |d| d.set_days_ahead(now, days + 1));
                            })),
                    ),
            )
            .child(about_line(format!(
                "Choose when winners of your {place} will be randomly selected. Up to {max_days} days ahead."
            )));

        // Show winners.
        body = body
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt_3()
                    .child(div().text_sm().child("Show winners"))
                    .child(
                        Switch::new("giveaway-show-winners")
                            .checked(draft.has_public_winners)
                            .accessibility_label("Show winners")
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                this.edit_giveaway(cx, |d| d.has_public_winners = on);
                            })),
                    ),
            )
            .child(about_line(
                "Choose whether to make the list of winners public when the giveaway ends.",
            ));

        // Price and notes.
        if let Some(price) = Self::giveaway_price(&draft, options) {
            body = body
                .child(section_title("Price"))
                .child(div().id("giveaway-price").text_sm().child(price))
                .child(about_line(
                    "Quill can't take payment for a new giveaway yet. Buy one in another Telegram app and it shows up here as a prepaid giveaway to start.",
                ));
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
                    .title(crate::ui::shell::dialog_title(TITLE))
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
            let footer = if launched {
                footer.child(close("Done"))
            } else if confirming {
                footer
                    .child(
                        Button::new("giveaway-confirm-no")
                            .label("Cancel")
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
                            .label("Start Giveaway")
                            .loading(launching)
                            .disabled(launching)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_prepaid_giveaway(cx);
                            })),
                    )
            } else {
                footer.child(close("Close")).child(
                    Button::new("giveaway-start")
                        .label("Start Giveaway")
                        .loading(launching)
                        // A new giveaway needs a payment Quill can't take.
                        .disabled(!prepaid || launching)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ask_to_start_giveaway(cx);
                        })),
                )
            };
            finish(dialog, TITLE, body, footer.into_any_element(), on_close)
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
    format!(
        "{} for {} Stars",
        format_payment_price(&option.currency, option.amount),
        group_digits(option.star_count)
    )
}

crate::ui::shell::register_dialogs! {
    /// "Boosts via Gifts": start a prepaid giveaway or see a new one's price.
    Giveaway => DialogSpec::new(
        2943,
        |app| app.giveaway.form.is_some(),
        QuillApp::build_giveaway_dialog,
    ),
}
