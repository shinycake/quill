//! Login passkeys of the signed-in account, after Telegram Desktop's
//! `data/components/passkeys.*` and `settings/sections/settings_passkeys.cpp`.
//!
//! Pure core: the TDLib types (`passkey`, `passkeys`), the requests
//! (`getLoginPasskeys`, `removeLoginPasskey`), the cached list and the
//! Privacy and Security row logic. The driver is `connect::passkeys`, the
//! reducer `state::domains::settings::passkeys`, the screens
//! `ui::security::passkeys`.
//!
//! Creating a passkey (`getPasskeyParameters`, the ceremony in
//! `crate::webauthn`, `addLoginPasskey`) needs a WebAuthn client:
//! [`webauthn_supported`] says whether this build has one.
#![deny(clippy::indexing_slicing)]

pub mod login;
#[cfg(test)]
mod tests;

use crate::ids::RequestId;
use crate::local_time::{CivilTime, day_label, hhmm};
use crate::webauthn::{CreationOptions, Registration};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a fetched list counts as fresh before the next look asks
/// again (tdesktop `kRequestTimeout`).
pub const LIST_FRESH_FOR: Duration = Duration::from_secs(5);

/// The name a passkey without one shows (`lng_settings_passkey_unknown`).
pub const UNNAMED: &str = "Passkey";

/// The most passkeys an account may have when the app config does not say
/// (tdesktop `passkeysAccountPasskeysMax`, default 10).
pub const DEFAULT_MAX_PASSKEYS: usize = 10;

/// Whether this build can create passkeys and sign in with them (tdesktop
/// `Platform::WebAuthn::IsSupported`): a USB security key over CTAP2 on
/// macOS and Linux, `webauthn.dll` on Windows (`crate::webauthn`). It
/// shows the Passkeys row on accounts without passkeys and enables the
/// "Create Passkey" button.
pub fn webauthn_supported() -> bool {
    crate::webauthn::supported()
}

/// One passkey (`passkey`, TDLib 1.8.68 `td_api.tl:189`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passkey {
    pub id: String,
    pub name: String,
    /// Unix time the passkey was added.
    pub addition_date: i64,
    /// Unix time it was last used; 0 if never.
    pub last_usage_date: i64,
    /// Custom emoji of the app that made it; 0 if unknown.
    pub software_icon_custom_emoji_id: i64,
}

fn int(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

impl Passkey {
    /// `None` without an id: such a passkey could not be removed.
    pub fn parse(value: &Value) -> Option<Self> {
        let id = value.get("id").and_then(Value::as_str)?.to_string();
        if id.is_empty() {
            return None;
        }
        Some(Self {
            id,
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            addition_date: int(value.get("addition_date")),
            last_usage_date: int(value.get("last_usage_date")),
            software_icon_custom_emoji_id: int(value.get("software_icon_custom_emoji_id")),
        })
    }

    /// The row title: the name, or "Passkey" when it has none.
    pub fn title(&self) -> &str {
        if self.name.is_empty() {
            UNNAMED
        } else {
            &self.name
        }
    }

    /// The row's second line: "Last used {date}" once used, else
    /// "Created {date}", with `{date}` as "{day} at {time}"
    /// (`lng_mediaview_date_time`). `civil` turns a Unix time into local
    /// wall-clock time; `now` is the local time now.
    pub fn date_line(&self, civil: impl Fn(i64) -> CivilTime, now: &CivilTime) -> String {
        let stamp = |unix: i64| {
            let date = civil(unix);
            format!("{} at {}", day_label(&date, now), hhmm(&date))
        };
        if self.last_usage_date > 0 {
            format!("Last used {}", stamp(self.last_usage_date))
        } else {
            format!("Created {}", stamp(self.addition_date))
        }
    }
}

/// In-flight passkey requests; wrapped as
/// `SettingsPurpose::Passkeys`. The passkey being removed is
/// [`PasskeysState::removing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasskeysPurpose {
    /// `getLoginPasskeys`; the answer is `passkeys`. Only the request the
    /// list stage waits for ([`PasskeyList::Asking`]) is taken for the
    /// list, so the answer of a look older than a removal is dropped.
    List,
    /// `removeLoginPasskey`; the answer is `ok`.
    Remove,
    /// `getPasskeyParameters`; the answer is `text` with the WebAuthn
    /// creation options.
    Parameters,
    /// `addLoginPasskey`; the answer is the new `passkey`.
    Add,
    /// `getApplicationConfig` while signed out, for
    /// `settings_display_passkeys` on the QR sign-in screen.
    LoginConfig,
    /// `getAuthenticationPasskeyParameters`; the answer is `text` with the
    /// WebAuthn request options.
    LoginParameters,
    /// `checkAuthenticationPasskey`; the answer is `ok`, and the
    /// authorization state moves on.
    CheckLogin,
}

