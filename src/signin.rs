//! Sign-in screen logic shared by the UI and its tests: the code-delivery
//! description, the resend countdown, the banned-number help mail and the
//! account-count limit. Mirrors tdesktop's `intro_code.cpp`,
//! `phone_banned_box.cpp` and `Main::Domain::maxAccounts`.

use crate::state::AuthRequestError;
use crate::telegram::envelope::{CodeDelivery, CodeKind, EmailResetState, ErrorClass};

/// Seconds of a flood wait still to run, `None` when the error is not a
/// flood or TDLib gave no wait.
pub fn flood_remaining(err: &AuthRequestError, elapsed: i64) -> Option<i64> {
    if err.class != ErrorClass::Flood {
        return None;
    }
    err.flood_wait_secs
        .map(|secs| remaining_secs(i64::try_from(secs).unwrap_or(i64::MAX), elapsed))
}

/// The error line under a sign-in form. A flood wait counts down live
/// (tdesktop's `lng_flood_error` plus the real wait); everything else is
/// the classified message.
pub fn auth_error_line(err: &AuthRequestError, elapsed: i64) -> String {
    if err.class != ErrorClass::Flood {
        return err.user_message();
    }
    match flood_remaining(err, elapsed) {
        None => "Too many tries. Please try again later.".to_string(),
        Some(0) => "You can try again now.".to_string(),
        Some(left) => format!(
            "Too many tries. Please try again in {}.",
            format_countdown(left)
        ),
    }
}

/// `m:ss`, or `h:mm:ss` from an hour up (tdesktop `lng_code_call`).
pub fn format_countdown(secs: i64) -> String {
    let secs = secs.max(0);
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// Seconds left of a server-specified wait that started `elapsed` ago.
pub fn remaining_secs(total: i64, elapsed: i64) -> i64 {
    (total - elapsed.max(0)).max(0)
}

/// The sentence under the title that says where the code went. `phone` is
/// the number the code was requested for (may be empty).
pub fn delivery_description(delivery: &CodeDelivery, phone: &str) -> String {
    let detail = &delivery.detail;
    match delivery.kind {
        CodeKind::TelegramMessage => {
            "A code was sent via Telegram to your other devices, if you have any connected.".into()
        }
        CodeKind::Sms | CodeKind::Other => {
            "We've sent an activation code to your phone. Please enter it below.".into()
        }
        CodeKind::Call => "Telegram will call you and read the code out loud.".into(),
        CodeKind::FlashCall => match detail.call_number.as_str() {
            "" => "Telegram will call you and hang up right away. Don't answer. The code is the number that called.".into(),
            pattern => format!(
                "Telegram will call you from a number like {pattern} and hang up right away. Don't answer. The code is the full number that called."
            ),
        },
        CodeKind::MissedCall => {
            let digits = match detail.missed_digits {
                Some(n) => format!("the last {n} digit{} of", if n == 1 { "" } else { "s" }),
                None => "the last digits of".to_string(),
            };
            match detail.call_number.as_str() {
                "" => format!("Telegram will call you and hang up right away. Don't answer. The code is {digits} the number that called."),
                prefix => format!(
                    "Telegram will call you from a number starting with {prefix} and hang up right away. Don't answer. The code is {digits} that number."
                ),
            }
        }
        // tdesktop `lng_intro_fragment_about`.
        CodeKind::Fragment => {
            if phone.is_empty() {
                "Get the code in the Anonymous Numbers section on Fragment.".into()
            } else {
                format!("Get the code for {phone} in the Anonymous Numbers section on Fragment.")
            }
        }
        CodeKind::Firebase => "Telegram wants to verify this device before it sends the code, and only the official Android and iOS apps can do that. Ask for the code another way below, or sign in with a QR code.".into(),
    }
}

/// The whole body of the code step: phone, delivery text, expected length.
pub fn code_step_body(delivery: &CodeDelivery, code_length: Option<i32>, phone: &str) -> String {
    let mut body = String::new();
    // Fragment names the number itself.
    if !phone.is_empty() && delivery.kind != CodeKind::Fragment {
        body.push_str(&format!("Code for {phone}. "));
    }
    body.push_str(&delivery_description(delivery, phone));
    // A missed call already says how many digits to type.
    let counts = matches!(
        delivery.kind,
        CodeKind::TelegramMessage | CodeKind::Sms | CodeKind::Call | CodeKind::Other
    );
    if let (true, Some(len)) = (counts, code_length) {
        body.push_str(&format!(" It has {len} digits."));
    }
    body
}

/// Title of the code step; Fragment gets tdesktop's `lng_intro_fragment_title`.
pub fn code_step_title(kind: CodeKind) -> &'static str {
    match kind {
        CodeKind::Fragment => "Enter code",
        _ => "Enter the code",
    }
}

