//! The OS readers behind [`super::detect`], one per platform.

use crate::proxy::ProxyDraft;

#[cfg(target_os = "macos")]
pub(super) fn detect() -> Option<ProxyDraft> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    #[link(name = "SystemConfiguration", kind = "framework")]
    unsafe extern "C" {
        fn SCDynamicStoreCopyProxies(store: *const std::ffi::c_void) -> CFDictionaryRef;
    }
    // SAFETY: a null store is allowed; the call uses a session of its own.
    let proxies = unsafe { SCDynamicStoreCopyProxies(std::ptr::null()) };
    if proxies.is_null() {
        return None;
    }
    // SAFETY: a non-null dictionary from a Copy call, owned from here on.
    let proxies: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_create_rule(proxies) };
    super::proxy_from_dictionary(|key| {
        let value = proxies.find(CFString::new(key))?;
        value.downcast::<CFString>().map_or_else(
            || {
                let number = value.downcast::<CFNumber>()?;
                Some(number.to_i64()?.to_string())
            },
            |text| Some(text.to_string()),
        )
    })
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
