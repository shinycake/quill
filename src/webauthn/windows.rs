//! Windows: the ceremony through `webauthn.dll`, Telegram Desktop's
//! `platform/win/webauthn_win.cpp` (`RegisterViaNative`,
//! `LoginViaNative`). The system's Windows Security dialog does the rest:
//! Windows Hello, a security key or a phone, whichever the user picks.
//!
//! The DLL is loaded at run time (tdesktop's `SafeLoadLibrary` and
//! `LOAD_SYMBOL`), from System32 only, so Quill starts on a Windows
//! without it and the seam (`super::supported`) is simply off there. The
//! blocking calls run on the ceremony's worker thread, with Quill's window
//! as the dialog's parent; Cancel in Quill's dialog ends a call through
//! `WebAuthNCancelCurrentOperation`. The pure parts (versions, options,
//! error mapping) are `windows_plan`, tested on every OS.
#![deny(clippy::indexing_slicing)]

use super::ceremony::{Ceremony, CeremonyEvent};
use super::windows_plan::{
    self, ATTACHMENT_ANY, ATTESTATION_NONE, UV_PREFERRED, Versions, copy_out, failure, length, wide,
};
use super::{
    Assertion, CeremonyError, CreationOptions, ParentWindow, Registration, RequestOptions,
};
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex, OnceLock};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Networking::WindowsWebServices as api;
use windows_sys::Win32::Networking::WindowsWebServices::{
    WEBAUTHN_ASSERTION, WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS,
    WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS, WEBAUTHN_CLIENT_DATA,
    WEBAUTHN_COSE_CREDENTIAL_PARAMETER, WEBAUTHN_COSE_CREDENTIAL_PARAMETERS, WEBAUTHN_CREDENTIAL,
    WEBAUTHN_CREDENTIAL_ATTESTATION, WEBAUTHN_CREDENTIALS, WEBAUTHN_RP_ENTITY_INFORMATION,
    WEBAUTHN_USER_ENTITY_INFORMATION,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
use windows_sys::core::{BOOL, GUID, HRESULT, PCWSTR};

// The plan's copies of the SDK constants are the SDK's.
const _: () = {
    assert!(windows_plan::NTE_USER_CANCELLED == windows_sys::Win32::Foundation::NTE_USER_CANCELLED);
    assert!(windows_plan::UV_REQUIRED == api::WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED);
    assert!(windows_plan::UV_PREFERRED == api::WEBAUTHN_USER_VERIFICATION_REQUIREMENT_PREFERRED);
    assert!(
        windows_plan::UV_DISCOURAGED == api::WEBAUTHN_USER_VERIFICATION_REQUIREMENT_DISCOURAGED
    );
    assert!(ATTACHMENT_ANY == api::WEBAUTHN_AUTHENTICATOR_ATTACHMENT_ANY);
    assert!(ATTESTATION_NONE == api::WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_NONE);
    assert!(windows_plan::API_VERSION_1 == api::WEBAUTHN_API_VERSION_1);
    assert!(windows_plan::API_VERSION_3 == api::WEBAUTHN_API_VERSION_3);
};

type GetApiVersionNumber = unsafe extern "system" fn() -> u32;
type MakeCredential = unsafe extern "system" fn(
    HWND,
    *const WEBAUTHN_RP_ENTITY_INFORMATION,
    *const WEBAUTHN_USER_ENTITY_INFORMATION,
    *const WEBAUTHN_COSE_CREDENTIAL_PARAMETERS,
    *const WEBAUTHN_CLIENT_DATA,
    *const WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS,
    *mut *mut WEBAUTHN_CREDENTIAL_ATTESTATION,
) -> HRESULT;
type FreeCredentialAttestation = unsafe extern "system" fn(*const WEBAUTHN_CREDENTIAL_ATTESTATION);
type GetAssertion = unsafe extern "system" fn(
    HWND,
    PCWSTR,
    *const WEBAUTHN_CLIENT_DATA,
    *const WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS,
    *mut *mut WEBAUTHN_ASSERTION,
) -> HRESULT;
type FreeAssertion = unsafe extern "system" fn(*const WEBAUTHN_ASSERTION);
type GetCancellationId = unsafe extern "system" fn(*mut GUID) -> HRESULT;
type CancelCurrentOperation = unsafe extern "system" fn(*const GUID) -> HRESULT;

/// The resolved `webauthn.dll`. The module stays loaded for the life of
/// the process, so the function pointers stay valid.
struct Api {
    versions: Versions,
    make_credential: MakeCredential,
    free_attestation: FreeCredentialAttestation,
    get_assertion: GetAssertion,
    free_assertion: FreeAssertion,
    /// Both exist from API version 1; without them Cancel only closes
    /// Quill's dialog and the system dialog runs to its own end.
    cancellation: Option<(GetCancellationId, CancelCurrentOperation)>,
}

static API: OnceLock<Option<Api>> = OnceLock::new();

fn loaded() -> Option<&'static Api> {
    API.get_or_init(resolve).as_ref()
}