/// The Fragment link to open, when this delivery has a usable one.
pub fn fragment_url(delivery: &CodeDelivery) -> Option<&str> {
    (delivery.kind == CodeKind::Fragment && !delivery.detail.url.is_empty())
        .then_some(delivery.detail.url.as_str())
}

/// Explainer for `authorizationStateWaitPremiumPurchase`. Quill never takes
/// the purchase, so this only says what Telegram asks and where to go.
pub fn premium_explainer(premium_day_count: i32) -> String {
    let grant = if premium_day_count > 0 {
        format!(
            " The purchase includes {}.",
            format_wait_period(i64::from(premium_day_count) * 86_400)
        )
    } else {
        String::new()
    };
    format!(
        "Telegram asks for a Premium subscription before this phone number can sign in.{grant} Quill can't take store payments. Sign in with a QR code from a phone where you're already signed in, or buy Premium in the official Telegram app for your phone."
    )
}

/// `mailto:` for the support address TDLib supplies with the premium state.
/// `None` unless the address looks like a plain mailbox.
pub fn premium_support_mailto(address: &str, subject: &str) -> Option<String> {
    let (local, domain) = address.split_once('@')?;
    let plain = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-+".contains(c))
    };
    if !plain(local) || !plain(domain) || !domain.contains('.') || domain.contains("..") {
        return None;
    }
    Some(if subject.is_empty() {
        format!("mailto:{address}")
    } else {
        format!("mailto:{address}?subject={}", url_encode(subject))
    })
}

/// The resend control on the code screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResendOption {
    pub label: String,
    /// False while the server-specified timeout runs.
    pub ready: bool,
    pub remaining_secs: i64,
}

/// What the resend control shows `elapsed` seconds after the code was sent.
/// `None` when TDLib offers no further delivery (`next_type` is null), since
/// `resendAuthenticationCode` would be refused.
pub fn resend_option(delivery: &CodeDelivery, elapsed: i64) -> Option<ResendOption> {
    let next = delivery.next?;
    let remaining = remaining_secs(i64::from(delivery.timeout_secs), elapsed);
    let ready = remaining == 0;
    let label = match (next, ready) {
        (CodeKind::Call, true) => "Call me".to_string(),
        (CodeKind::Call, false) => {
            format!("Telegram will call you in {}", format_countdown(remaining))
        }
        (CodeKind::Sms, true) => "Send code via SMS".to_string(),
        (CodeKind::Sms, false) => format!("Send code via SMS in {}", format_countdown(remaining)),
        (CodeKind::FlashCall | CodeKind::MissedCall, true) => "Call me".to_string(),
        (CodeKind::FlashCall | CodeKind::MissedCall, false) => {
            format!("Telegram will call you in {}", format_countdown(remaining))
        }
        (CodeKind::Fragment, true) => "Get the code from Fragment".to_string(),
        (CodeKind::Fragment, false) => {
            format!(
                "Get the code from Fragment in {}",
                format_countdown(remaining)
            )
        }
        (_, true) => "Send the code again".to_string(),
        (_, false) => format!("Send the code again in {}", format_countdown(remaining)),
    };
    Some(ResendOption {
        label,
        ready,
        remaining_secs: remaining,
    })
}

/// Text for the email step's reset option; `None` when TDLib offers none.
/// `elapsed` is the time since the state arrived (counts a pending reset down).
pub fn email_reset_label(state: EmailResetState, elapsed: i64) -> Option<String> {
    match state {
        EmailResetState::Unavailable => None,
        EmailResetState::Available { wait_period } => Some(if wait_period <= 0 {
            "Can't access this email? Reset it".to_string()
        } else {
            format!(
                "Can't access this email? Reset it ({} wait)",
                format_wait_period(i64::from(wait_period))
            )
        }),
        EmailResetState::Pending { reset_in } => Some(format!(
            "Email reset requested. It will complete in {}.",
            format_wait_period(remaining_secs(i64::from(reset_in), elapsed))
        )),
    }
}