/// The answers this module parses.
#[derive(Debug, Clone, PartialEq)]
pub enum PasskeysPayload {
    /// `passkeys`.
    List(Vec<Passkey>),
    /// `passkey` (the answer to `addLoginPasskey`).
    Added(Passkey),
    /// A `passkey` without an id: the registration cannot be finished.
    AddedUnreadable,
}

/// Parse `type_name`, or `None` for a type this module does not own.
pub fn parse_payload(type_name: &str, value: &Value) -> Option<PasskeysPayload> {
    match type_name {
        "passkeys" => Some(PasskeysPayload::List(
            value
                .get("passkeys")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(Passkey::parse).collect())
                .unwrap_or_default(),
        )),
        "passkey" => Some(
            Passkey::parse(value).map_or(PasskeysPayload::AddedUnreadable, PasskeysPayload::Added),
        ),
        _ => None,
    }
}

fn request(extra: RequestId, mut body: Value) -> String {
    crate::json_access::set(&mut body, "@extra", json!(extra.as_extra()));
    body.to_string()
}

/// `getLoginPasskeys = Passkeys` (TDLib 1.8.68 `td_api.tl:13161`).
pub fn get_login_passkeys(extra: RequestId) -> String {
    request(extra, json!({"@type": "getLoginPasskeys"}))
}

/// `removeLoginPasskey passkey_id:string = Ok` (`td_api.tl:13164`).
pub fn remove_login_passkey(extra: RequestId, passkey_id: &str) -> String {
    request(
        extra,
        json!({"@type": "removeLoginPasskey", "passkey_id": passkey_id}),
    )
}

/// `getPasskeyParameters = Text` (`td_api.tl:13153`).
pub fn get_passkey_parameters(extra: RequestId) -> String {
    request(extra, json!({"@type": "getPasskeyParameters"}))
}

/// `addLoginPasskey client_data:string attestation_object:bytes = Passkey`
/// (`td_api.tl:13158`); `bytes` travel as base64 in TDLib's JSON.
pub fn add_login_passkey(extra: RequestId, registration: &Registration) -> String {
    use base64::Engine;
    let attestation =
        base64::engine::general_purpose::STANDARD.encode(&registration.attestation_object);
    request(
        extra,
        json!({
            "@type": "addLoginPasskey",
            "client_data": registration.client_data_json,
            "attestation_object": attestation,
        }),
    )
}

/// Creation options fetched for a registration, reused while they are
/// within their own timeout (tdesktop `_pendingRegistration`).
#[derive(Debug, Clone)]
pub struct PendingRegistration {
    pub options: CreationOptions,
    pub fetched_at: Instant,
}

impl PendingRegistration {
    pub fn fresh(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.fetched_at)
            < Duration::from_millis(u64::from(self.options.timeout_ms))
    }
}

