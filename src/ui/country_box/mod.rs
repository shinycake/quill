//! The country box (Telegram Desktop `Ui::CountrySelectBox` for one country
//! and the giveaway's `Ui::SelectCountriesBox` for a limited list): a
//! search field over flag + name rows, built from the `getCountries` list
//! the sign-in screen already uses. The shipping address picks one
//! country; the poll's "Allowed Countries" picks up to the TDLib limit.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::country_select::{
    CountryClick, CountryMode, CountrySelection, POLL_COUNTRIES_FALLBACK, country_name, limit_text,
};
use quill::lang::{keys, tr};
use quill::phone::{self, Country};

/// Who opened the box, and so what a click, Save or Cancel does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CountryHost {
    /// The checkout's shipping country (one).
    Payment,
    /// The card form's billing country (one).
    PaymentCard,
    /// The poll's allowed countries (several).
    Poll,
}

impl CountryHost {
    fn id(self) -> &'static str {
        match self {
            Self::Payment => "payment",
            Self::PaymentCard => "payment-card",
            Self::Poll => "poll",
        }
    }
}

/// An open country box: what is chosen so far and the search field.
pub(crate) struct CountryBox {
    selection: CountrySelection,
    search: Entity<InputState>,
}

impl CountryBox {
    pub(crate) fn new(
        mode: CountryMode,
        chosen: &[String],
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_OPS_SEARCH)));
        cx.subscribe(&search, |_this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        Self {
            selection: CountrySelection::new(mode, chosen),
            search,
        }
    }
}

impl QuillApp {
    /// The `getCountries` rows: the live list, or the demo fixtures.
    fn country_box_countries(&self) -> Vec<Country> {
        match self.live.as_ref() {
            Some(live) => live
                .driver
                .session
                .auth_state
                .countries
                .clone()
                .unwrap_or_default(),
            None => crate::ui::signin_ui::demo_country_fixtures(),
        }
    }

    /// `poll_country_count_max`, until TDLib reports it tdesktop's fallback.
    fn poll_countries_max(&self) -> usize {
        self.session()
            .and_then(|session| session.payments.poll_countries_max)
            .unwrap_or(POLL_COUNTRIES_FALLBACK)
    }

    fn country_box_slot(&mut self, host: CountryHost) -> Option<&mut Option<CountryBox>> {
        match host {
            CountryHost::Payment => self.payments.dialog.as_mut().map(|d| &mut d.country_box),
            CountryHost::PaymentCard => self
                .payments
                .dialog
                .as_mut()
                .and_then(|d| d.card_form.as_mut())
                .map(|form| &mut form.country_box),
            CountryHost::Poll => self
                .composer_ui
                .poll_dialog
                .as_mut()
                .map(|d| &mut d.country_box),
        }
    }

