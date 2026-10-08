//! Validation and hand-off of Telegram links that arrive from outside the
//! window: the OS URL-open callback (macOS `application:openURLs:`), argv of
//! a second launch forwarded over the single-instance socket, and the
//! launch argv itself.
//!
//! Links are untrusted input (any web page can fire a `tg://` URL), so
//! [`sanitize_link`] accepts only Telegram link shapes and never anything
//! that could carry a file path or a command line flag. Accepted links are
//! queued in a process-wide inbox; the UI poll loop drains it and feeds the
//! same `getDeepLinkInfo` pipeline as an in-app click (`ui::deep_links`).

use std::collections::VecDeque;
use std::sync::Mutex;

/// Longest link accepted. Telegram links are short; this bounds queue memory.
const MAX_LINK_BYTES: usize = 2048;
/// Links kept while the UI is busy with another flow.
const MAX_QUEUED: usize = 8;
/// Hosts whose `http(s)` links Telegram treats as its own.
const WEB_HOSTS: [&str; 3] = ["t.me", "telegram.me", "telegram.dog"];

/// Normalizes one external string into a link Quill may act on, or `None`.
///
/// Accepts `tg://…` (and the `tg:…` shorthand, rewritten to `tg://…`) and
/// `http(s)://t.me|telegram.me|telegram.dog/<path>`. Rejects anything with
/// whitespace or control characters, userinfo/port tricks in the host, and
/// anything over [`MAX_LINK_BYTES`].
pub fn sanitize_link(raw: &str) -> Option<String> {
    let link = raw.trim();
    if link.is_empty()
        || link.len() > MAX_LINK_BYTES
        || link.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return None;
    }
    let lower = link.to_ascii_lowercase();
    if lower.starts_with("tg:") {
        let body = &link[3..];
        let body = body.strip_prefix("//").unwrap_or(body);
        // `tg://` alone (or `tg:///`) names nothing to open.
        if body.trim_matches('/').is_empty() {
            return None;
        }
        return Some(format!("tg://{body}"));
    }
    let after_scheme = if lower.starts_with("https://") {
        &link[8..]
    } else if lower.starts_with("http://") {
        &link[7..]
    } else {
        return None;
    };
    let host_end = after_scheme.find(['/', '?', '#'])?;
    let host = &after_scheme[..host_end];
    let path = &after_scheme[host_end..];
    if !WEB_HOSTS.iter().any(|h| host.eq_ignore_ascii_case(h)) {
        return None;
    }
    // Needs something to open: `https://t.me/` alone is the landing page.
    if path.trim_matches('/').is_empty() {
        return None;
    }
    Some(link.to_string())
}

#[derive(Default)]
struct Inbox {
    links: VecDeque<String>,
    activate: bool,
}

static INBOX: Mutex<Inbox> = Mutex::new(Inbox {
    links: VecDeque::new(),
    activate: false,
});

fn inbox() -> std::sync::MutexGuard<'static, Inbox> {
    // A panicked pusher leaves plain data behind; keep using it.
    INBOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Validates and queues an external link, and asks for the window to be
/// raised. Returns whether the link was accepted.
pub fn push_external_link(raw: &str) -> bool {
    let Some(link) = sanitize_link(raw) else {
        return false;
    };
    let mut inbox = inbox();
    inbox.activate = true;
    if inbox.links.len() >= MAX_QUEUED || inbox.links.contains(&link) {
        return true;
    }
    inbox.links.push_back(link);
    true
}

/// Handles the argv of a later launch (running on the single-instance
/// listener thread): queue every valid link and raise the window. A bare
/// `--start-minimized` relaunch (autostart while already running) must not
/// steal focus.
pub fn handle_forwarded_launch(args: &[String]) {
    let mut accepted = false;
    for arg in args {
        accepted |= push_external_link(arg);
    }
    let silent = args.iter().any(|arg| arg == "--start-minimized");
    if !accepted && !silent {
        request_activation();
    }
}

/// Asks the UI to raise the main window (a bare second launch).
pub fn request_activation() {
    inbox().activate = true;
}

/// Consumes the pending "raise the window" request.
pub fn take_activation() -> bool {
    std::mem::take(&mut inbox().activate)
}

/// Next queued link, oldest first.
pub fn pop_link() -> Option<String> {
    inbox().links.pop_front()
}

#[cfg(test)]
mod tests {
    use super::{push_external_link, sanitize_link};

    #[test]
    fn accepts_telegram_shapes() {
        assert_eq!(
            sanitize_link("tg://resolve?domain=durov").as_deref(),
            Some("tg://resolve?domain=durov")
        );
        assert_eq!(
            sanitize_link("TG:resolve?domain=durov").as_deref(),
            Some("tg://resolve?domain=durov")
        );
        assert!(sanitize_link("https://t.me/durov/42").is_some());
        assert!(sanitize_link("http://telegram.me/durov").is_some());
        assert!(sanitize_link("https://TELEGRAM.DOG/+AbCdEf").is_some());
        assert!(sanitize_link("  https://t.me/joinchat/xyz\n").is_some());
    }

    #[test]
    fn rejects_everything_else() {
        for bad in [
            "",
            "tg://",
            "tg:",
            "https://t.me/",
            "https://t.me",
            "https://evil.com/t.me/x",
            "https://t.me.evil.com/x",
            "https://t.me@evil.com/x",
            "https://t.me:8080/x",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "/Users/me/photo.png",
            "notes.txt",
            "--connect-smoke",
            "tg://resolve?domain=a b",
            "tg://resolve\n--flag",
            "ftp://t.me/x",
        ] {
            assert_eq!(sanitize_link(bad), None, "{bad:?}");
        }
        assert_eq!(sanitize_link(&format!("tg://{}", "a".repeat(3000))), None);
    }

    #[test]
    fn inbox_queues_dedupes_and_activates() {
        // The one test that owns the global inbox, to avoid cross-test races.
        while super::pop_link().is_some() {}
        let _ = super::take_activation();
        assert!(!push_external_link("file:///etc/passwd"));
        assert!(!super::take_activation());
        assert!(push_external_link("tg://resolve?domain=a"));
        assert!(push_external_link("tg://resolve?domain=a"));
        assert!(push_external_link("tg://resolve?domain=b"));
        assert!(super::take_activation());
        assert!(!super::take_activation());
        assert_eq!(super::pop_link().as_deref(), Some("tg://resolve?domain=a"));
        assert_eq!(super::pop_link().as_deref(), Some("tg://resolve?domain=b"));
        assert_eq!(super::pop_link(), None);

        // A forwarded launch: links queue; a bare relaunch raises the window;
        // an autostart relaunch stays silent.
        super::handle_forwarded_launch(&["tg://resolve?domain=c".to_string()]);
        assert!(super::take_activation());
        assert_eq!(super::pop_link().as_deref(), Some("tg://resolve?domain=c"));
        super::handle_forwarded_launch(&[]);
        assert!(super::take_activation());
        super::handle_forwarded_launch(&["--start-minimized".to_string()]);
        assert!(!super::take_activation());
    }
}