/// "Create Passkey" (tdesktop `Passkeys::initRegistration`): one stage at
/// a time. A request stage carries the id of its request, and only that
/// answer moves it on; the error, a dropped request and the box closing
/// (`PasskeysState::cancel_register`) each leave it.
#[derive(Debug, Clone, Default)]
pub enum PasskeyAdd {
    #[default]
    Idle,
    /// `getPasskeyParameters` is on its way.
    FetchingOptions(RequestId),
    /// Options ready for a ceremony the UI has not started yet; the UI
    /// takes them.
    Starting(PendingRegistration),
    /// The ceremony runs or ended; tdesktop keeps the options for their
    /// timeout and reuses them on the next try.
    Cached(PendingRegistration),
    /// `addLoginPasskey` is on its way.
    Sending(RequestId),
    /// A passkey was just added; the UI takes it and closes the box.
    Added,
    /// The attempt failed (the error, an unreadable answer); the About box
    /// says why. The next try or the box closing leaves it.
    Failed(String),
}

/// Deleting a passkey: one at a time, and why the last try failed. `Removing`
/// leaves on its own answer (`finish_remove`) or its error; `Failed` leaves
/// on the next try or when the confirmation is dismissed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RemoveStage {
    #[default]
    Idle,
    Removing(Removal),
    Failed(String),
}

/// The list a fetch in flight (or a failed one) stands on: what the screen
/// shows meanwhile, and whether an answer made it (tdesktop `listKnown`).
/// A re-ask keeps `Answered`, so the row stays usable while it runs and
/// after it fails.
#[derive(Debug, Clone)]
pub enum Basis {
    /// Not answered since the last change (never, or a removal or an add).
    Unanswered(Vec<Passkey>),
    /// The last answer.
    Answered(Vec<Passkey>),
}

impl Basis {
    fn list(&self) -> &[Passkey] {
        match self {
            Self::Unanswered(list) | Self::Answered(list) => list,
        }
    }

    fn list_mut(&mut self) -> &mut Vec<Passkey> {
        match self {
            Self::Unanswered(list) | Self::Answered(list) => list,
        }
    }
}

/// The passkey list as last fetched. The list the screen shows stays
/// through every stage, so it does not blank out under "Loading..." or an
/// error line. Each stage that asked remembers when (`at`, the 5 s throttle
/// of tdesktop `requestList`).
#[derive(Debug, Clone, Default)]
pub enum PasskeyList {
    /// Never asked.
    #[default]
    Unknown,
    /// A removal or an add made it out of date: the driver asks again at
    /// once.
    Stale(Vec<Passkey>),
    /// `getLoginPasskeys` is on its way as `request` (tdesktop `listKnown`
    /// is false only when no answer is behind it).
    Asking {
        at: Instant,
        basis: Basis,
        request: RequestId,
    },
    /// An answer arrived since the last change (tdesktop `listKnown`).
    Known { at: Instant, list: Vec<Passkey> },
    /// The ask failed with `error`; it is tried again with the next look
    /// after the throttle.
    Failed {
        at: Instant,
        basis: Basis,
        error: String,
    },
}

impl PasskeyList {
    /// The passkeys to show, in the server's order.
    pub fn shown(&self) -> &[Passkey] {
        match self {
            Self::Unknown => &[],
            Self::Stale(list) | Self::Known { list, .. } => list,
            Self::Asking { basis, .. } | Self::Failed { basis, .. } => basis.list(),
        }
    }

    /// Whether an answer stands behind the list shown.
    fn answered(&self) -> bool {
        match self {
            Self::Known { .. } => true,
            Self::Asking { basis, .. } | Self::Failed { basis, .. } => {
                matches!(basis, Basis::Answered(_))
            }
            Self::Unknown | Self::Stale(_) => false,
        }
    }

    fn shown_mut(&mut self) -> Option<&mut Vec<Passkey>> {
        match self {
            Self::Unknown => None,
            Self::Stale(list) | Self::Known { list, .. } => Some(list),
            Self::Asking { basis, .. } | Self::Failed { basis, .. } => Some(basis.list_mut()),
        }
    }

