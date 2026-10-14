//! The OS readers behind [`super::detect`], one per platform.

use crate::proxy::ProxyDraft;

#[cfg(target_os = "macos")]
pub(super) fn detect() -> Option<ProxyDraft> {
    let output = std::process::Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    super::parse_scutil_proxy(&String::from_utf8_lossy(&output.stdout))
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
            let output = std::process::Command::new("gsettings")
                .args(["get", schema, key])
                .output()
                .ok()?;
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        })
    })
}

/// Other targets have no system proxy Quill can read: connect directly.
#[cfg(not(any(unix, windows)))]
pub(super) fn detect() -> Option<ProxyDraft> {
    None
}
