//! When opening a message link needs the "Open this link?" box.
//!
//! Telegram Desktop (`core/click_handler_types.cpp`, lib_ui
//! `basic_click_handlers.cpp`) asks before following:
//! - a text link (`textEntityTypeTextUrl`, `HiddenUrlClickHandler`) whose
//!   visible text is not the address, unless the host is one of Telegram's
//!   own (`UrlRequiresConfirmation`; `t.me` short links still ask, since a
//!   hidden one can be a join link);
//! - any link whose domain mixes alphabets (`UrlClickHandler::IsSuspicious`),
//!   the homograph trick behind `аpple.com`; the box then marks the
//!   offending characters.
//!
//! The script test is a coarse port: Qt's `QChar::script` becomes the block
//! table in [`script_of`], which covers the scripts domains realistically
//! use; every other character counts as its own script.

use crate::text::LinkTarget;
use std::ops::Range;

/// What clicking a link should do first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenDecision {
    /// Open straight away.
    Open,
    /// Show the box. `shown` is the address as the box prints it and
    /// `suspicious` the byte ranges of `shown` to highlight (empty when
    /// the address is merely hidden behind other text).
    Confirm {
        url: String,
        shown: String,
        suspicious: Vec<Range<usize>>,
    },
}

/// Decide whether following `link` needs the confirmation box.
pub fn open_decision(link: &LinkTarget) -> OpenDecision {
    let LinkTarget::Url { url, label } = link else {
        return OpenDecision::Open;
    };
    let suspicious = suspicious_ranges(url);
    let hidden = label
        .as_deref()
        .is_some_and(|label| !same_address(label, url))
        && !trusted_host(url);
    if !hidden && suspicious.is_empty() {
        return OpenDecision::Open;
    }
    let (shown, suspicious) = printable(url, suspicious);
    OpenDecision::Confirm {
        url: url.clone(),
        shown,
        suspicious,
    }
}

/// Whether `label` reads as `url` itself (ignoring scheme, `www.` and a
/// trailing slash), so a text link that merely repeats its address is not
/// "hidden".
fn same_address(label: &str, url: &str) -> bool {
    fn bare(text: &str) -> String {
        let text = text.trim().to_lowercase();
        let text = text
            .strip_prefix("https://")
            .or_else(|| text.strip_prefix("http://"))
            .unwrap_or(&text);
        let text = text.strip_prefix("www.").unwrap_or(text);
        text.trim_end_matches('/').to_string()
    }
    bare(label) == bare(url)
}

/// Telegram's own hosts skip the box for a text link, except the `t.me`
/// short-link family.
fn trusted_host(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if under("t.me") || under("telegram.me") || under("telegram.dog") {
        return false;
    }
    [
        "telegram.org",
        "telegra.ph",
        "te.legra.ph",
        "graph.org",
        "fragment.com",
        "telesco.pe",
    ]
    .iter()
    .any(|domain| under(domain))
}

/// Lowercased host of an `http(s)` URL, without credentials, port or a
/// trailing dot.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.trim();
    let rest = match rest.split_once("://") {
        Some((_, rest)) => rest,
        None => rest,
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    };
    let host = host.trim_end_matches('.').to_lowercase();
    (!host.is_empty()).then_some(host)
}

/// The address as the box prints it. Suspicious characters that would not
/// show (zero-width, controls, combining marks) are spelled out as
/// `<U+200B>`, so the person sees what is there; the highlight ranges move
/// with the text.
fn printable(url: &str, suspicious: Vec<Range<usize>>) -> (String, Vec<Range<usize>>) {
    let mut shown = String::new();
    let mut marks: Vec<Range<usize>> = Vec::new();
    let mut open: Option<usize> = None;
    for (at, ch) in url.char_indices() {
        let flagged = suspicious.iter().any(|range| range.contains(&at));
        if flagged && open.is_none() {
            open = Some(shown.len());
        } else if !flagged && let Some(start) = open.take() {
            marks.push(start..shown.len());
        }
        if flagged && !is_visible(ch) {
            shown.push_str(&format!("<U+{:04X}>", ch as u32));
        } else {
            shown.push(ch);
        }
    }
    if let Some(start) = open {
        marks.push(start..shown.len());
    }
    (shown, marks)
}

