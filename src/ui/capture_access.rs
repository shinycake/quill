//! Camera and microphone permission before recording (tdesktop
//! `Platform::GetPermissionStatus` / `RequestPermission`): Quill asks
//! macOS itself, so the system prompt names Quill and a refusal is
//! explained before any capture starts.

use super::app::QuillApp;
use gpui_kit::*;
use std::time::Duration;

/// Where macOS stands on one kind of capture device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only macOS reports device permission")
)]
pub(super) enum Access {
    Granted,
    Denied,
    /// Not asked yet: requesting shows the system prompt.
    Undetermined,
}

#[cfg(target_os = "macos")]
fn media_type(camera: bool) -> Option<&'static objc2_av_foundation::AVMediaType> {
    // SAFETY: AVFoundation's media type constants are immutable statics.
    unsafe {
        if camera {
            objc2_av_foundation::AVMediaTypeVideo
        } else {
            objc2_av_foundation::AVMediaTypeAudio
        }
    }
}

#[cfg(target_os = "macos")]
fn access(camera: bool) -> Access {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice};
    let Some(kind) = media_type(camera) else {
        return Access::Granted;
    };
    // SAFETY: a class method taking one of AVFoundation's own media types.
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(kind) } {
        AVAuthorizationStatus::Authorized => Access::Granted,
        AVAuthorizationStatus::NotDetermined => Access::Undetermined,
        _ => Access::Denied,
    }
}

/// Windows has no prompt for desktop apps: the privacy switches in
/// Settings decide, and a refusal is read from the consent store.
#[cfg(windows)]
fn access(camera: bool) -> Access {
    if quill::media_tools::windows_privacy_denied(camera) {
        Access::Denied
    } else {
        Access::Granted
    }
}

/// Linux has no central permission: the capture error says what failed.
#[cfg(not(any(target_os = "macos", windows)))]
fn access(_camera: bool) -> Access {
    Access::Granted
}

/// Show the system prompt; `done` gets the answer on an arbitrary thread.
#[cfg(target_os = "macos")]
fn request(camera: bool, done: std::sync::mpsc::Sender<bool>) {
    use objc2_av_foundation::AVCaptureDevice;
    let Some(kind) = media_type(camera) else {
        let _ = done.send(true);
        return;
    };
    let block = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = done.send(granted.as_bool());
    });
    // SAFETY: AVFoundation copies the block and calls it once.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(kind, &block) };
}

#[cfg(not(target_os = "macos"))]
fn request(_camera: bool, done: std::sync::mpsc::Sender<bool>) {
    let _ = done.send(true);
}

#[cfg(windows)]
fn denied_note(camera: bool) -> String {
    let (device, pane) = if camera {
        ("camera", "Camera")
    } else {
        ("microphone", "Microphone")
    };
    format!(
        "Quill needs access to your {device} to record. Turn on \"Let desktop apps access \
         your {device}\" in Settings › Privacy & security › {pane}."
    )
}

#[cfg(not(windows))]
fn denied_note(camera: bool) -> String {
    let device = if camera { "camera" } else { "microphone" };
    let pane = if camera { "Camera" } else { "Microphone" };
    format!(
        "Quill needs access to your {device} to record. \
         Allow it in System Settings → Privacy & Security → {pane}."
    )
}

impl QuillApp {
    /// Run `start` once the devices a recording needs are allowed: the
    /// microphone, plus the camera for a video message. Asks for any not
    /// decided yet, one prompt at a time; a refusal leaves a note instead.
    pub(super) fn with_capture_access(
        &mut self,
        camera: bool,
        start: fn(&mut Self, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) {
        let devices: &[bool] = if camera { &[true, false] } else { &[false] };
        for &device in devices {
            match access(device) {
                Access::Granted => {}
                Access::Denied => {
                    self.status_note = denied_note(device);
                    cx.notify();
                    return;
                }
                Access::Undetermined => {
                    self.status_note = format!(
                        "Allow Quill to use the {} to record.",
                        if device { "camera" } else { "microphone" }
                    );
                    cx.notify();
                    let (sender, answer) = std::sync::mpsc::channel();
                    request(device, sender);
                    cx.spawn(async move |this, cx| {
                        let granted = loop {
                            match answer.try_recv() {
                                Ok(granted) => break granted,
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    cx.background_executor()
                                        .timer(Duration::from_millis(100))
                                        .await;
                                }
                                Err(_) => break false,
                            }
                        };
                        let _ = this.update(cx, |this, cx| {
                            if granted {
                                this.with_capture_access(camera, start, cx);
                            } else {
                                this.status_note = denied_note(device);
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                    return;
                }
            }
        }
        start(self, cx);
    }
}
