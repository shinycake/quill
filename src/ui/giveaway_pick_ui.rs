//! The pieces of the giveaway form that pick from a list: members to award
//! with a Premium gift code and the countries that may take part (tdesktop
//! `create_giveaway_box.cpp`: "Award specific users", `SelectCountriesBox`).
//! Both open as a page inside the form's dialog and go back with Done.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::giveaway_create_ui::{about_line, boosts_text, choice, section_title};
use super::giveaway_info_ui::day_text;
use super::giveaway_state::{GiveawayForm, GiveawayPage};
use gpui_base::Disableable;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::giveaway::{
    GiveawayDraft, GiveawayKind, GiveawayLimits, PERIOD_MAX_SECS, PRIZE_DESCRIPTION_MAX,
    gift_code_months, gift_code_price,
};
use quill::ids::ChatId;
use quill::local_time::now_unix;
use quill::state::{GiveawayOptions, Session};
use quill::telegram::envelope::{ChatKind, format_payment_price};

/// Rows drawn at once; the search narrows the rest.
const ROWS_MAX: usize = 150;

impl QuillApp {
    /// Everything of the form that only a random giveaway has: the extra
    /// channels, who may take part, the extra prize, the end date and
    /// whether winners are shown.
    pub(super) fn giveaway_random_sections(
        &self,
        mut body: Div,
        form: &GiveawayForm,
        draft: &GiveawayDraft,
        session: &Session,
        cx: &mut Context<Self>,
    ) -> Div {
        let options = &session.payments.giveaway.options;
        let group = Self::giveaway_group(session, draft.boosted_chat_id);
        let place = if group { "group" } else { "channel" };
        let chat_name = session
            .chats
            .get(&draft.boosted_chat_id)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "this chat".into());
        let now = now_unix() as i32;
        let boosts = draft.boost_count(&options.stars);

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
            "Choose the groups and channels users need to join to take part in the giveaway. {}/{} added.",
            draft.additional_chat_ids.len(),
            draft.limits.chats_max
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
        body = self.giveaway_countries_row(body, draft, session, cx);

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
        body
    }
}