fn is_visible(ch: char) -> bool {
    !ch.is_control()
        && !ch.is_whitespace()
        && !matches!(
            ch,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{115F}'..='\u{1160}'
                | '\u{17B4}'..='\u{17B5}'
                | '\u{180B}'..='\u{180F}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFA0}'
                | '\u{0300}'..='\u{036F}'
        )
}

/// Byte ranges of `url`'s domain characters that do not belong to the
/// domain's alphabet. Empty for plain ASCII domains and for domains written
/// in one script (`пример.рф`).
pub fn suspicious_ranges(url: &str) -> Vec<Range<usize>> {
    let mut start = 0;
    for prefix in ["https://", "http://", "sftp://", "ftp://"] {
        if url.len() >= prefix.len()
            && url.is_char_boundary(prefix.len())
            && url[..prefix.len()].eq_ignore_ascii_case(prefix)
        {
            start = prefix.len();
            break;
        }
    }
    let rest = &url[start..];
    let end = rest.find(['/', '#', ':', '?']).unwrap_or(rest.len());
    let domain = &rest[..end];
    if domain.is_empty() {
        return Vec::new();
    }
    let latin_tld = domain.rsplit_once('.').is_some_and(|(head, tld)| {
        !head.is_empty() && !tld.is_empty() && tld.chars().all(|c| c.is_ascii_alphabetic())
    });
    if !latin_tld && domain.is_ascii() {
        return Vec::new();
    }
    let script = if latin_tld {
        Script::Latin
    } else {
        let tld = domain.rsplit('.').next().unwrap_or(domain);
        tld.chars()
            .map(script_of)
            .find(|script| *script != Script::Common)
            .unwrap_or(Script::Common)
    };
    let ascii_only = matches!(script, Script::Latin | Script::Common | Script::Other);
    let allowed = |ch: char| {
        ch.is_ascii_digit()
            || ch == '-'
            || if ascii_only {
                ch.is_ascii_alphabetic()
            } else {
                script_of(ch) == script
            }
    };
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut label_start = start;
    for label in domain.split('.') {
        let plain = label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !(plain && !ascii_only) {
            for (at, ch) in label.char_indices() {
                if allowed(ch) {
                    continue;
                }
                let from = label_start + at;
                let to = from + ch.len_utf8();
                match out.last_mut() {
                    Some(last) if last.end == from => last.end = to,
                    _ => out.push(from..to),
                }
            }
        }
        label_start += label.len() + 1;
    }
    out
}

/// Writing systems told apart by the suspicious-domain check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    /// Punctuation, digits and symbols shared by every script.
    Common,
    Cyrillic,
    Greek,
    Armenian,
    Hebrew,
    Arabic,
    Devanagari,
    Bengali,
    Thai,
    Georgian,
    Hangul,
    Hiragana,
    Katakana,
    Han,
    /// Any other character: never equal to a script's letters.
    Other,
}

