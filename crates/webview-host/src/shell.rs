//! The trusted shell page: served from the `quill` custom scheme, embedded
//! in the binary. The bot page is an `<iframe>` inside it, so the shell's
//! DOM is out of the bot's reach (different origin) and the bot cannot
//! navigate the top frame (sandbox without `allow-top-navigation`).

pub const SCHEME: &str = "quill";

/// The shell's origin as the web view sees it. wry maps custom schemes to
/// `https://<scheme>.<host>/` on Windows (WebView2 has no custom-scheme
/// handler) and keeps `<scheme>://<host>/` elsewhere.
#[cfg(target_os = "windows")]
pub const ORIGIN: &str = "https://quill.localhost/";
#[cfg(not(target_os = "windows"))]
pub const ORIGIN: &str = "quill://localhost/";

#[cfg(target_os = "windows")]
pub const INDEX_URL: &str = "https://quill.localhost/index.html";
#[cfg(not(target_os = "windows"))]
pub const INDEX_URL: &str = "quill://localhost/index.html";

const INDEX_HTML: &str = include_str!("shell/index.html");
const PAGE_CSS: &str = include_str!("shell/page.css");
const PAGE_JS: &str = include_str!("shell/page.js");

/// `(mime, body)` of one shell asset, by path without the leading slash.
pub fn asset(path: &str) -> Option<(&'static str, &'static str)> {
    match path {
        "" | "index.html" => Some(("text/html; charset=utf-8", INDEX_HTML)),
        "page.css" => Some(("text/css; charset=utf-8", PAGE_CSS)),
        "page.js" => Some(("text/javascript; charset=utf-8", PAGE_JS)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_url_is_inside_the_origin() {
        assert!(INDEX_URL.starts_with(ORIGIN));
        assert!(asset("index.html").is_some());
        assert!(asset("").is_some());
        assert!(asset("../Cargo.toml").is_none());
    }

    #[test]
    fn shell_sandboxes_the_frame_and_delegates_no_device_permissions() {
        // The frame is created by the script; the attributes live there.
        assert!(PAGE_JS.contains("allow-scripts allow-same-origin allow-forms allow-popups"));
        assert!(!PAGE_JS.contains("allow-top-navigation"));
        assert!(PAGE_JS.contains("camera 'none'"));
        assert!(PAGE_JS.contains("microphone 'none'"));
        assert!(PAGE_JS.contains("geolocation 'none'"));
        // The shell only ever talks to its own frame and to the host.
        assert!(INDEX_HTML.contains("Content-Security-Policy"));
    }
}