/// Whether `webauthn.dll` is there with every function tdesktop loads,
/// and its API version is one Quill can talk to (tdesktop `Supported`).
pub(super) fn available() -> bool {
    loaded().is_some()
}

/// One export of `module`, as the function type `F`.
///
/// # Safety
///
/// `module` is a loaded module and `F` is the export's real signature (a
/// `extern "system"` function pointer type).
unsafe fn symbol<F: Copy>(module: *mut core::ffi::c_void, name: &core::ffi::CStr) -> Option<F> {
    // SAFETY: `module` is loaded (caller), `name` is NUL-terminated.
    let found = unsafe { GetProcAddress(module, name.as_ptr().cast()) }?;
    if std::mem::size_of::<F>() != std::mem::size_of_val(&found) {
        return None;
    }
    // SAFETY: both are function pointers of the same size, and the caller
    // names the export's documented signature (webauthn.h).
    Some(unsafe { std::mem::transmute_copy::<_, F>(&found) })
}

/// tdesktop `Resolve`: the DLL and its functions, or `None`.
fn resolve() -> Option<Api> {
    let name = wide("webauthn.dll");
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the
    // call; System32 only, so no other directory can supply the DLL.
    let module = unsafe { LoadLibraryExW(name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        super::log("webauthn.dll is not available");
        return None;
    }
    // SAFETY: `module` was just loaded and is never freed; each type below
    // is the export's signature in webauthn.h.
    let version: GetApiVersionNumber = unsafe { symbol(module, c"WebAuthNGetApiVersionNumber") }?;
    // tdesktop loads it too and refuses the DLL without it.
    // SAFETY: as above.
    let _: unsafe extern "system" fn(*mut BOOL) -> HRESULT = unsafe {
        symbol(
            module,
            c"WebAuthNIsUserVerifyingPlatformAuthenticatorAvailable",
        )
    }?;
    // SAFETY: as above.
    let make_credential = unsafe { symbol(module, c"WebAuthNAuthenticatorMakeCredential") }?;
    // SAFETY: as above.
    let free_attestation = unsafe { symbol(module, c"WebAuthNFreeCredentialAttestation") }?;
    // SAFETY: as above.
    let get_assertion = unsafe { symbol(module, c"WebAuthNAuthenticatorGetAssertion") }?;
    // SAFETY: as above.
    let free_assertion = unsafe { symbol(module, c"WebAuthNFreeAssertion") }?;
    // SAFETY: as above.
    let get_cancellation: Option<GetCancellationId> =
        unsafe { symbol(module, c"WebAuthNGetCancellationId") };
    // SAFETY: as above.
    let cancel: Option<CancelCurrentOperation> =
        unsafe { symbol(module, c"WebAuthNCancelCurrentOperation") };
    // SAFETY: a plain query without arguments.
    let api_version = unsafe { version() };
    let Some(versions) = windows_plan::versions(api_version) else {
        super::log(&format!(
            "webauthn.dll API version {api_version} is too old"
        ));
        return None;
    };
    Some(Api {
        versions,
        make_credential,
        free_attestation,
        get_assertion,
        free_assertion,
        cancellation: get_cancellation.zip(cancel),
    })
}

/// The cancellation of the call in flight, shared by the worker and
/// Quill's Cancel button.
#[derive(Default)]
struct Cancellation {
    /// Cancel was pressed; a call not started yet never starts.
    cancelled: bool,
    /// The id of the call in flight, when the DLL gave one.
    id: Option<GUID>,
}

type CancelSlot = Arc<Mutex<Cancellation>>;

/// Before a call: `Err(Cancelled)` if Cancel was already pressed, else the
/// call's cancellation id (none when the DLL cannot cancel).
fn begin(api: &Api, slot: &CancelSlot) -> Result<Option<GUID>, CeremonyError> {
    let mut id = None;
    if let Some((get_id, _)) = api.cancellation {
        let mut guid = GUID::from_u128(0);
        // SAFETY: `guid` is a valid out pointer for the call.
        if unsafe { get_id(&mut guid) } >= 0 {
            id = Some(guid);
        }
    }
    let mut state = slot
        .lock()
        .map_err(|_| CeremonyError::Other("cancellation state poisoned".into()))?;
    if state.cancelled {
        return Err(CeremonyError::Cancelled);
    }
    state.id = id;
    Ok(id)
}

/// After a call: nothing left to cancel.
fn end(slot: &CancelSlot) {
    if let Ok(mut state) = slot.lock() {
        state.id = None;
    }
}

/// Quill's Cancel: mark the ceremony cancelled and end the call in flight.
fn cancel(slot: &CancelSlot) {
    let id = match slot.lock() {
        Ok(mut state) => {
            state.cancelled = true;
            state.id.take()
        }
        Err(_) => None,
    };
    if let (Some(id), Some(api)) = (id, loaded())
        && let Some((_, cancel)) = api.cancellation
    {
        // SAFETY: `id` is a valid GUID on this stack; a call that already
        // ended just makes this return an error.
        let _ = unsafe { cancel(&id) };
    }
}

/// tdesktop `NativeParentWindow`: the window comes to the front and takes
/// focus, so the system dialog opens over it. Called on the UI thread,
/// which owns the window.
fn raise(parent: ParentWindow) {
    if parent.0 == 0 {
        return;
    }
    let hwnd = parent.0 as HWND;
    // SAFETY: plain user32 calls on Quill's own live window handle.
    unsafe {
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
    }
}

/// `WEBAUTHN_CLIENT_DATA` over `json` (tdesktop: version current, SHA-256).
fn client_data(json: &mut [u8]) -> Result<WEBAUTHN_CLIENT_DATA, CeremonyError> {
    Ok(WEBAUTHN_CLIENT_DATA {
        dwVersion: api::WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
        cbClientDataJSON: length(json.len())?,
        pbClientDataJSON: json.as_mut_ptr(),
        pwszHashAlgId: api::WEBAUTHN_HASH_ALGORITHM_SHA_256,
    })
}

/// A ceremony that creates a passkey in the system dialog.
pub(super) fn register(options: CreationOptions, parent: ParentWindow) -> Ceremony {
    raise(parent);
    let slot = CancelSlot::default();
    let canceller = slot.clone();
    Ceremony::spawn_system(
        move || {
            let result = loaded()
                .ok_or(CeremonyError::Other("webauthn.dll went away".into()))
                .and_then(|api| make_credential(api, &options, parent, &slot));
            match result {
                Ok(registration) => CeremonyEvent::Registered(registration),
                Err(error) => CeremonyEvent::Failed(error),
            }
        },
        Arc::new(move || cancel(&canceller)),
    )
}

/// A ceremony that signs in with a passkey in the system dialog.
pub(super) fn login(options: RequestOptions, parent: ParentWindow) -> Ceremony {
    raise(parent);
    let slot = CancelSlot::default();
    let canceller = slot.clone();
    Ceremony::spawn_system(
        move || {
            let result = loaded()
                .ok_or(CeremonyError::Other("webauthn.dll went away".into()))
                .and_then(|api| get_assertion(api, &options, parent, &slot));
            match result {
                Ok(assertion) => CeremonyEvent::SignedIn(assertion),
                Err(error) => CeremonyEvent::Failed(error),
            }
        },
        Arc::new(move || cancel(&canceller)),
    )
}

/// tdesktop `RegisterViaNative`.
fn make_credential(
    api: &Api,
    options: &CreationOptions,
    parent: ParentWindow,
    slot: &CancelSlot,
) -> Result<Registration, CeremonyError> {
    let client_data_json = options.client_data_json();
    let mut json = client_data_json.clone().into_bytes();
    let rp_id = wide(&options.rp_id);
    let rp_name = wide(&options.rp_name);
    let user_name = wide(&options.user_name);
    let user_display_name = wide(&options.user_display_name);
    let mut user_id = options.user_id.clone();
    let rp = WEBAUTHN_RP_ENTITY_INFORMATION {
        dwVersion: api::WEBAUTHN_RP_ENTITY_INFORMATION_CURRENT_VERSION,
        pwszId: rp_id.as_ptr(),
        pwszName: rp_name.as_ptr(),
        pwszIcon: null(),
    };
    let user = WEBAUTHN_USER_ENTITY_INFORMATION {
        dwVersion: api::WEBAUTHN_USER_ENTITY_INFORMATION_CURRENT_VERSION,
        cbId: length(user_id.len())?,
        pbId: user_id.as_mut_ptr(),
        pwszName: user_name.as_ptr(),
        pwszIcon: null(),
        pwszDisplayName: user_display_name.as_ptr(),
    };
    let mut params: Vec<WEBAUTHN_COSE_CREDENTIAL_PARAMETER> =
        windows_plan::cose_algorithms(&options.algorithms)
            .into_iter()
            .map(|alg| WEBAUTHN_COSE_CREDENTIAL_PARAMETER {
                dwVersion: api::WEBAUTHN_COSE_CREDENTIAL_PARAMETER_CURRENT_VERSION,
                pwszCredentialType: api::WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
                lAlg: alg,
            })
            .collect();
    let param_list = WEBAUTHN_COSE_CREDENTIAL_PARAMETERS {
        cCredentialParameters: length(params.len())?,
        pCredentialParameters: params.as_mut_ptr(),
    };
    let client = client_data(&mut json)?;
    let mut id = begin(api, slot)?;
    // SAFETY: every field is an integer, a BOOL or a raw pointer, for which
    // all-zero is valid (tdesktop `memset`s it the same way).
    let mut request: WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS = unsafe { std::mem::zeroed() };
    request.dwVersion = api.versions.make_credential_options;
    request.dwTimeoutMilliseconds = options.timeout_ms;
    request.dwAuthenticatorAttachment = ATTACHMENT_ANY;
    request.bRequireResidentKey = 0;
    request.dwUserVerificationRequirement = UV_PREFERRED;
    request.dwAttestationConveyancePreference = ATTESTATION_NONE;
    request.pCancellationId = id.as_mut().map_or(null_mut(), |id| id as *mut GUID);
    if api.versions.prefer_resident_key {
        request.bPreferResidentKey = 1;
    }
    let mut out: *mut WEBAUTHN_CREDENTIAL_ATTESTATION = null_mut();
    // SAFETY: every pointer above points into locals (`rp_id`, `user_id`,
    // `params`, `json`, `id`, ...) that live until after the call; `out` is
    // a valid out pointer. The call blocks this worker thread only.
    let hr = unsafe {
        (api.make_credential)(
            parent.0 as HWND,
            &rp,
            &user,
            &param_list,
            &client,
            &request,
            &mut out,
        )
    };
    end(slot);
    if hr < 0 || out.is_null() {
        let error = failure("MakeCredential", hr);
        if !matches!(error, CeremonyError::Cancelled) {
            super::log(&format!("{error:?}"));
        }
        return Err(error);
    }
    // SAFETY: a successful call returned a non-null attestation that stays
    // valid until it is freed below; the buffers are copied out first.
    let (credential_id, attestation_object) = unsafe {
        let attestation = &*out;
        (
            copy_out(attestation.pbCredentialId, attestation.cbCredentialId),
            copy_out(
                attestation.pbAttestationObject,
                attestation.cbAttestationObject,
            ),
        )
    };
    // SAFETY: `out` came from `WebAuthNAuthenticatorMakeCredential` and is
    // freed once, with its own free function.
    unsafe { (api.free_attestation)(out) };
    Ok(Registration {
        credential_id,
        attestation_object,
        client_data_json,
    })
}

/// tdesktop `LoginViaNative`.
fn get_assertion(
    api: &Api,
    options: &RequestOptions,
    parent: ParentWindow,
    slot: &CancelSlot,
) -> Result<Assertion, CeremonyError> {
    let client_data_json = options.client_data_json();
    let mut json = client_data_json.clone().into_bytes();
    let rp_id = wide(&options.rp_id);
    let mut ids = options.allow_credentials.clone();
    let mut allow = ids
        .iter_mut()
        .map(|id| {
            Ok(WEBAUTHN_CREDENTIAL {
                dwVersion: api::WEBAUTHN_CREDENTIAL_CURRENT_VERSION,
                cbId: length(id.len())?,
                pbId: id.as_mut_ptr(),
                pwszCredentialType: api::WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
            })
        })
        .collect::<Result<Vec<_>, CeremonyError>>()?;
    let client = client_data(&mut json)?;
    let mut id = begin(api, slot)?;
    // SAFETY: as in `make_credential`: all-zero is valid for every field.
    let mut request: WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS = unsafe { std::mem::zeroed() };
    request.dwVersion = api.versions.get_assertion_options;
    request.dwTimeoutMilliseconds = options.timeout_ms;
    if !allow.is_empty() {
        request.CredentialList = WEBAUTHN_CREDENTIALS {
            cCredentials: length(allow.len())?,
            pCredentials: allow.as_mut_ptr(),
        };
    }
    request.pCancellationId = id.as_mut().map_or(null_mut(), |id| id as *mut GUID);
    request.dwUserVerificationRequirement =
        windows_plan::login_user_verification(&options.user_verification);
    let mut out: *mut WEBAUTHN_ASSERTION = null_mut();
    // SAFETY: every pointer points into locals (`rp_id`, `ids`, `allow`,
    // `json`, `id`) that live until after the call; `out` is a valid out
    // pointer.
    let hr = unsafe {
        (api.get_assertion)(
            parent.0 as HWND,
            rp_id.as_ptr(),
            &client,
            &request,
            &mut out,
        )
    };
    end(slot);
    if hr < 0 || out.is_null() {
        let error = failure("GetAssertion", hr);
        if !matches!(error, CeremonyError::Cancelled) {
            super::log(&format!("{error:?}"));
        }
        return Err(error);
    }
    // SAFETY: a successful call returned a non-null assertion that stays
    // valid until it is freed below; the buffers are copied out first.
    let assertion = unsafe {
        let got = &*out;
        Assertion {
            credential_id: copy_out(got.Credential.pbId, got.Credential.cbId),
            authenticator_data: copy_out(got.pbAuthenticatorData, got.cbAuthenticatorData),
            signature: copy_out(got.pbSignature, got.cbSignature),
            user_handle: copy_out(got.pbUserId, got.cbUserId),
            client_data_json,
        }
    };
    // SAFETY: `out` came from `WebAuthNAuthenticatorGetAssertion` and is
    // freed once, with its own free function.
    unsafe { (api.free_assertion)(out) };
    Ok(assertion)
}
