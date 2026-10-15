//! The Windows backend's pure parts, run on every OS.
use super::*;

#[test]
fn versions_follow_the_installed_api() {
    assert_eq!(versions(0), None);
    assert_eq!(
        versions(1),
        Some(Versions {
            make_credential_options: 3,
            get_assertion_options: 4,
            prefer_resident_key: false,
        })
    );
    assert_eq!(versions(2).map(|v| v.prefer_resident_key), Some(false));
    let modern = versions(7).unwrap();
    assert_eq!(modern.make_credential_options, 4);
    assert_eq!(modern.get_assertion_options, 4);
    assert!(modern.prefer_resident_key);
    assert!(versions(3).unwrap().prefer_resident_key);
}

#[test]
fn cancellation_codes_map_to_cancelled() {
    assert!(is_cancelled(NTE_USER_CANCELLED));
    assert!(is_cancelled(HRESULT_ERROR_CANCELLED));
    assert_eq!(
        failure("MakeCredential", NTE_USER_CANCELLED),
        CeremonyError::Cancelled
    );
    assert_eq!(
        failure("GetAssertion", HRESULT_ERROR_CANCELLED),
        CeremonyError::Cancelled
    );
    // NTE_NOT_FOUND, E_INVALIDARG: anything else is "other", with the code.
    assert_eq!(
        failure("GetAssertion", 0x8009_0011_u32 as i32),
        CeremonyError::Other("GetAssertion failed, hr 0x80090011".into())
    );
    assert_eq!(
        failure("MakeCredential", 0x8007_0057_u32 as i32),
        CeremonyError::Other("MakeCredential failed, hr 0x80070057".into())
    );
    // A success code without a result is still a failure, never cancelled.
    assert!(matches!(
        failure("MakeCredential", 0),
        CeremonyError::Other(_)
    ));
}

#[test]
fn sign_in_verification_matches_tdesktop() {
    assert_eq!(
        login_user_verification(&UserVerification::Required),
        UV_REQUIRED
    );
    assert_eq!(
        login_user_verification(&UserVerification::Preferred),
        UV_PREFERRED
    );
    assert_eq!(
        login_user_verification(&UserVerification::Discouraged),
        UV_DISCOURAGED
    );
    assert_eq!(
        login_user_verification(&UserVerification::Unspecified),
        UV_DISCOURAGED
    );
}

#[test]
fn wide_strings_are_nul_terminated_utf16() {
    assert_eq!(wide("ab"), vec![0x61, 0x62, 0]);
    assert_eq!(wide(""), vec![0]);
    assert_eq!(wide("a\0b"), vec![0x61, 0x62, 0]);
    // Outside the BMP: a surrogate pair.
    assert_eq!(wide("\u{1F511}"), vec![0xD83D, 0xDD11, 0]);
}

#[test]
fn algorithms_keep_the_servers_order() {
    assert_eq!(cose_algorithms(&[-7, -257, -8]), vec![-7, -257, -8]);
    assert_eq!(cose_algorithms(&[i64::MAX, -7]), vec![-7]);
    assert!(cose_algorithms(&[]).is_empty());
}

#[test]
fn lengths_and_copies_are_checked() {
    assert_eq!(length(3), Ok(3));
    let bytes = [1_u8, 2, 3];
    // SAFETY: three readable bytes.
    assert_eq!(unsafe { copy_out(bytes.as_ptr(), 3) }, vec![1, 2, 3]);
    // SAFETY: null is allowed.
    assert!(unsafe { copy_out(std::ptr::null(), 5) }.is_empty());
    // SAFETY: zero bytes are read.
    assert!(unsafe { copy_out(bytes.as_ptr(), 0) }.is_empty());
}