    /// Open the box for `host` on its current choice, asking TDLib for the
    /// list when sign-in never did.
    pub(super) fn open_country_box(
        &mut self,
        host: CountryHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (mode, chosen) = match host {
            CountryHost::PaymentCard => {
                let Some(country_box) = self.card_country_box(window, cx) else {
                    return;
                };
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.fetch_countries();
                }
                if let Some(slot) = self.country_box_slot(host) {
                    *slot = Some(country_box);
                }
                cx.notify();
                return;
            }
            CountryHost::Payment => {
                let iso = self
                    .payments
                    .dialog
                    .as_ref()
                    .map(|d| d.country_iso.clone())
                    .filter(|iso| !iso.is_empty());
                (CountryMode::Single, iso.into_iter().collect::<Vec<_>>())
            }
            CountryHost::Poll => {
                let max = self.poll_countries_max();
                let chosen = self
                    .composer_ui
                    .poll_dialog
                    .as_ref()
                    .map(|d| d.country_codes.clone())
                    .unwrap_or_default();
                (CountryMode::Multi { max }, chosen)
            }
        };
        let country_box = CountryBox::new(mode, &chosen, window, cx);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_countries();
        }
        if let Some(slot) = self.country_box_slot(host) {
            *slot = Some(country_box);
        }
        cx.notify();
    }

    /// A country box is up (Esc closes it before its host).
    pub(super) fn country_box_open(&self) -> bool {
        self.payments.dialog.as_ref().is_some_and(|dialog| {
            dialog.country_box.is_some()
                || dialog
                    .card_form
                    .as_ref()
                    .is_some_and(|form| form.country_box.is_some())
        }) || self
            .composer_ui
            .poll_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.country_box.is_some())
    }

    pub(super) fn close_open_country_boxes(&mut self, cx: &mut Context<Self>) {
        self.close_country_box(CountryHost::Poll, cx);
        self.close_country_box(CountryHost::PaymentCard, cx);
        self.close_country_box(CountryHost::Payment, cx);
    }

    pub(super) fn close_country_box(&mut self, host: CountryHost, cx: &mut Context<Self>) {
        if let Some(slot) = self.country_box_slot(host) {
            *slot = None;
        }
        cx.notify();
    }

    fn country_box_click(
        &mut self,
        host: CountryHost,
        iso: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(country_box) = self.country_box_slot(host).and_then(Option::as_mut) else {
            return;
        };
        match country_box.selection.click(iso) {
            CountryClick::Picked(iso) => {
                self.close_country_box(host, cx);
                match host {
                    CountryHost::Payment => {
                        if let Some(dialog) = self.payments.dialog.as_mut() {
                            dialog.country_iso = iso;
                        }
                    }
                    CountryHost::PaymentCard => self.card_country_picked(iso, window, cx),
                    CountryHost::Poll => {}
                }
            }
            CountryClick::LimitReached(max) => {
                self.set_status_note(StatusNote::info(limit_text(max)), cx);
            }
            CountryClick::Added | CountryClick::Removed => cx.notify(),
        }
    }

    fn country_box_remove(&mut self, host: CountryHost, iso: &str, cx: &mut Context<Self>) {
        if let Some(country_box) = self.country_box_slot(host).and_then(Option::as_mut) {
            country_box.selection.remove(iso);
        }
        cx.notify();
    }

    fn country_box_save(&mut self, host: CountryHost, cx: &mut Context<Self>) {
        let chosen = self
            .country_box_slot(host)
            .and_then(|slot| slot.as_ref())
            .map(|country_box| country_box.selection.chosen().to_vec());
        if let (Some(chosen), CountryHost::Poll) = (chosen, host)
            && let Some(dialog) = self.composer_ui.poll_dialog.as_mut()
        {
            dialog.country_codes = chosen;
        }
        self.close_country_box(host, cx);
    }

    /// The field row that opens the box (tdesktop's read-only country
    /// field): the name, or the muted label while nothing is chosen, with a
    /// chevron. A kit `Button` keeps it focusable and Enter-activated.
    pub(super) fn country_field_button(
        &self,
        host: CountryHost,
        label: &'static str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let empty = value.is_empty();
        let text = if empty { label.to_string() } else { value };
        Button::new(SharedString::from(format!("country-field-{}", host.id())))
            .outline()
            .w_full()
            .h(px(36.))
            .px_3()
            .justify_start()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_left()
                            .when(empty, |this| this.text_color(muted))
                            .child(text),
                    )
                    .child(Icon::new(IconName::ChevronDown).small().text_color(muted)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_country_box(host, window, cx);
            }))
            .into_any_element()
    }

    /// The name shown in the field for an ISO code.
    pub(super) fn country_display_name(&self, iso: &str) -> String {
        if iso.is_empty() {
            return String::new();
        }
        country_name(iso, &self.country_box_countries())
    }

    /// "United States" or "3 countries" for the field.
    pub(super) fn country_summary(&self, codes: &[String]) -> String {
        quill::country_select::summary(codes, &self.country_box_countries())
    }

    /// The poll's panel while its country box is open.
    pub(super) fn poll_country_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = self.country_box_view(
            CountryHost::Poll,
            tr(keys::POLLS_CREATE_ALLOWED_COUNTRIES),
            cx,
        )?;
        Some(
            div()
                .id("poll-dialog")
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().primary)
                .bg(cx.theme().background)
                .child(view)
                .into_any_element(),
        )
    }

    /// The box itself: search, chips (several), rows, and the buttons.
    pub(super) fn country_box_view(
        &self,
        host: CountryHost,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let country_box = match host {
            CountryHost::Payment => self.payments.dialog.as_ref()?.country_box.as_ref()?,
            CountryHost::PaymentCard => self.card_form()?.country_box.as_ref()?,
            CountryHost::Poll => self
                .composer_ui
                .poll_dialog
                .as_ref()?
                .country_box
                .as_ref()?,
        };
        let countries = self.country_box_countries();
        let query = country_box.search.read(cx).value().to_string();
        let hits = phone::search_countries(&countries, &query);
        let multi = matches!(country_box.selection.mode(), CountryMode::Multi { .. });
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().secondary;
        let id = host.id();

        let mut list = div()
            .id(SharedString::from(format!("country-list-{id}")))
            .w_full()
            .max_h(px(280.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if hits.is_empty() {
            list = list.child(div().p_3().text_sm().text_color(muted).child(
                if countries.is_empty() {
                    tr(keys::Q_MONEY_LOADING_COUNTRIES)
                } else {
                    tr(keys::Q_OPS_COUNTRY_NOT_FOUND)
                },
            ));
        }
        for country in hits {
            let iso = country.iso.clone();
            let chosen = country_box.selection.contains(&iso);
            let label = format!("{}  {}", country.flag, country.name);
            let row_id = SharedString::from(format!("country-row-{id}-{iso}"));
            if multi {
                list = list.child(
                    div().px_3().py_1().child(
                        Checkbox::new(row_id)
                            .checked(chosen)
                            .label(label.trim().to_string())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.country_box_click(host, &iso, window, cx);
                            })),
                    ),
                );
            } else {
                list = list.child(
                    div()
                        .id(row_id)
                        .w_full()
                        .px_3()
                        .py_2()
                        .flex()
                        .items_center()
                        .gap_3()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.country_box_click(host, &iso, window, cx);
                        }))
                        .child(div().w(px(24.)).child(country.flag.clone()))
                        .child(div().flex_1().text_sm().child(country.name.clone()))
                        .when(chosen, |row| row.child(Icon::new(IconName::Check).small())),
                );
            }
        }

        let mut view = div()
            .id(SharedString::from(format!("country-box-{id}")))
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().font_semibold().child(title))
            .child(Input::new(&country_box.search).aria_label(tr(keys::Q_OPS_SEARCH_COUNTRIES)));
        if multi && !country_box.selection.chosen().is_empty() {
            let mut chips = div().flex().flex_wrap().gap_1();
            for iso in country_box.selection.chosen() {
                let code = iso.clone();
                let name = country_name(iso, &countries);
                chips = chips.child(
                    div()
                        .id(SharedString::from(format!("country-chip-{id}-{iso}")))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.country_box_remove(host, &code, cx);
                        }))
                        .child(
                            Tag::secondary()
                                .gap_1()
                                .child(name)
                                .child(Icon::new(IconName::Close).xsmall()),
                        ),
                );
            }
            view = view.child(chips);
        }
        view = view.child(list);
        let mut buttons = div().flex().justify_end().gap_2().child(
            Button::new(SharedString::from(format!("country-cancel-{id}")))
                .label(tr(keys::CANCEL))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| this.close_country_box(host, cx))),
        );
        if multi {
            buttons = buttons.child(
                Button::new(SharedString::from(format!("country-save-{id}")))
                    .label(tr(keys::SETTINGS_SAVE))
                    .primary()
                    .on_click(cx.listener(move |this, _, _, cx| this.country_box_save(host, cx))),
            );
        }
        Some(view.child(buttons).into_any_element())
    }
}

mod demo;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;
