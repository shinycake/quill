//! Sign-in polish (tdesktop `intro_phone.cpp` / `intro_code.cpp` /
//! `country_select_box.cpp`): country picker with search, as-you-type
//! international formatting, resend countdown, "Wrong number?", the
//! banned-number box and flood countdowns. The pure logic lives in
//! `quill::phone` and `quill::signin`.

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::auth::AuthView;
use quill::phone::{self, Country};
use quill::signin;
use quill::state::AuthRequestError;
use quill::telegram::envelope::{AuthorizationState, CodeDelivery, EmailResetState};
use std::time::{Duration, Instant};

/// Per-screen UI state for the sign-in steps. Nothing here is a secret:
/// the phone is shown on screen anyway and the code is never kept.
pub(super) struct SignInUi {
    pub(super) search: Entity<InputState>,
    pub(super) picker_open: bool,
    /// The text last written to (or accepted from) the phone field.
    pub(super) phone_text: String,
    /// Inline hint under the phone field after a rejected attempt.
    pub(super) hint: Option<&'static str>,
    /// The number the code was requested for, shown on the code screen.
    pub(super) submitted_phone: String,
    /// "Wrong number?" re-opens the phone form over the code step.
    pub(super) editing_phone: bool,
    /// The user typed or picked a country: the IP-based default stays out.
    pub(super) touched: bool,
    pub(super) code_clock: Option<(CodeDelivery, Instant)>,
    pub(super) email_clock: Option<(EmailResetState, Instant)>,
    pub(super) error_clock: Option<(AuthRequestError, Instant)>,
    pub(super) banned_dismissed: bool,
    tick_armed: bool,
    /// Demo-only fixtures (no live TDLib to ask).
    pub(super) demo_countries: Vec<Country>,
    pub(super) demo_guess: Option<String>,
    pub(super) demo_error: Option<AuthRequestError>,
}

impl SignInUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        cx.subscribe(&search, |_this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        Self {
            search,
            picker_open: false,
            phone_text: String::new(),
            hint: None,
            submitted_phone: String::new(),
            editing_phone: false,
            touched: false,
            code_clock: None,
            email_clock: None,
            error_clock: None,
            banned_dismissed: false,
            tick_armed: false,
            demo_countries: Vec::new(),
            demo_guess: None,
            demo_error: None,
        }
    }
}

/// English fixture rows for the demo captures (no `getCountries` there).
pub(super) fn demo_country_fixtures() -> Vec<Country> {
    let row = |iso: &str, name: &str, flag: &str, code: &str| Country {
        iso: iso.into(),
        name: name.into(),
        english_name: name.into(),
        flag: flag.into(),
        calling_codes: vec![code.into()],
        hidden: false,
    };
    vec![
        row("AU", "Australia", "\u{1F1E6}\u{1F1FA}", "61"),
        row("BR", "Brazil", "\u{1F1E7}\u{1F1F7}", "55"),
        row("CA", "Canada", "\u{1F1E8}\u{1F1E6}", "1"),
        row("FR", "France", "\u{1F1EB}\u{1F1F7}", "33"),
        row("DE", "Germany", "\u{1F1E9}\u{1F1EA}", "49"),
        row("IN", "India", "\u{1F1EE}\u{1F1F3}", "91"),
        row("IL", "Israel", "\u{1F1EE}\u{1F1F1}", "972"),
        row("JP", "Japan", "\u{1F1EF}\u{1F1F5}", "81"),
        row("NL", "Netherlands", "\u{1F1F3}\u{1F1F1}", "31"),
        row("ES", "Spain", "\u{1F1EA}\u{1F1F8}", "34"),
        row("UA", "Ukraine", "\u{1F1FA}\u{1F1E6}", "380"),
        row("GB", "United Kingdom", "\u{1F1EC}\u{1F1E7}", "44"),
        row("US", "United States", "\u{1F1FA}\u{1F1F8}", "1"),
    ]
}

impl QuillApp {
    fn signin_countries(&self) -> Vec<Country> {
        match self.live.as_ref() {
            Some(live) => live.driver.session.countries.clone().unwrap_or_default(),
            None => self.signin.demo_countries.clone(),
        }
    }

    fn signin_guess_iso(&self) -> Option<String> {
        match self.live.as_ref() {
            Some(live) => live.driver.session.guessed_country_iso.clone(),
            None => self.signin.demo_guess.clone(),
        }
    }