/// "3 days", "5 hours", "12 minutes", "40 seconds": the largest whole unit.
pub fn format_wait_period(secs: i64) -> String {
    let (n, unit) = match secs.max(0) {
        s if s >= 86_400 => (s / 86_400, "day"),
        s if s >= 3600 => (s / 3600, "hour"),
        s if s >= 60 => (s / 60, "minute"),
        s => (s, "second"),
    };
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

/// Telegram's address for banned-number appeals (tdesktop `SendToBannedHelp`).
pub const BANNED_HELP_EMAIL: &str = "login@stel.com";

fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The prefilled `mailto:` for the banned-number box's Help button.
pub fn banned_help_mailto(phone: &str, app_version: &str, os: &str) -> String {
    let subject = format!("Banned phone number: {phone}");
    let body = format!(
        "I'm trying to use my mobile phone number: {phone}\nBut Telegram says it's banned. Please help.\n\nApp version: {app_version}\nOS version: {os}"
    );
    format!(
        "mailto:?to={}&subject={}&body={}",
        url_encode(BANNED_HELP_EMAIL),
        url_encode(&subject),
        url_encode(&body)
    )
}

/// tdesktop's `kMaxAccounts`: three accounts, plus one per Premium account,
/// up to six.
pub const MAX_ACCOUNTS: usize = 3;
pub const MAX_ACCOUNTS_PREMIUM: usize = 6;

pub fn max_accounts(premium_accounts: usize) -> usize {
    (MAX_ACCOUNTS + premium_accounts).min(MAX_ACCOUNTS_PREMIUM)
}

/// Why another account cannot be added, or `None` when there is room.
pub fn add_account_blocker(existing: usize, premium_accounts: usize) -> Option<String> {
    let max = max_accounts(premium_accounts);
    if existing < max {
        return None;
    }
    Some(if max >= MAX_ACCOUNTS_PREMIUM {
        format!("You can have up to {max} accounts. Remove one to add another.")
    } else {
        format!(
            "You can have up to {max} accounts. Remove one, or use Telegram Premium to add more (up to {MAX_ACCOUNTS_PREMIUM})."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        BANNED_HELP_EMAIL, CodeDelivery, CodeKind, EmailResetState, add_account_blocker,
        auth_error_line, banned_help_mailto, code_step_body, code_step_title, delivery_description,
        email_reset_label, flood_remaining, format_countdown, format_wait_period, fragment_url,
        max_accounts, premium_explainer, premium_support_mailto, resend_option,
    };
    use crate::telegram::envelope::CodeDetail;

    fn delivery(next: Option<CodeKind>, timeout: i32) -> CodeDelivery {
        CodeDelivery {
            kind: CodeKind::TelegramMessage,
            next,
            timeout_secs: timeout,
            detail: CodeDetail::default(),
        }
    }

    #[test]
    fn countdown_formats_minutes_and_hours() {
        assert_eq!(format_countdown(0), "0:00");
        assert_eq!(format_countdown(7), "0:07");
        assert_eq!(format_countdown(59), "0:59");
        assert_eq!(format_countdown(60), "1:00");
        assert_eq!(format_countdown(3599), "59:59");
        assert_eq!(format_countdown(3600), "1:00:00");
        assert_eq!(format_countdown(3725), "1:02:05");
        assert_eq!(format_countdown(-5), "0:00");
    }

    #[test]
    fn resend_counts_down_then_unlocks() {
        let sms = delivery(Some(CodeKind::Sms), 60);
        let waiting = resend_option(&sms, 18).unwrap();
        assert!(!waiting.ready);
        assert_eq!(waiting.remaining_secs, 42);
        assert_eq!(waiting.label, "Send code via SMS in 0:42");
        let ready = resend_option(&sms, 60).unwrap();
        assert!(ready.ready);
        assert_eq!(ready.label, "Send code via SMS");
        // Clock skew never goes negative or re-locks.
        assert!(resend_option(&sms, 600).unwrap().ready);
        assert_eq!(resend_option(&sms, -3).unwrap().remaining_secs, 60);
    }

    #[test]
    fn resend_label_follows_next_type() {
        let call = delivery(Some(CodeKind::Call), 120);
        assert_eq!(
            resend_option(&call, 0).unwrap().label,
            "Telegram will call you in 2:00"
        );
        assert_eq!(resend_option(&call, 120).unwrap().label, "Call me");
        let other = delivery(Some(CodeKind::Firebase), 0);
        assert_eq!(
            resend_option(&other, 0).unwrap().label,
            "Send the code again"
        );
    }

    #[test]
    fn no_next_type_means_no_resend() {
        assert_eq!(resend_option(&delivery(None, 60), 100), None);
    }

    #[test]
    fn delivery_text_names_the_channel() {
        let of = |kind| CodeDelivery {
            kind,
            ..delivery(None, 0)
        };
        assert!(delivery_description(&of(CodeKind::TelegramMessage), "").contains("via Telegram"));
        assert!(delivery_description(&of(CodeKind::Sms), "").contains("your phone"));
        assert!(delivery_description(&of(CodeKind::Call), "").contains("call"));
    }

    #[test]
    fn email_reset_states() {
        assert_eq!(email_reset_label(EmailResetState::Unavailable, 0), None);
        assert_eq!(
            email_reset_label(EmailResetState::Available { wait_period: 0 }, 0).unwrap(),
            "Can't access this email? Reset it"
        );
        assert!(
            email_reset_label(
                EmailResetState::Available {
                    wait_period: 86_400 * 7
                },
                0
            )
            .unwrap()
            .contains("7 days")
        );
        assert_eq!(
            email_reset_label(EmailResetState::Pending { reset_in: 7200 }, 3600).unwrap(),
            "Email reset requested. It will complete in 1 hour."
        );
        assert_eq!(format_wait_period(1), "1 second");
        assert_eq!(format_wait_period(90), "1 minute");
        assert_eq!(format_wait_period(86_400 * 3), "3 days");
    }

    #[test]
    fn banned_mail_is_prefilled_and_encoded() {
        assert_eq!(BANNED_HELP_EMAIL, "login@stel.com");
        let url = banned_help_mailto("+15550100199", "1.2.3", "macOS 15");
        assert!(url.starts_with("mailto:?to=login%40stel.com&subject="));
        assert!(url.contains("Banned%20phone%20number%3A%20%2B15550100199"));
        assert!(url.contains("%0A"));
        assert!(url.contains("App%20version%3A%201.2.3"));
        assert!(!url.contains(' '));
        assert!(!url.contains('\n'));
    }

    #[test]
    fn flood_errors_count_down_live() {
        use crate::state::{AuthRequestError, RequestPurpose};
        use crate::telegram::envelope::ErrorClass;
        let flood = |secs| AuthRequestError {
            purpose: RequestPurpose::CheckAuthenticationCode,
            class: ErrorClass::Flood,
            flood_wait_secs: secs,
        };
        assert_eq!(flood_remaining(&flood(Some(60)), 18), Some(42));
        assert_eq!(flood_remaining(&flood(Some(60)), 99), Some(0));
        assert_eq!(flood_remaining(&flood(None), 5), None);
        assert_eq!(
            auth_error_line(&flood(Some(60)), 18),
            "Too many tries. Please try again in 0:42."
        );
        assert_eq!(
            auth_error_line(&flood(Some(60)), 60),
            "You can try again now."
        );
        assert_eq!(
            auth_error_line(&flood(None), 0),
            "Too many tries. Please try again later."
        );
        let banned = AuthRequestError {
            purpose: RequestPurpose::SetPhoneNumber,
            class: ErrorClass::PhoneBanned,
            flood_wait_secs: None,
        };
        assert_eq!(flood_remaining(&banned, 0), None);
        assert_eq!(auth_error_line(&banned, 0), "This phone number is banned.");
        let invalid = AuthRequestError {
            class: ErrorClass::PhoneInvalid,
            ..banned
        };
        assert_eq!(
            auth_error_line(&invalid, 0),
            "Invalid phone number. Please try again."
        );
    }

    #[test]
    fn account_limit_matches_tdesktop() {
        assert_eq!(max_accounts(0), 3);
        assert_eq!(max_accounts(1), 4);
        assert_eq!(max_accounts(3), 6);
        assert_eq!(max_accounts(9), 6);
        assert_eq!(add_account_blocker(2, 0), None);
        let blocked = add_account_blocker(3, 0).unwrap();
        assert!(blocked.contains("up to 3") && blocked.contains("Premium"));
        assert_eq!(add_account_blocker(3, 1), None);
        let full = add_account_blocker(6, 6).unwrap();
        assert!(full.contains("up to 6") && !full.contains("Premium"));
    }

    fn typed(kind: CodeKind, call_number: &str, missed: Option<i32>, url: &str) -> CodeDelivery {
        CodeDelivery {
            kind,
            detail: CodeDetail {
                call_number: call_number.into(),
                missed_digits: missed,
                url: url.into(),
            },
            ..delivery(None, 0)
        }
    }

    #[test]
    fn flash_call_names_the_pattern_and_the_whole_number() {
        let flash = typed(CodeKind::FlashCall, "+1555*****", None, "");
        let text = code_step_body(&flash, None, "+1 555 010 0199");
        assert!(text.starts_with("Code for +1 555 010 0199. "));
        assert!(text.contains("like +1555*****"));
        assert!(text.contains("full number that called"));
        assert!(!text.contains("digits."));
    }

    #[test]
    fn missed_call_counts_digits_without_repeating_the_length() {
        let missed = typed(CodeKind::MissedCall, "+1555", Some(6), "");
        let text = code_step_body(&missed, Some(6), "");
        assert!(text.contains("starting with +1555"));
        assert!(text.contains("last 6 digits of that number"));
        assert!(!text.contains("It has"));
        let one = typed(CodeKind::MissedCall, "", Some(1), "");
        assert!(delivery_description(&one, "").contains("last 1 digit of the number"));
    }

    #[test]
    fn fragment_matches_tdesktop_and_opens_only_https() {
        let fragment = typed(CodeKind::Fragment, "", None, "https://fragment.com/login");
        assert_eq!(
            delivery_description(&fragment, "+1 555 010 0199"),
            "Get the code for +1 555 010 0199 in the Anonymous Numbers section on Fragment."
        );
        // Fragment names the number once, not twice.
        assert!(!code_step_body(&fragment, Some(5), "+1 555 010 0199").contains("Code for"));
        assert_eq!(fragment_url(&fragment), Some("https://fragment.com/login"));
        assert_eq!(code_step_title(CodeKind::Fragment), "Enter code");
        assert_eq!(fragment_url(&typed(CodeKind::Fragment, "", None, "")), None);
        assert_eq!(
            fragment_url(&typed(CodeKind::Sms, "", None, "https://x.y")),
            None
        );
    }

    #[test]
    fn firebase_is_honest_about_needing_an_official_app() {
        let text = delivery_description(&typed(CodeKind::Firebase, "", None, ""), "");
        assert!(text.contains("official Android and iOS apps"));
        assert!(text.contains("QR"));
    }

    #[test]
    fn call_resend_labels_cover_every_call_kind() {
        for next in [CodeKind::FlashCall, CodeKind::MissedCall] {
            let wait = resend_option(&delivery(Some(next), 90), 30).unwrap();
            assert_eq!(wait.label, "Telegram will call you in 1:00");
            assert!(!wait.ready);
            assert_eq!(
                resend_option(&delivery(Some(next), 90), 90).unwrap().label,
                "Call me"
            );
        }
        assert_eq!(
            resend_option(&delivery(Some(CodeKind::Fragment), 0), 0)
                .unwrap()
                .label,
            "Get the code from Fragment"
        );
    }

    #[test]
    fn premium_explainer_states_the_grant_and_never_offers_a_purchase() {
        let with_days = premium_explainer(365);
        assert!(with_days.contains("includes 365 days"));
        assert!(with_days.contains("Quill can't take store payments"));
        assert!(with_days.contains("QR code"));
        assert!(!premium_explainer(0).contains("includes"));
    }

    #[test]
    fn premium_support_mailto_accepts_only_plain_mailboxes() {
        assert_eq!(
            premium_support_mailto("support@example.com", "Premium sign-in").as_deref(),
            Some("mailto:support@example.com?subject=Premium%20sign-in")
        );
        assert_eq!(
            premium_support_mailto("support@example.com", "").as_deref(),
            Some("mailto:support@example.com")
        );
        for bad in [
            "",
            "no-at",
            "a@b",
            "a b@example.com",
            "a@example.com?bcc=x",
            "a@@b.c",
            "<a@b.c>",
        ] {
            assert_eq!(premium_support_mailto(bad, "s"), None, "{bad}");
        }
    }
}
