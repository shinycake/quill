//! Login passkeys of the signed-in account, after Telegram Desktop's
//! `data/components/passkeys.*` and `settings/sections/settings_passkeys.cpp`.
//!
//! Pure core: the TDLib types (`passkey`, `passkeys`), the requests
//! (`getLoginPasskeys`, `removeLoginPasskey`), the cached list and the
//! Privacy and Security row logic. The driver is `connect::passkeys`, the
//! reducer `state::domains::settings::passkeys`, the screens
//! `ui::security::passkeys`.
//!
//! Creating a passkey and signing in with one need the platform WebAuthn
//! API, which Quill does not have yet: [`webauthn_supported`] is the one
//! switch that work turns on.
#![deny(clippy::indexing_slicing)]

#[cfg(test)]
mod tests;

use crate::ids::RequestId;
use crate::local_time::{CivilTime, day_label, hhmm};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a fetched list counts as fresh before the next look asks
/// again (tdesktop `kRequestTimeout`).
pub const LIST_FRESH_FOR: Duration = Duration::from_secs(5);

/// The name a passkey without one shows (`lng_settings_passkey_unknown`).
pub const UNNAMED: &str = "Passkey";

/// Whether this build can create passkeys and sign in with them through
/// the platform WebAuthn API (tdesktop `Platform::WebAuthn::IsSupported`).
///
/// Always `false` for now: the WebAuthn backends (macOS, Windows, Linux
/// security keys and phones) are separate work. Turning this on is what
/// shows the Passkeys row on accounts without passkeys and enables the
/// "Create Passkey" button.
pub const fn webauthn_supported() -> bool {
    false
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
    /// `getLoginPasskeys`; the answer is `passkeys`.
    List,
    /// `removeLoginPasskey`; the answer is `ok`.
    Remove,
}

/// The answers this module parses.
#[derive(Debug, Clone, PartialEq)]
pub enum PasskeysPayload {
    /// `passkeys`.
    List(Vec<Passkey>),
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

    /// The list answer arrived.
    pub fn accept_list(&mut self, list: Vec<Passkey>) {
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

    /// Whether "Create Passkey" works (tdesktop `canRegister`).
    pub fn can_register(&self) -> bool {
        webauthn_supported()
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

/// `settings_display_passkeys` from `getApplicationConfig`'s
/// `jsonValueObject` (tdesktop `AppConfig::settingsDisplayPasskeys`, off
/// unless the server says otherwise).
pub fn display_allowed_from_config(config: &Value) -> bool {
    config
        .get("members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|m| m.get("key").and_then(Value::as_str) == Some("settings_display_passkeys"))
        .and_then(|m| m.get("value"))
        .filter(|v| v.get("@type").and_then(Value::as_str) == Some("jsonValueBoolean"))
        .and_then(|v| v.get("value"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