    /// Per-frame bookkeeping for the sign-in steps: ask TDLib for the
    /// country list and default, keep the countdown clocks in step with the
    /// auth state and prefill the default country code.
    pub(super) fn sync_signin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let auth = self.current_auth();
        if let Some(live) = self.live.as_mut()
            && matches!(auth, AuthorizationState::WaitPhoneNumber)
        {
            let _ = live.driver.fetch_countries();
            let _ = live.driver.fetch_country_code();
        }
        match &auth {
            AuthorizationState::WaitCode { delivery, .. } => {
                if self.signin.code_clock.as_ref().map(|(d, _)| d) != Some(delivery) {
                    self.signin.code_clock = Some((*delivery, Instant::now()));
                }
            }
            _ => self.signin.code_clock = None,
        }
        match &auth {
            AuthorizationState::WaitEmailCode { reset, .. } => {
                if self.signin.email_clock.as_ref().map(|(r, _)| r) != Some(reset) {
                    self.signin.email_clock = Some((*reset, Instant::now()));
                }
            }
            _ => self.signin.email_clock = None,
        }
        if !matches!(auth, AuthorizationState::WaitCode { .. }) {
            self.signin.editing_phone = false;
        }
        let phone_screen = matches!(auth, AuthorizationState::WaitPhoneNumber)
            || (self.signin.editing_phone && matches!(auth, AuthorizationState::WaitCode { .. }));
        if phone_screen
            && !self.signin.touched
            && self.signin.phone_text.is_empty()
            && let Some(iso) = self.signin_guess_iso()
        {
            let countries = self.signin_countries();
            if let Some(code) = countries
                .iter()
                .find(|country| country.iso == iso)
                .and_then(|country| country.calling_codes.first())
            {
                let text = format!("+{code}");
                self.signin.phone_text = text.clone();
                cx.defer_in(window, move |this, window, cx| {
                    this.phone_input
                        .update(cx, |input, cx| input.set_value(text, window, cx));
                });
            }
        }
    }

    /// The phone field changed: re-group the digits, refresh the country.
    pub(super) fn signin_phone_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.phone_input.read(cx).value().to_string();
        if value == self.signin.phone_text {
            return;
        }
        let countries = self.signin_countries();
        let formatted = phone::reformat_after_edit(&self.signin.phone_text, &value, &countries);
        self.signin.touched = true;
        self.signin.hint = None;
        self.signin.phone_text = formatted.text.clone();
        if formatted.text != value {
            self.phone_input
                .update(cx, |input, cx| input.set_value(formatted.text, window, cx));
        }
        cx.notify();
    }

    fn signin_pick_country(&mut self, iso: &str, window: &mut Window, cx: &mut Context<Self>) {
        let countries = self.signin_countries();
        let Some(code) = countries
            .iter()
            .find(|country| country.iso == iso)
            .and_then(|country| country.calling_codes.first())
        else {
            return;
        };
        let text = format!("+{code}");
        self.signin.phone_text = text.clone();
        self.signin.touched = true;
        self.signin.hint = None;
        self.signin.picker_open = false;
        self.signin
            .search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.phone_input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn signin_toggle_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.signin.picker_open = !self.signin.picker_open;
        if self.signin.picker_open {
            self.signin
                .search
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// Validate what is in the phone field. `Ok` carries the E.164 number
    /// for TDLib and the grouped text for display.
    pub(super) fn signin_checked_phone(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, String)> {
        let countries = self.signin_countries();
        let value = self.phone_input.read(cx).value().to_string();
        let formatted = phone::format_international(&value, &countries);
        match phone::e164(&formatted) {
            Some(number) => {
                self.signin.hint = None;
                Some((number, formatted.text))
            }
            None => {
                self.signin.hint = phone::validate(&formatted)
                    .message()
                    .or(Some("Invalid phone number. Please try again."));
                cx.notify();
                None
            }
        }
    }

    /// "Wrong number?" on the code screen.
    fn signin_wrong_number(&mut self, cx: &mut Context<Self>) {
        self.signin.editing_phone = true;
        self.signin.hint = None;
        cx.notify();
    }

    fn signin_error(&mut self) -> Option<(AuthRequestError, i64)> {
        let err = self
            .session()
            .and_then(|session| session.last_auth_error)
            .or(self.signin.demo_error);
        let Some(err) = err else {
            self.signin.error_clock = None;
            self.signin.banned_dismissed = false;
            return None;
        };
        match &self.signin.error_clock {
            Some((seen, since)) if *seen == err => {
                Some((err, i64::try_from(since.elapsed().as_secs()).unwrap_or(0)))
            }
            _ => {
                self.signin.error_clock = Some((err, Instant::now()));
                self.signin.banned_dismissed = false;
                Some((err, 0))
            }
        }
    }

    /// Re-render once a second while any countdown on screen is running.
    fn arm_signin_tick(&mut self, cx: &mut Context<Self>) {
        if self.signin.tick_armed {
            return;
        }
        self.signin.tick_armed = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update(cx, |this, cx| {
                this.signin.tick_armed = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// Title/body overrides: delivery text on the code step, the phone
    /// prompt while correcting the number.
    pub(super) fn signin_view(&self, auth: &AuthView, state: &AuthorizationState) -> AuthView {
        let mut view = auth.clone();
        match state {
            AuthorizationState::WaitCode { delivery, .. } if self.signin.editing_phone => {
                let _ = delivery;
                view.title = "Your phone number";
                view.body =
                    "Please confirm your country code and enter your phone number.".to_string();
            }
            AuthorizationState::WaitCode {
                code_length,
                delivery,
            } => {
                let mut body = String::new();
                if !self.signin.submitted_phone.is_empty() {
                    body.push_str(&format!("Code for {}. ", self.signin.submitted_phone));
                }
                body.push_str(signin::delivery_description(delivery.kind));
                if let Some(len) = code_length {
                    body.push_str(&format!(" It has {len} digits."));
                }
                view.body = body;
            }
            AuthorizationState::WaitPhoneNumber => {
                view.body =
                    "Please confirm your country code and enter your phone number.".to_string();
            }
            _ => {}
        }
        view
    }

    pub(super) fn signin_phone_visible(&self, state: &AuthorizationState) -> bool {
        matches!(state, AuthorizationState::WaitPhoneNumber)
            || (self.signin.editing_phone && matches!(state, AuthorizationState::WaitCode { .. }))
    }

    /// Error line, banned box and flood lock for the current screen.
    /// Returns the elements to append and whether submits are locked.
    pub(super) fn signin_error_elements(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, bool) {
        let mut out = Vec::new();
        let Some((err, elapsed)) = self.signin_error() else {
            return (out, false);
        };
        let locked = signin::flood_remaining(&err, elapsed).is_some_and(|left| left > 0);
        if locked {
            self.arm_signin_tick(cx);
        }
        if err.class == quill::telegram::envelope::ErrorClass::PhoneBanned {
            if !self.signin.banned_dismissed {
                out.push(self.banned_box(cx));
            }
            return (out, locked);
        }
        out.push(
            div()
                .id("auth-error")
                .role(Role::Alert)
                .w_full()
                .text_sm()
                .text_center()
                .text_color(cx.theme().danger)
                .child(signin::auth_error_line(&err, elapsed))
                .into_any_element(),
        );
        (out, locked)
    }

    /// tdesktop's `ShowPhoneBannedError` box: the text, OK, and Help which
    /// opens a prefilled mail to Telegram's login support.
    fn banned_box(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let phone = self.signin.submitted_phone.clone();
        div()
            .id("phone-banned-box")
            .role(Role::AlertDialog)
            .aria_label("This phone number is banned.")
            .w_full()
            .p_4()
            .gap_3()
            .flex()
            .flex_col()
            .items_center()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().danger)
            .bg(cx.theme().secondary)
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("This phone number is banned."),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("banned-help")
                            .label("Help")
                            .ghost()
                            .on_click(cx.listener(move |_, _, _, cx| {
                                let url = signin::banned_help_mailto(
                                    &phone,
                                    quill::version::APP,
                                    std::env::consts::OS,
                                );
                                cx.open_url(&url);
                            })),
                    )
                    .child(
                        Button::new("banned-ok")
                            .label("OK")
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.signin.banned_dismissed = true;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Country button, optional picker and the formatted phone field.
    pub(super) fn signin_phone_section(
        &mut self,
        busy: bool,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let countries = self.signin_countries();
        let value = self.phone_input.read(cx).value().to_string();
        let formatted = phone::format_international(&value, &countries);
        let selected = phone::country_for_digits(&formatted.digits, &countries)
            .cloned()
            .or_else(|| {
                if formatted.digits.is_empty() {
                    self.signin_guess_iso()
                        .and_then(|iso| countries.iter().find(|c| c.iso == iso).cloned())
                } else {
                    None
                }
            });
        let label = match &selected {
            Some(country) => format!(
                "{}  {}  {}",
                country.flag,
                country.name,
                country.display_code()
            ),
            None => "Select country".to_string(),
        };
        let mut out: Vec<AnyElement> = Vec::new();
        out.push(
            Button::new("country-select")
                .label(label)
                .ghost()
                .w_full()
                .on_click(cx.listener(|this, _, window, cx| this.signin_toggle_picker(window, cx)))
                .into_any_element(),
        );
        if self.signin.picker_open {
            out.push(self.country_picker(&countries, cx));
        }
        out.push(
            Textarea::new(&self.phone_input)
                .aria_label("Phone number")
                .h(px(40.))
                .into_any_element(),
        );
        if let Some(hint) = self.signin.hint {
            out.push(
                div()
                    .id("phone-hint")
                    .role(Role::Alert)
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(hint)
                    .into_any_element(),
            );
        }
        out.push(
            Button::new("submit-phone")
                .label("Continue")
                .primary()
                .w_full()
                .loading(busy)
                .disabled(busy || locked)
                .on_click(cx.listener(|this, _, window, cx| this.submit_phone(window, cx)))
                .into_any_element(),
        );
        if self.signin.editing_phone {
            out.push(
                Button::new("phone-edit-back")
                    .label("Back")
                    .ghost()
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.signin.editing_phone = false;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        out
    }

    fn country_picker(&mut self, countries: &[Country], cx: &mut Context<Self>) -> AnyElement {
        let query = self.signin.search.read(cx).value().to_string();
        let hits = phone::search_countries(countries, &query);
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().secondary;
        let mut list = div()
            .id("country-list")
            .w_full()
            .max_h(px(240.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if hits.is_empty() {
            list = list.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(muted)
                    .child("Country not found"),
            );
        }
        for (index, country) in hits.iter().enumerate() {
            let iso = country.iso.clone();
            list = list.child(
                div()
                    .id(("country-row", index))
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
                        this.signin_pick_country(&iso, window, cx);
                    }))
                    .child(div().w(px(24.)).child(country.flag.clone()))
                    .child(div().flex_1().text_sm().child(country.name.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(country.display_code()),
                    ),
            );
        }
        div()
            .id("country-picker")
            .w_full()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .child(Input::new(&self.signin.search).aria_label("Search countries"))
            .child(list)
            .into_any_element()
    }

    /// Code entry, resend countdown, "Wrong number?" and the email reset.
    pub(super) fn signin_code_section(
        &mut self,
        busy: bool,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = vec![
            Textarea::new(&self.code_input)
                .aria_label("Sign-in code")
                .h(px(40.))
                .into_any_element(),
            Button::new("submit-code")
                .label("Continue")
                .primary()
                .w_full()
                .loading(busy)
                .disabled(busy || locked)
                .on_click(cx.listener(|this, _, window, cx| this.submit_code(window, cx)))
                .into_any_element(),
        ];
        let auth = self.current_auth();
        let mut ticking = false;
        match &auth {
            AuthorizationState::WaitCode { delivery, .. } => {
                let elapsed = self.signin.code_clock.as_ref().map_or(0, |(_, since)| {
                    i64::try_from(since.elapsed().as_secs()).unwrap_or(0)
                });
                if let Some(option) = signin::resend_option(delivery, elapsed) {
                    ticking |= !option.ready;
                    out.push(
                        Button::new("resend-code")
                            .label(option.label)
                            .ghost()
                            .w_full()
                            .disabled(!option.ready || busy)
                            .on_click(cx.listener(|this, _, _, cx| this.resend_code(cx)))
                            .into_any_element(),
                    );
                }
                out.push(
                    Button::new("wrong-number")
                        .label("Wrong number?")
                        .ghost()
                        .w_full()
                        .on_click(cx.listener(|this, _, _, cx| this.signin_wrong_number(cx)))
                        .into_any_element(),
                );
            }
            AuthorizationState::WaitEmailCode { reset, .. } => {
                out.push(
                    Button::new("resend-code")
                        .label("Send the code again")
                        .ghost()
                        .w_full()
                        .on_click(cx.listener(|this, _, _, cx| this.resend_code(cx)))
                        .into_any_element(),
                );
                let elapsed = self.signin.email_clock.as_ref().map_or(0, |(_, since)| {
                    i64::try_from(since.elapsed().as_secs()).unwrap_or(0)
                });
                if let Some(label) = signin::email_reset_label(*reset, elapsed) {
                    match reset {
                        EmailResetState::Available { .. } => out.push(
                            Button::new("reset-login-email")
                                .label(label)
                                .ghost()
                                .w_full()
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| this.reset_login_email(cx)))
                                .into_any_element(),
                        ),
                        _ => {
                            ticking = true;
                            out.push(
                                div()
                                    .id("email-reset-pending")
                                    .text_sm()
                                    .text_center()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label)
                                    .into_any_element(),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        if ticking {
            self.arm_signin_tick(cx);
        }
        out
    }

    /// `resetAuthenticationEmailAddress` from the email-code step.
    fn reset_login_email(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        self.status_note = match live.driver.reset_login_email() {
            Ok(_) => "email reset requested — waiting for Telegram".into(),
            Err(_) => "could not reset the email".into(),
        };
        cx.notify();
    }
}
