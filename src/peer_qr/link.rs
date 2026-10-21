//! Which peers get the box and which link it encodes (tdesktop
//! `DefaultShowFillPeerQrBoxCallback`, `Info::Profile::LinkValue` and
//! `OwnQrLinkValue`).

use crate::profile_forms::profile_link;
use crate::telegram::requests::PrivacyWho;

/// Seconds before expiry when a temporary link is no longer copied or
/// shared (tdesktop `kShareCutoff`).
pub const SHARE_CUTOFF_SECS: i64 = 3;

/// The current user's box shows, and so does any user, group or channel
/// with a public username.
pub fn eligible(is_self: bool, active_usernames: &[String]) -> bool {
    is_self || active_usernames.iter().any(|name| !name.is_empty())
}

/// The name under another peer's code: `@USERNAME`.
pub fn username_label(username: &str) -> String {
    format!("@{}", username.trim_start_matches('@')).to_uppercase()
}

/// What the current user's code encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnLinkKind {
    Username,
    Phone,
    /// A temporary contact link, for an account with no username whose
    /// number is hidden.
    Contact,
}

/// The current user's link and the text under the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnQrLink {
    pub kind: OwnLinkKind,
    pub link: String,
    pub label: String,
    /// Unix time the link stops working; 0 when it is permanent.
    pub expires_at: i64,
}

/// A `getUserLink` answer, with `expires_in` made absolute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserLinkData {
    pub url: String,
    /// 0 when permanent.
    pub expires_at: i64,
}

/// The `getUserLink` fetch for the current user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UserLinkFetch {
    #[default]
    Idle,
    Loading,
    Ready(UserLinkData),
    Failed,
}

/// What the own-link resolution needs from the session.
#[derive(Debug, Clone, Copy)]
pub struct OwnLinkInputs<'a> {
    pub username: &'a str,
    /// Raw digits as TDLib sends them.
    pub phone: &'a str,
    /// The number as the profile shows it.
    pub phone_label: &'a str,
    /// `userPrivacySettingShowPhoneNumber`, once loaded.
    pub show_phone: Option<PrivacyWho>,
    /// `userPrivacySettingAllowFindingByPhoneNumber`, once loaded.
    pub find_by_phone: Option<PrivacyWho>,
    pub contact: &'a UserLinkFetch,
    pub now: i64,
}

/// Where the current user's link stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnLinkState {
    Ready(OwnQrLink),
    /// The phone privacy rules are still loading.
    Waiting,
    /// No username and a hidden number: a temporary link is needed
    /// (`getUserLink`), or the last one expired.
    NeedsContactLink,
    /// The temporary link could not be made.
    Unavailable,
}

/// `m:ss` left on a temporary link (tdesktop `CountdownText`).
pub fn countdown_text(remaining: i64) -> String {
    let remaining = remaining.max(0);
    format!("{}:{:02}", remaining / 60, remaining % 60)
}

/// tdesktop `OwnQrLinkState::refresh`: the username link, else the phone
/// link when the number is visible (not hidden from everyone with
/// finding by number limited), else a temporary contact link.
pub fn own_link(inputs: OwnLinkInputs<'_>) -> OwnLinkState {
    let username = inputs.username.trim_start_matches('@');
    if !username.is_empty() {
        return OwnLinkState::Ready(OwnQrLink {
            kind: OwnLinkKind::Username,
            link: profile_link(username),
            label: username_label(username),
            expires_at: 0,
        });
    }
    let (Some(show), Some(find)) = (inputs.show_phone, inputs.find_by_phone) else {
        return OwnLinkState::Waiting;
    };
    let digits: String = inputs.phone.chars().filter(char::is_ascii_digit).collect();
    let hidden = show == PrivacyWho::Nobody && find != PrivacyWho::Everybody;
    if !digits.is_empty() && !hidden {
        return OwnLinkState::Ready(OwnQrLink {
            kind: OwnLinkKind::Phone,
            link: format!("https://t.me/+{digits}"),
            label: inputs.phone_label.to_string(),
            expires_at: 0,
        });
    }
    match inputs.contact {
        UserLinkFetch::Ready(data) if data.url.is_empty() => OwnLinkState::Unavailable,
        UserLinkFetch::Ready(data) if data.expires_at == 0 => OwnLinkState::Ready(OwnQrLink {
            kind: OwnLinkKind::Contact,
            link: data.url.clone(),
            label: String::new(),
            expires_at: 0,
        }),
        UserLinkFetch::Ready(data) if data.expires_at > inputs.now => {
            OwnLinkState::Ready(OwnQrLink {
                kind: OwnLinkKind::Contact,
                link: data.url.clone(),
                label: countdown_text(data.expires_at - inputs.now),
                expires_at: data.expires_at,
            })
        }
        UserLinkFetch::Failed => OwnLinkState::Unavailable,
        UserLinkFetch::Loading => OwnLinkState::Waiting,
        UserLinkFetch::Idle | UserLinkFetch::Ready(_) => OwnLinkState::NeedsContactLink,
    }
}