    fn take_basis(&mut self) -> Basis {
        match std::mem::take(self) {
            Self::Unknown => Basis::Unanswered(Vec::new()),
            Self::Stale(list) => Basis::Unanswered(list),
            Self::Known { list, .. } => Basis::Answered(list),
            Self::Asking { basis, .. } | Self::Failed { basis, .. } => basis,
        }
    }

    fn take_shown(&mut self) -> Vec<Passkey> {
        match self.take_basis() {
            Basis::Unanswered(list) | Basis::Answered(list) => list,
        }
    }

    fn asked_at(&self) -> Option<Instant> {
        match self {
            Self::Unknown | Self::Stale(_) => None,
            Self::Asking { at, .. } | Self::Known { at, .. } | Self::Failed { at, .. } => Some(*at),
        }
    }
}

/// A `removeLoginPasskey` in flight: the passkey and the request whose
/// answer settles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub passkey_id: String,
    pub request: RequestId,
}

/// The account's passkeys as last fetched (tdesktop `Data::Passkeys`).
#[derive(Debug, Clone, Default)]
pub struct PasskeysState {
    /// The passkey list and where its fetch stands.
    pub list: PasskeyList,
    /// The removal in flight; one at a time.
    pub removing: RemoveStage,
    /// The server lets the app show passkeys (`settings_display_passkeys`
    /// in the application config; off until it says so).
    pub display_allowed: bool,
    /// `passkeys_account_passkeys_max`; `None` until the app config said.
    pub max_passkeys: Option<usize>,
    /// "Create Passkey" from the options to the server's answer.
    pub add: PasskeyAdd,
    /// Signing in with a passkey (`login`).
    pub login: login::LoginState,
}

impl PasskeysState {
    /// Whether a look at the list should fetch it again (tdesktop
    /// `requestList`: never asked, or asked more than 5 s ago).
    pub fn wants_fetch(&self, now: Instant) -> bool {
        matches!(self.list, PasskeyList::Stale(_))
            || self
                .list
                .asked_at()
                .is_none_or(|at| now.saturating_duration_since(at) > LIST_FRESH_FOR)
    }

    /// The passkeys to show.
    pub fn passkeys(&self) -> &[Passkey] {
        self.list.shown()
    }

    /// A removal or an add made the list out of date.
    pub fn is_stale(&self) -> bool {
        matches!(self.list, PasskeyList::Stale(_))
    }

    /// An answer stands behind the list (tdesktop `listKnown`); a re-ask
    /// or a failed one keeps it.
    pub fn is_known(&self) -> bool {
        self.list.answered()
    }

    /// `getLoginPasskeys` went out as `request` at `now`: the list on
    /// screen stays, the last list error is gone.
    pub fn list_asked(&mut self, request: RequestId, now: Instant) {
        let basis = self.list.take_basis();
        self.list = PasskeyList::Asking {
            at: now,
            basis,
            request,
        };
    }

    /// Whether the list ask is out as `request`.
    pub fn list_waits_for(&self, request: RequestId) -> bool {
        matches!(&self.list, PasskeyList::Asking { request: asked, .. } if *asked == request)
    }

    /// The list ask `request` failed. One that is not the ask the list
    /// waits for (asked before a removal made the list stale) says nothing
    /// about the list now.
    pub fn list_failed(&mut self, request: RequestId, error: String) {
        if !self.list_waits_for(request) {
            return;
        }
        let at = self.list.asked_at().unwrap_or_else(Instant::now);
        let basis = self.list.take_basis();
        self.list = PasskeyList::Failed { at, basis, error };
    }

    /// The list answer to `request` arrived. One that is not the ask the
    /// list waits for is dropped: an ask older than the last removal still
    /// has the removed passkey, and the list stays stale for the look that
    /// follows.
    pub fn accept_list(&mut self, request: RequestId, list: Vec<Passkey>) {
        if !self.list_waits_for(request) {
            return;
        }
        let at = self.list.asked_at().unwrap_or_else(Instant::now);
        self.list = PasskeyList::Known { at, list };
    }

