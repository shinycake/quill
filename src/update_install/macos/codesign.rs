//! Code-signature checks on a staged bundle, through the Security framework
//! (what `codesign --verify --deep --strict` does, in process).
//!
//! The staged bundle must carry a valid signature under strict validation,
//! nested code included, for the running copy's signing identifier; both
//! `Info.plist` files must name the same bundle identifier, and the staged
//! one the release's version. Then the identity: a running copy signed with
//! a certificate (a Developer ID, or the owner's Apple Development identity)
//! has a designated requirement that names its team or leaf certificate, and
//! the staged bundle must satisfy it. The keychain approval for the TDLib
//! database key is bound to that requirement, so a bundle signed by anyone
//! else never installs and the approval survives the update. An ad-hoc
//! signature has no identity to pin (its requirement is its own cdhash), so
//! the release SHA-256 fetched over HTTPS from GitHub stays the trust anchor,
//! plus the strict check here.
//!
//! `judge` is the whole policy, over facts gathered once per bundle.
#![deny(clippy::indexing_slicing)]

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use core_foundation_sys::base::OSStatus;
use core_foundation_sys::dictionary::CFDictionaryRef;
use security_framework::os::macos::code_signing::{Flags, SecRequirement, SecStaticCode};
use security_framework_sys::code_signing::{
    SecCSFlags, SecRequirementRef, SecStaticCodeCheckValidity, SecStaticCodeRef,
    kSecCSBasicValidateOnly, kSecCSCheckAllArchitectures, kSecCSCheckNestedCode,
    kSecCSNoNetworkAccess, kSecCSStrictValidate,
};
use std::path::Path;

/// `kSecCSSigningInformation`: include the certificates and the team in a
/// `SecCodeCopySigningInformation` answer.
const SIGNING_INFORMATION: SecCSFlags = 1 << 1;

/// `codesign --verify --deep --strict`, every architecture, and no
/// certificate fetch over the network.
const STRICT: SecCSFlags = kSecCSStrictValidate
    | kSecCSCheckNestedCode
    | kSecCSCheckAllArchitectures
    | kSecCSNoNetworkAccess;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecCodeCopySigningInformation(
        code: SecStaticCodeRef,
        flags: SecCSFlags,
        information: *mut CFDictionaryRef,
    ) -> OSStatus;
    fn SecCodeCopyDesignatedRequirement(
        code: SecStaticCodeRef,
        flags: SecCSFlags,
        requirement: *mut SecRequirementRef,
    ) -> OSStatus;
}

/// Why a staged bundle is not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Rejection {
    /// Signed by someone else than the running copy, or the running copy is
    /// not signed at all: this copy of Quill updates by hand.
    Mismatch(String),
    /// A broken or missing signature, another bundle identifier, another
    /// version than the release's.
    Invalid(String),
}

/// How the running copy is signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Signer {
    AdHoc,
    Certificate,
}

/// What was found out about the running copy and the staged bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Facts {
    /// The running copy's signature; `None` when it has none.
    pub running: Option<Signer>,
    /// The staged bundle passed the strict check with the running copy's
    /// signing identifier.
    pub staged_valid: bool,
    /// Both `Info.plist` files name the same `CFBundleIdentifier`.
    pub same_bundle_id: bool,
    /// The staged `CFBundleShortVersionString` is the release version.
    pub version_matches: bool,
    /// The staged bundle satisfies the running copy's designated
    /// requirement; asked only of a certificate-signed copy.
    pub satisfies_requirement: Option<bool>,
}

pub(super) fn judge(facts: &Facts) -> Result<(), Rejection> {
    let Some(running) = facts.running else {
        return Err(Rejection::Mismatch(
            "the running copy of Quill is not signed".into(),
        ));
    };
    if !facts.staged_valid {
        return Err(Rejection::Invalid(
            "the staged bundle failed the strict signature check".into(),
        ));
    }
    if !facts.same_bundle_id {
        return Err(Rejection::Invalid(
            "the staged bundle has another bundle identifier".into(),
        ));
    }
    if !facts.version_matches {
        return Err(Rejection::Invalid(
            "the staged bundle is not the release version".into(),
        ));
    }
    match (running, facts.satisfies_requirement) {
        (Signer::AdHoc, _) | (Signer::Certificate, Some(true)) => Ok(()),
        (Signer::Certificate, _) => Err(Rejection::Mismatch(
            "the staged bundle does not satisfy the running copy's designated requirement".into(),
        )),
    }
}

/// Whether the bundle at `staged` may replace the one at `running` for
/// release `version`.
pub(crate) fn verify(running: &Path, staged: &Path, version: &str) -> Result<(), Rejection> {
    judge(&gather(running, staged, version).map_err(Rejection::Invalid)?)
}

