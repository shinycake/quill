//! Camera and microphone permission before recording (tdesktop
//! `Platform::GetPermissionStatus` / `RequestPermission`): Quill asks
//! macOS itself, so the system prompt names Quill and a refusal is
//! explained before any capture starts.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};

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
fn request(camera: bool, done: futures_channel::oneshot::Sender<bool>) {
    use objc2_av_foundation::AVCaptureDevice;
    let Some(kind) = media_type(camera) else {
        let _ = done.send(true);
        return;
    };
    // The block is called once, but the type says `Fn`: the sender waits in
    // a slot it is taken from.
    let done = std::sync::Mutex::new(Some(done));
    let block = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
        if let Some(done) = done.lock().ok().and_then(|mut slot| slot.take()) {
            let _ = done.send(granted.as_bool());
        }
    });
    // SAFETY: AVFoundation copies the block and calls it once.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(kind, &block) };
}

#[cfg(not(target_os = "macos"))]
fn request(_camera: bool, done: futures_channel::oneshot::Sender<bool>) {
    let _ = done.send(true);
}

#[cfg(windows)]
fn denied_note(camera: bool) -> String {
    let (device, pane) = if camera {
        ("camera", tr(keys::Q_MISC_CAMERA))
    } else {
        ("microphone", tr(keys::Q_MISC_MICROPHONE))
    };
    tr_with(
        keys::Q_MISC_QUILL_NEEDS_ACCESS_TO_YOUR,
        &[
            ("device", &device.to_string()),
            ("device_2", &device.to_string()),
            ("pane", &pane.to_string()),
        ],
    )
}

#[cfg(not(windows))]
fn denied_note(camera: bool) -> String {
    let device = if camera { "camera" } else { "microphone" };
    let pane = if camera {
        tr(keys::Q_MISC_CAMERA)
    } else {
        tr(keys::Q_MISC_MICROPHONE)
    };
    tr_with(
        keys::Q_MISC_QUILL_NEEDS_ACCESS_TO_YOUR_2,
        &[("device", device), ("pane", pane)],
    )
}

/// What runs once access is granted: it carries what asked, so an answer
/// to one prompt never starts another request's work.
type AfterAccess = Box<dyn FnOnce(&mut QuillApp, &mut Context<QuillApp>)>;

impl QuillApp {
    /// Run `start` once the devices a recording needs are allowed: the
    /// microphone, plus the camera for a video message. Asks for any not
    /// decided yet, one prompt at a time; a refusal leaves a note instead.
    pub(super) fn with_capture_access(
        &mut self,
        camera: bool,
        start: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let devices: &'static [bool] = if camera { &[true, false] } else { &[false] };
        self.with_device_access(devices, Box::new(start), cx);
    }

    /// Run `start` once the camera is allowed (a call turning it on), or
    /// say why it can't be used.
    pub(super) fn with_camera_access(
        &mut self,
        start: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.with_device_access(&[true], Box::new(start), cx);
    }

    /// `devices`: `true` for the camera, `false` for the microphone.
    fn with_device_access(
        &mut self,
        devices: &'static [bool],
        start: AfterAccess,
        cx: &mut Context<Self>,
    ) {
        // A prompt already up belongs to an earlier press: this one must not
        // wipe it (its Allow would be ignored), nor start behind it.
        if self.recording.asking() {
            return;
        }
        for &device in devices {
            match access(device) {
                Access::Granted => {}
                Access::Denied => {
                    self.set_note(StatusNote::error(denied_note(device)));
                    cx.notify();
                    return;
                }
                Access::Undetermined => {
                    // Another recording is starting or running: this press
                    // would only queue a second start behind it.
                    if self.recording.starting() {
                        self.set_note(StatusNote::silent(tr(
                            keys::Q_MISC_FINISH_THE_RECORDING_FIRST,
                        )));
                        cx.notify();
                        return;
                    }
                    let call = self.active_call_id();
                    let ask = self.recording.next_start_id();
                    self.set_note(StatusNote::silent(tr(if device {
                        keys::Q_LEFTOVERS_ALLOW_CAMERA
                    } else {
                        keys::Q_LEFTOVERS_ALLOW_MICROPHONE
                    })));
                    let (sender, answer) = futures_channel::oneshot::channel();
                    request(device, sender);
                    // The wait belongs to the account on screen, so the stage
                    // ends whichever window asked; the recording starts in
                    // the window that asked, or not at all once it closed.
                    let asker = self.scoped_app(cx);
                    let wait = self.spawn_screen(cx, async move |this, cx| {
                        // The prompt's answer wakes this; a prompt that went
                        // away without one counts as a refusal.
                        let granted = answer.await.unwrap_or(false);
                        let ended = this.update(cx, |this, cx| {
                            let mine = this.recording.finish_asking(ask);
                            if mine {
                                cx.notify();
                            }
                            mine
                        });
                        if !ended.is_ok_and(|mine| mine) {
                            return;
                        }
                        let _ = asker.update(cx, |this, cx| {
                            // The call the press was for ended or changed
                            // while the prompt was up: nothing to start.
                            if call.is_some() && this.active_call_id() != call {
                                return;
                            }
                            if granted {
                                this.with_device_access(devices, start, cx);
                            } else {
                                this.set_note(StatusNote::error(denied_note(device)));
                                cx.notify();
                            }
                        });
                    });
                    self.recording.begin_asking(ask, wait);
                    cx.notify();
                    return;
                }
            }
        }
        start(self, cx);
    }

    /// The id of the call on screen, if any.
    fn active_call_id(&self) -> Option<i32> {
        self.live
            .as_ref()
            .and_then(|live| live.driver.session.calls.active_call.as_ref())
            .map(|call| call.id)
    }
}
