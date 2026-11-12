//! "Unlock with Touch ID / system authentication" (tdesktop
//! `base::SystemUnlock`). macOS shows the LocalAuthentication prompt
//! (Touch ID, Apple Watch or the account password). Windows Hello and a
//! Linux polkit prompt have no backend yet: `available()` is false there,
//! so the switch is not offered (docs/decisions/codex-local-passcode.md).
#![deny(clippy::indexing_slicing)]

use futures_channel::oneshot::{Receiver, channel};
use quill::lang::{keys, tr};

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

    /// Close the prompt: the reply then comes with a failure.
    pub fn invalidate(ctx: &AnyObject) {
        // SAFETY: documented selector, callable from any thread; it ends an
        // evaluation still in flight.
        let _: () = unsafe { msg_send![ctx, invalidate] };
    }

    pub fn authenticate(
        reason: &str,
        done: futures_channel::oneshot::Sender<bool>,
    ) -> Retained<AnyObject> {
        let ctx = context();
        let reason = NSString::from_str(reason);
        let keep = ctx.clone();
        // The block is called once; `Cell::take` hands the sender over.
        let done = std::cell::Cell::new(Some(done));
        let block = block2::RcBlock::new(move |ok: Bool, _error: *mut NSError| {
            // Keeps the context alive until the reply arrives.
            let _keep = &keep;
            if let Some(done) = done.take() {
                let _ = done.send(ok.as_bool());
            }
        });
        // SAFETY: documented selector; LocalAuthentication copies the block
        // and calls it once on a private queue.
        let _: () = unsafe {
            msg_send![&*ctx, evaluatePolicy: POLICY, localizedReason: &*reason, reply: &*block]
        };
        ctx
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
            return tr(keys::SETTINGS_USE_TOUCHID);
        }
    }
    tr(keys::SETTINGS_USE_SYSTEMPWD)
}

/// Closes the OS prompt when dropped (macOS: `LAContext invalidate`), so a
/// prompt nobody waits for any more leaves the screen with its wait.
pub struct Close {
    #[cfg(target_os = "macos")]
    context: Option<objc2::rc::Retained<objc2::runtime::AnyObject>>,
}

impl Drop for Close {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if let Some(context) = self.context.take() {
                mac::invalidate(&context);
            }
        }
    }
}

/// The prompt on screen: its answer, and the handle that closes it.
pub struct Prompt {
    pub answer: Receiver<bool>,
    pub close: Close,
}

impl Prompt {
    /// An answer with no OS prompt behind it.
    pub fn without_prompt(answer: Receiver<bool>) -> Self {
        Self {
            answer,
            close: Close {
                #[cfg(target_os = "macos")]
                context: None,
            },
        }
    }
}

/// Show the prompt; the answer arrives on the prompt's receiver, and
/// dropping the prompt closes it.
pub fn authenticate(reason: &str) -> Prompt {
    let (tx, rx) = channel();
    #[cfg(target_os = "macos")]
    {
        let context = mac::authenticate(reason, tx);
        Prompt {
            answer: rx,
            close: Close {
                context: Some(context),
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = reason;
        let _ = tx.send(false);
        Prompt::without_prompt(rx)
    }
}