fn gather(running: &Path, staged: &Path, version: &str) -> Result<Facts, String> {
    let running_code = static_code(running)?;
    let running_info = SigningInfo::of(&running_code)?;
    let running_signer = signer(&running_code, &running_info);
    let staged_code = static_code(staged)?;
    let requirement = running_info
        .string("identifier")
        .and_then(|identifier| identifier_requirement(&identifier));
    let staged_valid = requirement
        .as_ref()
        .is_some_and(|requirement| check(&staged_code, STRICT, Some(requirement)).is_ok());
    let staged_info = SigningInfo::of(&staged_code)?;
    let running_id = running_info.plist_string("CFBundleIdentifier");
    let satisfies_requirement = match running_signer {
        Some(Signer::Certificate) => Some(
            designated_requirement(&running_code)
                .and_then(|requirement| check(&staged_code, STRICT, Some(&requirement)))
                .is_ok(),
        ),
        _ => None,
    };
    Ok(Facts {
        running: running_signer,
        staged_valid,
        same_bundle_id: running_id.is_some()
            && running_id == staged_info.plist_string("CFBundleIdentifier"),
        version_matches: staged_info
            .plist_string("CFBundleShortVersionString")
            .as_deref()
            == Some(version),
        satisfies_requirement,
    })
}

/// `None` when the code has no valid signature at all.
fn signer(code: &SecStaticCode, info: &SigningInfo) -> Option<Signer> {
    check(code, kSecCSBasicValidateOnly, None).ok()?;
    Some(if info.certificate_count() > 0 {
        Signer::Certificate
    } else {
        Signer::AdHoc
    })
}

fn status_result(status: OSStatus) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(security_framework::base::Error::from_code(status).to_string())
    }
}

fn static_code(path: &Path) -> Result<SecStaticCode, String> {
    let url = CFURL::from_path(path, true).ok_or_else(|| "invalid bundle path".to_string())?;
    SecStaticCode::from_path(&url, Flags::NONE).map_err(|error| error.to_string())
}

/// `identifier "<id>"` for a plain identifier.
fn identifier_requirement(identifier: &str) -> Option<SecRequirement> {
    if identifier.is_empty() || identifier.contains(['"', '\\']) {
        return None;
    }
    format!("identifier \"{identifier}\"").parse().ok()
}

/// `SecStaticCodeCheckValidity`, with or without a requirement.
fn check(
    code: &SecStaticCode,
    flags: SecCSFlags,
    requirement: Option<&SecRequirement>,
) -> Result<(), String> {
    let requirement = requirement.map_or(std::ptr::null_mut(), TCFType::as_concrete_TypeRef);
    // SAFETY: `code` is a live static-code object, `requirement` is null or a
    // live requirement object; both outlive the call, which only reads them.
    let status =
        unsafe { SecStaticCodeCheckValidity(code.as_concrete_TypeRef(), flags, requirement) };
    status_result(status)
}

fn designated_requirement(code: &SecStaticCode) -> Result<SecRequirement, String> {
    let mut requirement: SecRequirementRef = std::ptr::null_mut();
    // SAFETY: `code` is a live static-code object; on success the out
    // pointer holds a requirement object this process owns one reference to.
    let status = unsafe {
        SecCodeCopyDesignatedRequirement(code.as_concrete_TypeRef(), 0, &mut requirement)
    };
    status_result(status)?;
    if requirement.is_null() {
        return Err("no designated requirement".into());
    }
    // SAFETY: the create rule applies to the copied requirement, and this is
    // the only wrapper that takes ownership of it.
    Ok(unsafe { SecRequirement::wrap_under_create_rule(requirement) })
}

/// A `SecCodeCopySigningInformation` answer.
struct SigningInfo(CFDictionary);

impl SigningInfo {
    fn of(code: &SecStaticCode) -> Result<Self, String> {
        let mut dictionary: CFDictionaryRef = std::ptr::null();
        // SAFETY: `code` is a live static-code object; on success the out
        // pointer holds a dictionary this process owns one reference to.
        let status = unsafe {
            SecCodeCopySigningInformation(
                code.as_concrete_TypeRef(),
                SIGNING_INFORMATION,
                &mut dictionary,
            )
        };
        status_result(status)?;
        if dictionary.is_null() {
            return Err("no signing information".into());
        }
        // SAFETY: the create rule applies to the copied dictionary, and this
        // is the only wrapper that takes ownership of it.
        Ok(Self(unsafe {
            CFDictionary::wrap_under_create_rule(dictionary)
        }))
    }

    fn value_in(dictionary: &CFDictionary, key: &str) -> Option<CFType> {
        let key = CFString::new(key);
        let item = dictionary.find(key.as_CFTypeRef())?;
        if item.is_null() {
            return None;
        }
        // SAFETY: the dictionary keeps the value alive while it lives; the
        // get rule retains it for the wrapper's own lifetime.
        Some(unsafe { CFType::wrap_under_get_rule(*item) })
    }

    fn string(&self, key: &str) -> Option<String> {
        Self::value_in(&self.0, key)?
            .downcast::<CFString>()
            .map(|string| string.to_string())
    }

    /// The certificate chain's length (0 for an ad-hoc signature).
    fn certificate_count(&self) -> usize {
        Self::value_in(&self.0, "certificates")
            .and_then(|value| value.downcast::<CFArray>())
            .map_or(0, |array| usize::try_from(array.len()).unwrap_or(0))
    }

    /// A string from the bundle's `Info.plist`.
    fn plist_string(&self, key: &str) -> Option<String> {
        let plist = Self::value_in(&self.0, "info-plist")?.downcast::<CFDictionary>()?;
        Self::value_in(&plist, key)?
            .downcast::<CFString>()
            .map(|string| string.to_string())
    }
}
