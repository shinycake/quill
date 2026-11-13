//! The links a widget may open. The sandbox already checks every
//! `openLink` and `openTelegramLink` (an `https://` URL of at most 256
//! characters whose host is, or is not, Telegram's), and Quill checks the
//! URL again before it shows the confirm box or opens anything, so a fault
//! in the sandbox can't hand the OS another scheme or a disguised host.

/// The longest link a widget may open, in bytes (the sandbox counts
/// UTF-16 units against 256 and percent-encodes what is not ASCII, so a
/// link it lets through is never longer than this).
pub const MAX_LINK_BYTES: usize = 256 * 9;

/// The hosts `openTelegramLink` is for, and `openLink` is not.
const TELEGRAM_HOSTS: [&str; 3] = ["t.me", "telegram.me", "telegram.dog"];

/// Which method asked for the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// `openLink`: a web page, opened in the browser after a confirm.
    Web,
    /// `openTelegramLink`: a t.me link, opened inside Quill.
    Telegram,
}

/// `url` when a widget may open it as `kind`, otherwise `None`.
pub fn checked(kind: LinkKind, url: &str) -> Option<&str> {
    if url.len() > MAX_LINK_BYTES {
        return None;
    }
    let scheme = url.get(..8)?;
    if !scheme.eq_ignore_ascii_case("https://") {
        return None;
    }
    let rest = url.get(8..)?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..end)?;
    let tail = rest.get(end..)?;
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if !valid_host(host) || !port.is_none_or(valid_port) || !tail.bytes().all(url_byte) {
        return None;
    }
    let telegram = is_telegram(&host.to_ascii_lowercase());
    (telegram == (kind == LinkKind::Telegram)).then_some(url)
}

/// DNS labels of letters, digits and inner hyphens, 253 bytes at most.
/// This rejects userinfo (`@`), IP literals in brackets and anything
/// non-ASCII the sandbox would have encoded.
fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn valid_port(port: &str) -> bool {
    !port.is_empty()
        && port.len() <= 5
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u32>().is_ok_and(|n| n <= 65535)
}

/// A byte that may stand after the authority: printable ASCII other than
/// space, quotes, angle brackets, backslash and the like (RFC 3986's
/// unreserved and reserved characters, and `%`).
fn url_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'()*+,;=%".contains(&b)
}

fn is_telegram(host: &str) -> bool {
    TELEGRAM_HOSTS.iter().any(|telegram| {
        host == *telegram
            || host
                .strip_suffix(telegram)
                .is_some_and(|sub| sub.ends_with('.'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_links_must_be_https_to_a_plain_host() {
        assert_eq!(
            checked(LinkKind::Web, "https://example.com/a?b=1#c"),
            Some("https://example.com/a?b=1#c")
        );
        assert!(checked(LinkKind::Web, "HTTPS://Example.com:8443").is_some());
        for url in [
            "http://example.com",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "tg://resolve?domain=x",
            "https://",
            "https://user@example.com",
            "https://exa_mple.com",
            "https://-example.com",
            "https://example..com",
            "https://[::1]/",
            "https://example.com:99999",
            "https://example.com:",
            "https://example.com/a b",
            "https://example.com/\"x\"",
            "https://example.com/\u{202e}",
            "https://ex\u{0430}mple.com",
        ] {
            assert_eq!(checked(LinkKind::Web, url), None, "{url}");
        }
        let long = format!("https://example.com/{}", "a".repeat(MAX_LINK_BYTES));
        assert_eq!(checked(LinkKind::Web, &long), None);
    }

    #[test]
    fn telegram_hosts_open_only_as_telegram_links() {
        for url in [
            "https://t.me/durov",
            "https://web.telegram.me/x",
            "https://TELEGRAM.DOG",
        ] {
            assert_eq!(checked(LinkKind::Telegram, url), Some(url));
            assert_eq!(checked(LinkKind::Web, url), None, "{url}");
        }
        for url in [
            "https://not-t.me/x",
            "https://t.me.example.com/",
            "https://example.com",
        ] {
            assert_eq!(checked(LinkKind::Telegram, url), None, "{url}");
        }
    }
}
