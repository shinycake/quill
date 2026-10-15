//! The parts of the Windows WebAuthn backend (`windows`) that never touch
//! `webauthn.dll`: which structure versions the installed API takes, the
//! options Telegram Desktop passes (`platform/win/webauthn_win.cpp`), how
//! an `HRESULT` maps to a [`CeremonyError`], and the string and buffer
//! conversions. Plain Rust on every OS, so the tests run on macOS and
//! Linux too; the values match `windows-sys`'s `WindowsWebServices`
//! constants, checked at compile time in `windows`.
#![deny(clippy::indexing_slicing)]

use super::{CeremonyError, UserVerification};

/// `WEBAUTHN_API_VERSION_1`: the first `webauthn.dll` (Windows 10 1903).
/// Every call and structure Quill uses exists from here on.
pub const API_VERSION_1: u32 = 1;
/// `WEBAUTHN_API_VERSION_3`: `WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS`
/// version 4, the first with `bPreferResidentKey`.
pub const API_VERSION_3: u32 = 3;

/// `NTE_USER_CANCELLED`: the user closed the Windows Security dialog.
pub const NTE_USER_CANCELLED: i32 = 0x8009_0036_u32 as i32;
/// `HRESULT_FROM_WIN32(ERROR_CANCELLED)`: the operation was cancelled
/// (also what `WebAuthNCancelCurrentOperation` ends the call with).
pub const HRESULT_ERROR_CANCELLED: i32 = 0x8007_04C7_u32 as i32;

/// `WEBAUTHN_USER_VERIFICATION_REQUIREMENT_*`.
pub const UV_REQUIRED: u32 = 1;
pub const UV_PREFERRED: u32 = 2;
pub const UV_DISCOURAGED: u32 = 3;
/// `WEBAUTHN_AUTHENTICATOR_ATTACHMENT_ANY`: Windows Hello, a security key
/// or a phone, whichever the user picks in the system dialog.
pub const ATTACHMENT_ANY: u32 = 0;
/// `WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_NONE`.
pub const ATTESTATION_NONE: u32 = 1;

/// The structure versions to send to the installed `webauthn.dll`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Versions {
    /// `WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS.dwVersion`.
    pub make_credential_options: u32,
    /// `WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS.dwVersion`.
    pub get_assertion_options: u32,
    /// Whether `bPreferResidentKey` can be set.
    pub prefer_resident_key: bool,
}

/// What the API version reported by `WebAuthNGetApiVersionNumber` takes,
/// or `None` below version 1. tdesktop sets every `dwVersion` to the
/// `_CURRENT_VERSION` of the SDK it was built with and sets
/// `bPreferResidentKey` when that SDK has it; Quill picks the newest
/// version holding the fields tdesktop sets that the installed DLL also
/// knows, so an older Windows gets the same request minus the fields it
/// cannot read.
pub fn versions(api_version: u32) -> Option<Versions> {
    if api_version < API_VERSION_1 {
        return None;
    }
    let prefer = api_version >= API_VERSION_3;
    Some(Versions {
        // v3 (API 1) has `pCancellationId`; v4 (API 3) adds
        // `bPreferResidentKey`.
        make_credential_options: if prefer { 4 } else { 3 },
        // v4 (API 1) has `pCancellationId`; nothing newer is set.
        get_assertion_options: 4,
        prefer_resident_key: prefer,
    })
}

/// tdesktop `NativeCancelled`.
pub fn is_cancelled(hr: i32) -> bool {
    hr == NTE_USER_CANCELLED || hr == HRESULT_ERROR_CANCELLED
}

/// A failed call: cancelled, or anything else with its code for the log
/// (tdesktop `Error::Cancelled` / `Error::Other`).
pub fn failure(call: &str, hr: i32) -> CeremonyError {
    if is_cancelled(hr) {
        CeremonyError::Cancelled
    } else {
        CeremonyError::Other(format!("{call} failed, hr 0x{:08x}", hr as u32))
    }
}

/// `dwUserVerificationRequirement` for a sign-in (tdesktop
/// `LoginViaNative`): `required` and `preferred` as asked, anything else
/// discouraged. A registration always asks for `PREFERRED`.
pub fn login_user_verification(requirement: &UserVerification) -> u32 {
    match requirement {
        UserVerification::Required => UV_REQUIRED,
        UserVerification::Preferred => UV_PREFERRED,
        UserVerification::Discouraged | UserVerification::Unspecified => UV_DISCOURAGED,
    }
}

/// A NUL-terminated UTF-16 string for a `PCWSTR`. An embedded NUL would
/// cut the string short on the Windows side, so it is dropped.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16()
        .filter(|unit| *unit != 0)
        .chain(std::iter::once(0))
        .collect()
}

/// `pubKeyCredParams[].alg` as `lAlg`, in the server's order (tdesktop
/// passes every parameter). The parser only keeps values in `i32`, so
/// nothing is dropped in practice.
pub fn cose_algorithms(algorithms: &[i64]) -> Vec<i32> {
    algorithms
        .iter()
        .filter_map(|alg| i32::try_from(*alg).ok())
        .collect()
}

/// A buffer length as the `DWORD` the API takes.
pub fn length(len: usize) -> Result<u32, CeremonyError> {
    u32::try_from(len).map_err(|_| CeremonyError::Other("buffer too large".into()))
}

/// Copy a buffer the API returned (`cb*` bytes at `pb*`) into Rust memory
/// before the structure holding it is freed. Null or empty is empty.
///
/// # Safety
///
/// When `ptr` is not null it must point to `len` readable bytes that stay
/// valid for the duration of the call.
pub unsafe fn copy_out(ptr: *const u8, len: u32) -> Vec<u8> {
    let Ok(len) = usize::try_from(len) else {
        return Vec::new();
    };
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: the caller guarantees `len` readable bytes at the non-null
    // `ptr`; the slice is copied out at once and not kept.
    unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
}

#[cfg(test)]
#[path = "windows_plan_tests.rs"]
mod tests;
