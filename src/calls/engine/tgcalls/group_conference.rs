//! A conference call's own callbacks (tdesktop `GroupCall::conference()`):
//! every frame through [`FrameCrypto`] (`e2eEncryptDecrypt`), and tgcalls'
//! media channel requests answered from the participants' audio sources,
//! since decrypting a frame needs its sender (tdesktop
//! `mediaChannelDescriptionsStart`).
use super::group_callbacks::{GroupContext, context};
use crate::calls::engine::{FrameCrypto, SsrcOwners};
use crate::locks::LockExt;
use quill_calls_sys::{
    Loader, qc_group, qc_group_e2e_frame, qc_group_e2e_result_fn, qc_group_media_channel,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};

/// What a conference call's instance (and its presentation) reads.
pub(super) struct Conference {
    pub(super) crypto: Arc<dyn FrameCrypto>,
    /// The instance, once created: a media channel request is answered
    /// in place when its owners are known. Null until then, and the
    /// request waits for [`Conference::set_owners`].
    handle: AtomicPtr<qc_group>,
    media: Mutex<MediaChannels>,
}

#[derive(Default)]
struct MediaChannels {
    owners: SsrcOwners,
    /// Requests tgcalls is waiting on: `(request id, ssrcs)`.
    pending: Vec<(u64, Vec<u32>)>,
}

impl MediaChannels {
    /// The answer for `ssrcs`, once every one has an owner or every
    /// participant is loaded (tdesktop answers with the resolved ones).
    fn answer(&self, ssrcs: &[u32]) -> Option<Vec<qc_group_media_channel>> {
        let known: Vec<qc_group_media_channel> = ssrcs
            .iter()
            .filter_map(|&ssrc| {
                self.owners
                    .owner(ssrc)
                    .map(|user_id| qc_group_media_channel {
                        audio_ssrc: ssrc,
                        user_id,
                    })
            })
            .collect();
        (known.len() == ssrcs.len() || self.owners.complete).then_some(known)
    }
}

impl Conference {
    pub(super) fn new(crypto: Arc<dyn FrameCrypto>) -> Self {
        Self {
            crypto,
            handle: AtomicPtr::new(std::ptr::null_mut()),
            media: Mutex::new(MediaChannels::default()),
        }
    }

    /// The instance answers go to; set right after `qc_group_create`,
    /// cleared before `qc_group_destroy`.
    pub(super) fn set_handle(&self, handle: *mut qc_group) {
        self.handle.store(handle, Ordering::SeqCst);
    }

    /// The participants' sources changed: the requests that can be
    /// answered now, taken off the waiting list.
    pub(super) fn set_owners(
        &self,
        owners: &SsrcOwners,
    ) -> Vec<(u64, Vec<qc_group_media_channel>)> {
        let mut media = self.media.lock_or_recover();
        if media.owners == *owners {
            return Vec::new();
        }
        media.owners = owners.clone();
        let mut ready = Vec::new();
        let mut waiting = Vec::new();
        for (id, ssrcs) in std::mem::take(&mut media.pending) {
            match media.answer(&ssrcs) {
                Some(answer) => ready.push((id, answer)),
                None => waiting.push((id, ssrcs)),
            }
        }
        media.pending = waiting;
        ready
    }

    /// A media channel request: its answer now, or `None` when it waits
    /// for [`Conference::set_owners`].
    fn request(&self, id: u64, ssrcs: Vec<u32>) -> Option<Vec<qc_group_media_channel>> {
        let mut media = self.media.lock_or_recover();
        let answer = media.answer(&ssrcs);
        if answer.is_none() {
            media.pending.push((id, ssrcs));
        }
        answer
    }

    /// tgcalls cancelled request `id`; true when it was a media channel
    /// request.
    pub(super) fn cancel(&self, id: u64) -> bool {
        let mut media = self.media.lock_or_recover();
        let before = media.pending.len();
        media.pending.retain(|(pending, _)| *pending != id);
        media.pending.len() != before
    }

    /// Answers request `id` on the instance, if it is still there.
    pub(super) fn answer(&self, api: &Loader, id: u64, channels: &[qc_group_media_channel]) {
        let handle = self.handle.load(Ordering::SeqCst);
        if handle.is_null() {
            return;
        }
        // SAFETY: the handle is set only while the instance lives (cleared
        // before qc_group_destroy, which waits out every callback); the
        // channels are read during the call.
        unsafe {
            (api.qc_group_answer_media_channels)(handle, id, channels.as_ptr(), channels.len())
        };
    }

    /// `pending` request ids, for tests.
    #[cfg(test)]
    pub(super) fn pending(&self) -> Vec<u64> {
        self.media
            .lock_or_recover()
            .pending
            .iter()
            .map(|(id, _)| *id)
            .collect()
    }
}

/// tgcalls' `requestMediaChannelDescriptions`.
pub(super) unsafe extern "C" fn on_media_channels(
    user: *mut c_void,
    request_id: u64,
    ssrcs: *const u32,
    count: usize,
) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        let Some(conference) = call.conference.as_ref() else {
            return;
        };
        let ssrcs = if ssrcs.is_null() || count == 0 {
            Vec::new()
        } else {
            // SAFETY: the library hands `count` ssrcs at `ssrcs` for this call.
            unsafe { std::slice::from_raw_parts(ssrcs, count) }.to_vec()
        };
        if let Some(answer) = conference.request(request_id, ssrcs)
            && let Ok(api) = super::library()
        {
            conference.answer(api, request_id, &answer);
        }
    });
}

/// tgcalls' `e2eEncryptDecrypt`, synchronous on its media threads.
pub(super) unsafe extern "C" fn on_e2e(
    user: *mut c_void,
    frame: *const qc_group_e2e_frame,
    set_result: qc_group_e2e_result_fn,
    result: *mut c_void,
) {
    crate::thread_guard::ffi_boundary((), || {
        // SAFETY: tgcalls passes back the `user` the engine registered.
        let Some(call) = (unsafe { context(user) }) else {
            return;
        };
        // SAFETY: the library hands a frame valid for this call, or null.
        let Some(frame) = (unsafe { frame.as_ref() }) else {
            return;
        };
        let Some(transformed) = transform(call, frame) else {
            return;
        };
        // SAFETY: `result` is the library's own, valid during this call;
        // the library copies the bytes before `set_result` returns.
        unsafe { set_result(result, transformed.as_ptr(), transformed.len()) };
    });
}

fn transform(call: &GroupContext, frame: &qc_group_e2e_frame) -> Option<Vec<u8>> {
    let crypto = &call.conference.as_ref()?.crypto;
    let data = if frame.data.is_null() || frame.length == 0 {
        &[][..]
    } else {
        // SAFETY: the library hands `length` bytes at `data` for this call.
        unsafe { std::slice::from_raw_parts(frame.data, frame.length) }
    };
    if frame.is_encrypt != 0 {
        crypto.encrypt(data, frame.unencrypted_prefix_size)
    } else {
        crypto.decrypt(frame.user_id, data)
    }
}

#[cfg(test)]
#[path = "group_conference_tests.rs"]
mod tests;
