//! The OS readers behind [`super::detect`], one per platform.

use crate::proxy::ProxyDraft;

#[cfg(target_os = "macos")]
pub(super) fn detect() -> Option<ProxyDraft> {
    let text = crate::child_timeout::stdout_within(
        "/usr/sbin/scutil",
        &["--proxy"],
        crate::child_timeout::DESKTOP_QUERY_LIMIT,
    )?;
    super::parse_scutil_proxy(&text)
}

#[cfg(windows)]
pub(super) fn detect() -> Option<ProxyDraft> {
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    if crate::winreg::get_dword(KEY, "ProxyEnable").ok().flatten() != Some(1) {
        return None;
    }
    let server = crate::winreg::get_string(KEY, "ProxyServer")
        .ok()
        .flatten()?;
    super::parse_windows_proxy_server(&server)
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn detect() -> Option<ProxyDraft> {
    super::proxy_from_env(|name| std::env::var(name).ok()).or_else(|| {
        super::proxy_from_gsettings(|schema, key| {
            crate::child_timeout::stdout_within(
                "gsettings",
                &["get", schema, key],
                crate::child_timeout::PROBE_LIMIT,
            )
        })
    })
}

/// Other targets have no system proxy Quill can read: connect directly.
#[cfg(not(any(unix, windows)))]
pub(super) fn detect() -> Option<ProxyDraft> {
    None
}