fn script_of(ch: char) -> Script {
    match ch as u32 {
        0x30..=0x39 | 0x2D | 0x2E | 0x5F => Script::Common,
        0x41..=0x5A | 0x61..=0x7A | 0xAA | 0xBA | 0xC0..=0x24F | 0x1E00..=0x1EFF => Script::Latin,
        0x370..=0x3FF | 0x1F00..=0x1FFF => Script::Greek,
        0x400..=0x52F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Script::Cyrillic,
        0x530..=0x58F => Script::Armenian,
        0x590..=0x5FF => Script::Hebrew,
        0x600..=0x6FF | 0x750..=0x77F | 0x8A0..=0x8FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            Script::Arabic
        }
        0x900..=0x97F => Script::Devanagari,
        0x980..=0x9FF => Script::Bengali,
        0xE00..=0xE7F => Script::Thai,
        0x10A0..=0x10FF => Script::Georgian,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Script::Hangul,
        0x3041..=0x309F => Script::Hiragana,
        0x30A0..=0x30FF => Script::Katakana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0x20000..=0x2FA1F => Script::Han,
        _ => Script::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenDecision, host_of, open_decision, suspicious_ranges};
    use crate::text::LinkTarget;

    fn text_link(url: &str, label: &str) -> LinkTarget {
        LinkTarget::Url {
            url: url.into(),
            label: Some(label.into()),
        }
    }

    fn bare_link(url: &str) -> LinkTarget {
        LinkTarget::Url {
            url: url.into(),
            label: None,
        }
    }

    #[test]
    fn a_bare_ascii_url_opens_without_asking() {
        assert_eq!(
            open_decision(&bare_link("https://example.com/a?b=1")),
            OpenDecision::Open
        );
    }

    #[test]
    fn a_text_link_with_other_text_asks_and_shows_the_address() {
        let decision = open_decision(&text_link("https://evil.example/x", "your bank"));
        assert_eq!(
            decision,
            OpenDecision::Confirm {
                url: "https://evil.example/x".into(),
                shown: "https://evil.example/x".into(),
                suspicious: vec![]
            }
        );
    }

    #[test]
    fn a_text_link_repeating_its_address_does_not_ask() {
        assert_eq!(
            open_decision(&text_link("https://example.com/", "example.com")),
            OpenDecision::Open
        );
        assert_eq!(
            open_decision(&text_link("http://www.example.com", "https://example.com/")),
            OpenDecision::Open
        );
    }

    #[test]
    fn telegram_hosts_skip_the_box_but_short_links_ask() {
        assert_eq!(
            open_decision(&text_link("https://telegram.org/faq", "FAQ")),
            OpenDecision::Open
        );
        assert_eq!(
            open_decision(&text_link("https://fragment.com/x", "buy")),
            OpenDecision::Open
        );
        assert!(matches!(
            open_decision(&text_link("https://t.me/joinchat/abc", "join")),
            OpenDecision::Confirm { .. }
        ));
        assert!(matches!(
            open_decision(&text_link("https://telegram.org.evil.com/", "faq")),
            OpenDecision::Confirm { .. }
        ));
    }

    #[test]
    fn a_mixed_alphabet_domain_asks_and_marks_the_odd_letter() {
        // Cyrillic "а" (U+0430) among Latin letters.
        let url = "https://\u{430}pple.com/login";
        assert_eq!(suspicious_ranges(url), vec![8..10]);
        let OpenDecision::Confirm {
            shown, suspicious, ..
        } = open_decision(&bare_link(url))
        else {
            panic!("a lookalike domain must ask");
        };
        assert_eq!(shown, url);
        assert_eq!(&shown[suspicious[0].clone()], "\u{430}");
    }

    #[test]
    fn one_script_domains_are_not_suspicious() {
        assert!(suspicious_ranges("https://пример.рф/path").is_empty());
        assert!(suspicious_ranges("https://xn--80ak6aa92e.com").is_empty());
        assert!(suspicious_ranges("https://sub.example.co.uk:8080/").is_empty());
        assert!(suspicious_ranges("example.com").is_empty());
    }

    #[test]
    fn a_cyrillic_tld_flags_latin_letters() {
        // Latin "ex" + Cyrillic "а" + Latin "mple" under a Cyrillic TLD.
        assert!(!suspicious_ranges("https://ex\u{430}mple.рф").is_empty());
    }

    #[test]
    fn invisible_suspicious_characters_are_spelled_out() {
        let url = "https://exa\u{200B}mple.com";
        let OpenDecision::Confirm {
            shown, suspicious, ..
        } = open_decision(&bare_link(url))
        else {
            panic!("a zero-width space in a domain must ask");
        };
        assert!(shown.contains("<U+200B>"));
        assert_eq!(&shown[suspicious[0].clone()], "<U+200B>");
    }

    #[test]
    fn the_host_drops_credentials_port_and_trailing_dot() {
        assert_eq!(
            host_of("https://user:pw@Example.COM.:8443/a").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_of("https://").as_deref(), None);
    }

    #[test]
    fn non_web_links_never_ask() {
        assert_eq!(
            open_decision(&LinkTarget::Email("a@b.c".into())),
            OpenDecision::Open
        );
    }
}