impl QuillApp {
    /// "Who may take part": the countries row of the random giveaway.
    fn giveaway_countries_row(
        &self,
        body: Div,
        draft: &GiveawayDraft,
        session: &Session,
        cx: &mut Context<Self>,
    ) -> Div {
        let summary = match draft.country_codes.as_slice() {
            [] => "From all countries".to_string(),
            codes => {
                let names: Vec<String> = codes
                    .iter()
                    .map(|code| country_name(session, code))
                    .collect();
                format!("From {}", names.join(", "))
            }
        };
        body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .pt_2()
                .child(div().id("giveaway-countries").text_sm().child(summary))
                .child(
                    Button::new("giveaway-countries-pick")
                        .label("Choose countries\u{2026}")
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_giveaway_page(GiveawayPage::Countries, window, cx);
                        })),
                ),
        )
        .child(about_line(format!(
            "Choose up to {} countries. Users with a number from Fragment can take part in any giveaway.",
            draft.limits.countries_max
        )))
    }

    /// "Award specific users": a Premium gift code for each chosen member.
    pub(super) fn giveaway_award_row(
        &self,
        body: Div,
        draft: &GiveawayDraft,
        options: &GiveawayOptions,
        cx: &mut Context<Self>,
    ) -> Div {
        if draft.prepaid.is_some()
            || draft.kind != GiveawayKind::Premium
            || options.premium.is_empty()
        {
            return body;
        }
        let count = draft.award_user_ids.len();
        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .pt_2()
            .child(div().id("giveaway-award").text_sm().child(match count {
                0 => "Random winners".to_string(),
                1 => "1 member gets a gift code".to_string(),
                n => format!("{n} members get a gift code"),
            }))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .when(count > 0, |row| {
                        row.child(
                            Button::new("giveaway-award-clear")
                                .label("Pick randomly")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.edit_giveaway(cx, |d| d.award_user_ids.clear());
                                })),
                        )
                    })
                    .child(
                        Button::new("giveaway-award-pick")
                            .label("Choose members\u{2026}")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_giveaway_page(GiveawayPage::Users, window, cx);
                            })),
                    ),
            );
        body.child(row).child(about_line(
            "Give a Premium gift code to chosen members instead of drawing winners.",
        ))
    }

    /// The duration choices of a gift-code giveaway: one price per member.
    pub(super) fn giveaway_gift_code_months(
        &self,
        section: Div,
        draft: &GiveawayDraft,
        options: &GiveawayOptions,
        cx: &mut Context<Self>,
    ) -> Div {
        let users = draft.award_user_ids.len();
        let mut months = div().flex().flex_col().gap_1();
        for month_count in gift_code_months(&options.premium) {
            let price = gift_code_price(&options.premium, month_count, users)
                .map(|(currency, amount)| format_payment_price(&currency, amount))
                .unwrap_or_default();
            months = months.child(
                choice(
                    ("giveaway-months", month_count as u64),
                    format!(
                        "{} \u{b7} {price}",
                        quill::deep_link_types::gift_duration(i64::from(month_count), 0)
                    ),
                    month_count == draft.premium_months,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit_giveaway(cx, |d| d.premium_months = month_count);
                })),
            );
        }
        section
            .child(section_title("Duration of Premium subscriptions"))
            .child(months)
    }

    /// The price line of a gift-code giveaway.
    pub(super) fn gift_code_price_line(
        draft: &GiveawayDraft,
        options: &GiveawayOptions,
    ) -> Option<String> {
        let users = draft.award_user_ids.len();
        let (currency, amount) = gift_code_price(&options.premium, draft.premium_months, users)?;
        Some(format!(
            "{} for {users} Premium {}",
            format_payment_price(&currency, amount),
            if users == 1 {
                "subscription"
            } else {
                "subscriptions"
            }
        ))
    }

    /// Open a list page of the form and start loading what it lists.
    pub(super) fn open_giveaway_page(
        &mut self,
        page: GiveawayPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.payments
            .pick_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        let chat = self.giveaway.form.as_ref().map(|f| f.draft.boosted_chat_id);
        if let (Some(live), Some(chat)) = (self.live.as_mut(), chat) {
            match page {
                GiveawayPage::Users => {
                    let _ = live.driver.search_award_members(ChatId(chat), "");
                }
                GiveawayPage::Countries => {
                    let _ = live.driver.fetch_countries();
                }
                GiveawayPage::Form => {}
            }
        }
        if let Some(form) = self.giveaway.form.as_mut() {
            form.page = page;
            form.note = None;
        }
        cx.notify();
    }

    pub(super) fn close_giveaway_page(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.giveaway.form.as_mut() {
            form.page = GiveawayPage::Form;
        }
        cx.notify();
    }

    /// The giveaway limits of this account.
    pub(super) fn giveaway_limits(&self) -> GiveawayLimits {
        self.session()
            .map(|s| s.payments.giveaway.limits)
            .unwrap_or_default()
    }

    /// The next page of the members list.
    fn load_more_award(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_award_members();
        }
        cx.notify();
    }

    /// The search field changed: on the members page, search on the server.
    pub(super) fn pick_search_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.payments.pick_search.read(cx).value().to_string();
        let on_users = self
            .giveaway
            .form
            .as_ref()
            .filter(|f| f.page == GiveawayPage::Users)
            .map(|f| f.draft.boosted_chat_id);
        if let (Some(chat), Some(live)) = (on_users, self.live.as_mut()) {
            let _ = live.driver.search_award_members(ChatId(chat), &query);
        }
        cx.notify();
    }

    fn toggle_award_member(&mut self, user_id: i64, cx: &mut Context<Self>) {
        let limits = self.giveaway_limits();
        let mut refused = None;
        self.edit_giveaway(cx, |draft| {
            refused = draft.toggle_award_user(user_id).err();
        });
        if let (Some(error), Some(form)) = (refused, self.giveaway.form.as_mut()) {
            form.note = Some(error.text_for(&limits));
        }
    }

    fn toggle_giveaway_country(&mut self, code: String, cx: &mut Context<Self>) {
        let limits = self.giveaway_limits();
        let mut refused = None;
        self.edit_giveaway(cx, |draft| {
            refused = draft.toggle_country(&code).err();
        });
        if let (Some(error), Some(form)) = (refused, self.giveaway.form.as_mut()) {
            form.note = Some(error.text_for(&limits));
        }
    }

    /// The members or countries page.
    pub(super) fn giveaway_pick_page(
        &self,
        form: &GiveawayForm,
        draft: &GiveawayDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let query = self
            .payments
            .pick_search
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let mut body = div().flex().flex_col().gap_1();
        body = body.child(Input::new(&self.payments.pick_search).aria_label("Search"));
        match form.page {
            GiveawayPage::Users => {
                let list = &session.payments.giveaway.award_members;
                body = body.child(about_line(format!(
                    "Choose up to {} members. {} chosen.",
                    draft.limits.chats_max,
                    draft.award_user_ids.len()
                )));
                let chosen_first = draft
                    .award_user_ids
                    .iter()
                    .filter(|id| !list.user_ids.contains(id));
                let people = chosen_first.chain(list.user_ids.iter()).map(|id| {
                    let name = session
                        .users
                        .get(id)
                        .map(|u| u.display_name())
                        .unwrap_or_else(|| format!("User {id}"));
                    (*id, name)
                });
                // The chosen members stay on top when a search hides them.
                let mut shown = 0;
                for (id, name) in people {
                    shown += 1;
                    body = body.child(
                        Checkbox::new(("giveaway-award-user", id.unsigned_abs()))
                            .checked(draft.award_user_ids.contains(&id))
                            .label(name)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_award_member(id, cx);
                            })),
                    );
                }
                if let Some(error) = list.error.as_ref() {
                    body = body.child(div().text_sm().child(error.clone()));
                } else if list.loading {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Loading members\u{2026}"),
                    );
                } else if shown == 0 {
                    body = body.child(div().text_sm().text_color(muted).child("No members found."));
                }
                if list.can_load_more() || list.error.is_some() {
                    body = body.child(
                        Button::new("giveaway-award-more")
                            .label("Load more")
                            .ghost()
                            .small()
                            .disabled(list.loading)
                            .on_click(cx.listener(|this, _, _, cx| this.load_more_award(cx))),
                    );
                }
            }
            GiveawayPage::Countries => {
                body = body.child(about_line(format!(
                    "Choose up to {} countries. {} chosen.",
                    draft.limits.countries_max,
                    draft.country_codes.len()
                )));
                match session.auth_state.countries.as_ref() {
                    Some(countries) => {
                        let mut list: Vec<&quill::phone::Country> = countries
                            .iter()
                            .filter(|c| !c.hidden)
                            .filter(|c| {
                                query.is_empty()
                                    || c.name.to_lowercase().contains(&query)
                                    || c.english_name.to_lowercase().contains(&query)
                            })
                            .collect();
                        list.sort_by_key(|c| c.name.to_lowercase());
                        for country in list.into_iter().take(ROWS_MAX) {
                            let code = country.iso.clone();
                            let label = format!("{} {}", country.flag, country.name);
                            body =
                                body.child(
                                    Checkbox::new(SharedString::from(format!(
                                        "giveaway-country-{}",
                                        country.iso
                                    )))
                                    .checked(draft.country_codes.contains(&country.iso))
                                    .label(label.trim().to_string())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_giveaway_country(code.clone(), cx);
                                    })),
                                );
                        }
                    }
                    None => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child("Loading countries\u{2026}"),
                        );
                    }
                }
            }
            GiveawayPage::Form => {}
        }
        if let Some(note) = form.note.as_ref() {
            body = body.child(
                div()
                    .id("giveaway-note")
                    .pt_2()
                    .text_sm()
                    .child(note.clone()),
            );
        }
        if form.page == GiveawayPage::Users {
            // Scrolling the list asks for the next page as well.
            return body
                .id("giveaway-award-list")
                .on_scroll_wheel(cx.listener(|this, _, _, cx| this.load_more_award(cx)))
                .into_any_element();
        }
        body.into_any_element()
    }
}

/// The name of a country by its code, falling back to the code.
fn country_name(session: &Session, code: &str) -> String {
    session
        .auth_state
        .countries
        .as_ref()
        .and_then(|list| list.iter().find(|c| c.iso == code))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| code.to_string())
}
