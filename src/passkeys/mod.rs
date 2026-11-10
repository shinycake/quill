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
    /// `getLoginPasskeys`; the answer is `passkeys`. Carries the
    /// [`PasskeysState::generation`] it was asked at, so the answer of a
    /// look older than a removal is not taken for the current list.
    List(u32),
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
}

/// The account's passkeys as last fetched (tdesktop `Data::Passkeys`).
#[derive(Debug, Clone, Default)]
pub struct PasskeysState {
    /// The passkeys of the last answer, in the server's order.
    pub list: Vec<Passkey>,
    /// An answer arrived since the last change (tdesktop `listKnown`).
    pub known: bool,
    /// When the last list request went out.
    pub requested_at: Option<Instant>,
    /// A removal succeeded: the driver asks for the list again.
    pub stale: bool,
    /// The passkey a `removeLoginPasskey` is in flight for.
    pub removing: Option<String>,
    /// Why the last fetch or removal failed; cleared on the next try.
    pub error: Option<String>,
    /// The server lets the app show passkeys (`settings_display_passkeys`
    /// in the application config; off until it says so).
    pub display_allowed: bool,
    /// `passkeys_account_passkeys_max`; `None` until the app config said.
    pub max_passkeys: Option<usize>,
    /// Bumped by a removal: a list answer asked before it is out of date.
    pub generation: u32,
    /// "Create Passkey" from the options to the server's answer.
    pub add: PasskeyAdd,
    /// Signing in with a passkey (`login`).
    pub login: login::LoginState,
}

impl PasskeysState {
    /// Whether a look at the list should fetch it again (tdesktop
    /// `requestList`: never asked, or asked more than 5 s ago).
    pub fn wants_fetch(&self, now: Instant) -> bool {
        self.stale
            || self
                .requested_at
                .is_none_or(|at| now.saturating_duration_since(at) > LIST_FRESH_FOR)
    }

    /// The list answer arrived. One asked before the last removal
    /// (`generation`) is dropped: it still has the removed passkey, and the
    /// list stays stale for the look that follows.
    pub fn accept_list(&mut self, list: Vec<Passkey>, generation: u32) {
        if generation != self.generation {
            return;
        }
        self.list = list;
        self.known = true;
        self.stale = false;
    }

    /// `removeLoginPasskey` succeeded: drop the passkey and ask again
    /// (tdesktop forgets the list and reloads it).
    pub fn finish_remove(&mut self, passkey_id: &str) {
        self.list.retain(|p| p.id != passkey_id);
        self.removing = None;
        self.error = None;
        self.known = false;
        self.stale = true;
        self.requested_at = None;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn find(&self, passkey_id: &str) -> Option<&Passkey> {
        self.list.iter().find(|p| p.id == passkey_id)
    }

    /// Custom emoji ids of the software icons in the list.
    pub fn icon_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.list
            .iter()
            .map(|p| p.software_icon_custom_emoji_id)
            .filter(|id| *id != 0)
    }

    /// Whether the Privacy and Security screen shows the Passkeys row:
    /// the server allows it, and the app can create passkeys or the
    /// account already has some.
    pub fn row_shown(&self) -> bool {
        self.display_allowed && (webauthn_supported() || !self.list.is_empty())
    }

    /// Whether the QR sign-in screen shows "Log in using passkey" (tdesktop
    /// `setupPasskeyLink`: `settingsDisplayPasskeys` and WebAuthn).
    pub fn login_link_shown(&self) -> bool {
        self.display_allowed && webauthn_supported()
    }

    /// The value on the right of the row.
    pub fn row_label(&self) -> RowLabel {
        if !self.known {
            return RowLabel::Loading;
        }
        match self.list.as_slice() {
            [] => RowLabel::Off,
            [one] => RowLabel::Name(one.name.clone()),
            many => RowLabel::Count(many.len()),
        }
    }

    /// Where a click on the row goes (tdesktop: nothing while loading,
    /// the "Protect your account" box without passkeys, else the list).
    pub fn row_target(&self) -> Option<RowTarget> {
        if !self.known {
            None
        } else if self.list.is_empty() {
            Some(RowTarget::About)
        } else {
            Some(RowTarget::List)
        }
    }

    /// Whether "Create Passkey" works (tdesktop `canRegister`: WebAuthn,
    /// and fewer passkeys than `passkeys_account_passkeys_max`).
    pub fn can_register(&self) -> bool {
        webauthn_supported() && self.list.len() < self.max_passkeys.unwrap_or(DEFAULT_MAX_PASSKEYS)
    }

    /// The new passkey from `addLoginPasskey` joins the list (tdesktop
    /// appends it) and the registration is over.
    pub fn accept_added(&mut self, passkey: Passkey) {
        self.list.retain(|p| p.id != passkey.id);
        self.list.push(passkey);
        self.add = PasskeyAdd::Added;
        self.error = None;
    }

    /// The creation options of request `id` arrived: keep them and start
    /// the ceremony. `false` when they cannot be read (tdesktop then does
    /// nothing). An answer to anything but the request the stage waits for
    /// (the box was closed, a newer ask went out) is ignored.
    pub fn accept_parameters(&mut self, id: Option<RequestId>, json: &str, now: Instant) -> bool {
        if !matches!(&self.add, PasskeyAdd::FetchingOptions(waiting) if Some(*waiting) == id) {
            return true;
        }
        let Some(options) = CreationOptions::parse(json) else {
            self.add = PasskeyAdd::Idle;
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
            PasskeyAdd::FetchingOptions(_) | PasskeyAdd::Starting(_) | PasskeyAdd::Added
        ) {
            self.add = PasskeyAdd::Idle;
        }
    }

    /// The request `id` of the registration failed (error, unreadable
    /// answer, or lost): the stage waiting for it ends and the options are
    /// dropped (tdesktop does). `false` when `id` is not what the stage
    /// waits for (the box was closed, a newer ask went out).
    pub fn fail_register(&mut self, id: Option<RequestId>) -> bool {
        match &self.add {
            PasskeyAdd::FetchingOptions(waiting) | PasskeyAdd::Sending(waiting)
                if Some(*waiting) == id =>
            {
                self.add = PasskeyAdd::Idle;
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
