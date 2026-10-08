//! "Unlock with Touch ID / system authentication" (tdesktop
//! `base::SystemUnlock`). macOS shows the LocalAuthentication prompt
//! (Touch ID, Apple Watch or the account password). Windows Hello and a
//! Linux polkit prompt have no backend yet: `available()` is false there,
//! so the switch is not offered (docs/decisions/codex-local-passcode.md).

use std::sync::mpsc::{Receiver, channel};

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use objc2_foundation::{NSError, NSString};

    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}

    /// `LAPolicyDeviceOwnerAuthentication`: biometrics, Apple Watch or the
    /// account password, so a Mac without Touch ID still has a prompt.
    const POLICY: isize = 2;

    fn context() -> Retained<AnyObject> {
        // SAFETY: LAContext is a plain NSObject subclass; `new` returns an
        // owned instance.
        unsafe { msg_send![class!(LAContext), new] }
    }

    fn can_evaluate(policy: isize) -> bool {
        let ctx = context();
        let mut error: *mut NSError = std::ptr::null_mut();
        // SAFETY: documented selector; `error` is a valid out pointer.
        let ok: Bool = unsafe {
            msg_send![&*ctx, canEvaluatePolicy: policy, error: std::ptr::addr_of_mut!(error)]
        };
        ok.as_bool()
    }

    pub fn available() -> bool {
        can_evaluate(POLICY)
    }

    /// `LAPolicyDeviceOwnerAuthenticationWithBiometrics`.
    pub fn biometrics() -> bool {
        can_evaluate(1)
    }

    pub fn authenticate(reason: &str, done: std::sync::mpsc::Sender<bool>) {
        let ctx = context();
        let reason = NSString::from_str(reason);
        let keep = ctx.clone();
        let block = block2::RcBlock::new(move |ok: Bool, _error: *mut NSError| {
            // Keeps the context alive until the reply arrives.
            let _keep = &keep;
            let _ = done.send(ok.as_bool());
        });
        // SAFETY: documented selector; LocalAuthentication copies the block
        // and calls it once on a private queue.
        let _: () = unsafe {
            msg_send![&*ctx, evaluatePolicy: POLICY, localizedReason: &*reason, reply: &*block]
        };
    }
}

/// Whether the platform has a system unlock prompt.
pub fn available() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::available()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Switch label: "Unlock with Touch ID" where biometrics exist.
pub fn label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        if mac::biometrics() {
            return "Unlock with Touch ID";
        }
    }
    "Unlock with system password"
}

/// Show the prompt; the answer arrives on the receiver.
pub fn authenticate(reason: &str) -> Receiver<bool> {
    let (tx, rx) = channel();
    #[cfg(target_os = "macos")]
    mac::authenticate(reason, tx);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = reason;
        let _ = tx.send(false);
    }
    rx
}