    /// Why the last list ask failed.
    pub fn list_error(&self) -> Option<&str> {
        match &self.list {
            PasskeyList::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// `removeLoginPasskey` for `passkey_id` went out as `request`.
    pub fn remove_started(&mut self, passkey_id: String, request: RequestId) {
        self.removing = RemoveStage::Removing(Removal {
            passkey_id,
            request,
        });
    }

    /// A removal is out and unanswered (one at a time).
    pub fn is_removing(&self) -> bool {
        matches!(self.removing, RemoveStage::Removing(_))
    }

    /// Why the last removal failed.
    pub fn remove_error(&self) -> Option<&str> {
        match &self.removing {
            RemoveStage::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The confirmation was dismissed or opened afresh: a failed removal's
    /// error goes; a removal in flight stays.
    pub fn dismiss_remove_error(&mut self) {
        if matches!(self.removing, RemoveStage::Failed(_)) {
            self.removing = RemoveStage::Idle;
        }
    }

    /// The `ok` of removal `request`: the passkey leaves the list. Only the
    /// removal in flight settles it.
    pub fn removal_answered(&mut self, request: Option<RequestId>) {
        let RemoveStage::Removing(removal) = &self.removing else {
            return;
        };
        if Some(removal.request) != request {
            return;
        }
        let passkey_id = removal.passkey_id.clone();
        self.finish_remove(&passkey_id);
    }

    /// The error of removal `request`: only the removal in flight fails.
    pub fn removal_failed(&mut self, request: Option<RequestId>, error: String) {
        if matches!(&self.removing, RemoveStage::Removing(removal) if Some(removal.request) == request)
        {
            self.removing = RemoveStage::Failed(error);
        }
    }

    /// Why the last add failed.
    pub fn add_error(&self) -> Option<&str> {
        match &self.add {
            PasskeyAdd::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The line the list screen shows under its header: the last removal,
    /// add or list failure.
    pub fn error(&self) -> Option<&str> {
        self.remove_error()
            .or_else(|| self.add_error())
            .or_else(|| self.list_error())
    }

    /// `removeLoginPasskey` succeeded: drop the passkey and ask again
    /// (tdesktop forgets the list and reloads it). The list is stale, so a
    /// list answer asked before the removal is dropped.
    pub fn finish_remove(&mut self, passkey_id: &str) {
        let mut list = self.list.take_shown();
        list.retain(|p| p.id != passkey_id);
        self.list = PasskeyList::Stale(list);
        self.removing = RemoveStage::Idle;
    }

    pub fn find(&self, passkey_id: &str) -> Option<&Passkey> {
        self.passkeys().iter().find(|p| p.id == passkey_id)
    }

    /// Custom emoji ids of the software icons in the list.
    pub fn icon_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.passkeys()
            .iter()
            .map(|p| p.software_icon_custom_emoji_id)
            .filter(|id| *id != 0)
    }

    /// Whether the Privacy and Security screen shows the Passkeys row:
    /// the server allows it, and the app can create passkeys or the
    /// account already has some.
    pub fn row_shown(&self) -> bool {
        self.display_allowed && (webauthn_supported() || !self.passkeys().is_empty())
    }

    /// Whether the QR sign-in screen shows "Log in using passkey" (tdesktop
    /// `setupPasskeyLink`: `settingsDisplayPasskeys` and WebAuthn).
    pub fn login_link_shown(&self) -> bool {
        self.display_allowed && webauthn_supported()
    }

    /// The value on the right of the row.
    pub fn row_label(&self) -> RowLabel {
        if !self.is_known() {
            return RowLabel::Loading;
        }
        match self.passkeys() {
            [] => RowLabel::Off,
            [one] => RowLabel::Name(one.name.clone()),
            many => RowLabel::Count(many.len()),
        }
    }

    /// Where a click on the row goes (tdesktop: nothing while loading,
    /// the "Protect your account" box without passkeys, else the list).
    pub fn row_target(&self) -> Option<RowTarget> {
        if !self.is_known() {
            None
        } else if self.passkeys().is_empty() {
            Some(RowTarget::About)
        } else {
            Some(RowTarget::List)
        }
    }

    /// Whether "Create Passkey" works (tdesktop `canRegister`: WebAuthn,
    /// and fewer passkeys than `passkeys_account_passkeys_max`).
    pub fn can_register(&self) -> bool {
        webauthn_supported()
            && self.passkeys().len() < self.max_passkeys.unwrap_or(DEFAULT_MAX_PASSKEYS)
    }

    /// The new passkey from `addLoginPasskey` joins the list (tdesktop
    /// appends it) and the registration is over.
    pub fn accept_added(&mut self, passkey: Passkey) {
        // A list never asked for becomes one that asks again.
        if matches!(self.list, PasskeyList::Unknown) {
            self.list = PasskeyList::Stale(Vec::new());
        }
        if let Some(list) = self.list.shown_mut() {
            list.retain(|p| p.id != passkey.id);
            list.push(passkey);
        }
        self.add = PasskeyAdd::Added;
    }

    /// The creation options of request `id` arrived: keep them and start
    /// the ceremony. `false` when they cannot be read (the stage fails with
    /// a line the box shows; tdesktop does nothing). An answer to anything but the request the stage waits for
    /// (the box was closed, a newer ask went out) is ignored.
    pub fn accept_parameters(&mut self, id: Option<RequestId>, json: &str, now: Instant) -> bool {
        if !matches!(&self.add, PasskeyAdd::FetchingOptions(waiting) if Some(*waiting) == id) {
            return true;
        }
        let Some(options) = CreationOptions::parse(json) else {
            self.add =
                PasskeyAdd::Failed("Could not add the passkey: unreadable parameters".into());
            return false;
        };
        self.add = PasskeyAdd::Starting(PendingRegistration {
            options,
            fetched_at: now,
        });
        true
    }

    /// Whether "Create Passkey" has options, an options request or the
    /// result on its way.
    pub fn add_busy(&self) -> bool {
        matches!(
            self.add,
            PasskeyAdd::FetchingOptions(_) | PasskeyAdd::Starting(_) | PasskeyAdd::Sending(_)
        )
    }

    /// The UI starts the ceremony with the options that landed; they stay
    /// cached for the next try.
    pub fn take_starting(&mut self) -> Option<CreationOptions> {
        match std::mem::take(&mut self.add) {
            PasskeyAdd::Starting(pending) => {
                let options = pending.options.clone();
                self.add = PasskeyAdd::Cached(pending);
                Some(options)
            }
            other => {
                self.add = other;
                None
            }
        }
    }

    /// The UI consumes "just added".
    pub fn take_added(&mut self) -> bool {
        if matches!(self.add, PasskeyAdd::Added) {
            self.add = PasskeyAdd::Idle;
            true
        } else {
            false
        }
    }

    /// The box was closed (or the lock closed it): options still on their
    /// way or waiting to start are dropped, so no ceremony starts for a
    /// box nobody sees. Cached options stay for the next try, and a result
    /// already sent stays sent.
    pub fn cancel_register(&mut self) {
        if matches!(
            self.add,
            PasskeyAdd::FetchingOptions(_)
                | PasskeyAdd::Starting(_)
                | PasskeyAdd::Added
                | PasskeyAdd::Failed(_)
        ) {
            self.add = PasskeyAdd::Idle;
        }
    }

    /// A request the registration waits for is no longer pending and
    /// nothing answered it (a dropped connection): the stage ends, and a
    /// lost `addLoginPasskey` makes the list fetched again, since the
    /// server may have added the passkey.
    pub fn drop_lost_register(&mut self, pending: impl Fn(RequestId) -> bool) {
        match &self.add {
            PasskeyAdd::FetchingOptions(id) if !pending(*id) => self.add = PasskeyAdd::Idle,
            PasskeyAdd::Sending(id) if !pending(*id) => {
                self.add = PasskeyAdd::Idle;
                if matches!(self.list, PasskeyList::Known { .. }) {
                    self.list = PasskeyList::Stale(self.list.take_shown());
                }
            }
            _ => {}
        }
    }

    /// The request `id` of the registration failed (error, unreadable
    /// answer, or lost): the stage waiting for it ends and the options are
    /// dropped (tdesktop does); the box shows `error`. `false` when `id` is
    /// not what the stage waits for (the box was closed, a newer ask went
    /// out).
    pub fn fail_register(&mut self, id: Option<RequestId>, error: String) -> bool {
        match &self.add {
            PasskeyAdd::FetchingOptions(waiting) | PasskeyAdd::Sending(waiting)
                if Some(*waiting) == id =>
            {
                self.add = PasskeyAdd::Failed(error);
                true
            }
            _ => false,
        }
    }
}

/// The Passkeys row value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowLabel {
    /// `lng_profile_loading`.
    Loading,
    /// The only passkey's name (as is, even when empty).
    Name(String),
    Count(usize),
    /// `lng_settings_cloud_password_off`.
    Off,
}

impl RowLabel {
    pub fn text(&self) -> String {
        match self {
            Self::Loading => "Loading...".into(),
            Self::Name(name) => name.clone(),
            Self::Count(count) => count.to_string(),
            Self::Off => "Off".into(),
        }
    }
}

/// What a click on the Passkeys row opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowTarget {
    /// The "Protect your account" box (`PasskeysNoneBox`).
    About,
    /// The Passkeys section with the list.
    List,
}

/// The delete confirmation text: "Once deleted, {name} can't be used to
/// log in." (or the unnamed wording), a blank line and the reminder
/// (`lng_settings_passkeys_delete_sure_*`). The name is bold in the box,
/// so it is returned apart: `(before, name, after)`.
pub fn delete_question(name: &str) -> (&'static str, Option<String>, String) {
    const ABOUT2: &str = "Don't forget to remove it from your password manager too.";
    if name.is_empty() {
        (
            "",
            None,
            format!("Once deleted, this passkey can't be used to log in.\n\n{ABOUT2}"),
        )
    } else {
        (
            "Once deleted, ",
            Some(name.to_string()),
            format!(" can't be used to log in.\n\n{ABOUT2}"),
        )
    }
}

/// The passkey switches of the application config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PasskeysConfig {
    /// `settings_display_passkeys` (tdesktop `settingsDisplayPasskeys`, off
    /// unless the server says otherwise).
    pub display: bool,
    /// `passkeys_account_passkeys_max` (tdesktop default 10), when sent.
    pub max: Option<usize>,
}

fn config_member<'a>(config: &'a Value, key: &str) -> Option<&'a Value> {
    config
        .get("members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|m| m.get("key").and_then(Value::as_str) == Some(key))
        .and_then(|m| m.get("value"))
}

/// The passkey switches from `getApplicationConfig`'s `jsonValueObject`.
pub fn config_from_app_config(config: &Value) -> PasskeysConfig {
    let display = config_member(config, "settings_display_passkeys")
        .filter(|v| v.get("@type").and_then(Value::as_str) == Some("jsonValueBoolean"))
        .and_then(|v| v.get("value"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let max = config_member(config, "passkeys_account_passkeys_max")
        .filter(|v| v.get("@type").and_then(Value::as_str) == Some("jsonValueNumber"))
        .and_then(|v| v.get("value"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v as usize);
    PasskeysConfig { display, max }
}

/// `settings_display_passkeys` alone.
pub fn display_allowed_from_config(config: &Value) -> bool {
    config_from_app_config(config).display
}
