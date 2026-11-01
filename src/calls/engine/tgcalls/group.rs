//! Group calls (video chats) on tgcalls, driven the way Telegram Desktop
//! drives them (calls_group_call.cpp): one `qc_group` instance per call,
//! and a second one for its presentation (the shared screen) while you
//! share it.
//!
//! The library has no devices: Quill's microphone is pushed in through
//! the external frame sender, its screen through the presentation's
//! external source, and the call's sound comes out to a `StreamPlayer`.
use super::group_callbacks::{GroupContext, on_local_video};
use super::group_config::{RTMP_ENDPOINT, join_mode, requested_video_json, unified_min_quality};
use super::*;
use crate::locks::LockExt;
use quill_calls_sys::{QC_GROUP_MODE_BROADCAST, QC_GROUP_MODE_RTC, qc_group, qc_video_capture};
use std::collections::HashSet;
use std::ffi::CString;

/// One group call, by TDLib group call id.
pub(super) struct GroupCall {
    chat_id: i64,
    role: GroupCallRole,
    /// A conference call's encryption, shared with its presentation.
    crypto: Option<Arc<dyn FrameCrypto>>,
    context: Arc<GroupContext>,
    handle: Native<qc_group>,
    /// Feeder threads' way in; cleared before the handles go away.
    gate: Arc<sender::Gate>,
    /// The join answer was applied (`connect_group_call`).
    connected: bool,
    mode: GroupCallMode,
    muted: bool,
    /// The camera to send, by device id, and its source while it runs.
    camera_id: Option<String>,
    camera: Option<Native<qc_video_capture>>,
    speaker: Option<String>,
    playback_muted: bool,
    sound: Option<StreamPlayer>,
    /// The participant videos last asked for, kept so a connect or a
    /// new stream quality asks again without waiting for the driver.
    video_sources: Vec<GroupVideoSource>,
    /// The requested-video JSON last given to tgcalls.
    requested_video: Option<String>,
    /// Endpoints with an incoming video output.
    outputs: HashSet<String>,
    presentation: Option<Presentation>,
    stream_quality: Option<StreamQuality>,
}

/// The screen-sharing instance beside a call (tdesktop's `_screenInstance`).
struct Presentation {
    /// What the instance's callbacks read; outlives `qc_group_destroy`.
    context: Arc<GroupContext>,
    handle: Native<qc_group>,
    /// The external screencast source Quill's screen capture feeds.
    screen: Native<qc_video_capture>,
    connected: bool,
}

/// A new instance and its join payload; `None` (logged) when tgcalls
/// refuses it.
fn create_instance(
    api: &'static Loader,
    context: &Arc<GroupContext>,
    video: *mut qc_video_capture,
) -> Option<(Native<qc_group>, String)> {
    let config = match (context.presentation, context.conference.is_some()) {
        (false, false) => c"{\"presentation\":false}",
        (true, false) => c"{\"presentation\":true}",
        (false, true) => c"{\"presentation\":false,\"e2e\":true}",
        (true, true) => c"{\"presentation\":true,\"e2e\":true}",
    };
    let callbacks = context.callbacks();
    // SAFETY: NUL-ended config; the context behind `callbacks.user` lives
    // until `qc_group_destroy` returns; the instance takes
    // its own reference to `video` (or none for NULL).
    let handle = unsafe { (api.qc_group_create)(config.as_ptr(), &callbacks, video) };
    let handle = Native::new(handle)?;
    if let Some(conference) = context.conference.as_ref() {
        for (id, channels) in conference.attach(handle.as_ptr()) {
            conference.answer(api, id, &channels);
        }
    }
    // SAFETY: a live instance, not called from one of its callbacks; the
    // payload is the caller's to free.
    let raw = unsafe { (api.qc_group_join_payload)(handle.as_ptr(), std::ptr::null_mut()) };
    match take_string(api, raw) {
        Some(payload) => Some((handle, payload)),
        None => {
            context.retire();
            close_conference(context);
            let context = Arc::clone(context);
            reaper::hand_off(move || {
                // SAFETY: the instance made above, destroyed once and never
                // from its own callback; the context outlives it.
                unsafe { (api.qc_group_destroy)(handle.as_ptr()) };
                drop(context);
            });
            None
        }
    }
}

/// Called before an instance's destroy: a conference call's frames are
/// dropped from now on instead of going to TDLib (each would wait up to
/// [`crate::telegram::group_call_crypto::FRAME_TIMEOUT`] while the destroy
/// waits for it), and nothing answers the instance any more. Every
/// destroy goes through here: leaving, a rejoin's fresh instance, a
/// refused create, the presentation, and the engine's drop (the driver
/// dropped, the account switched).
fn close_conference(context: &GroupContext) {
    if let Some(conference) = context.conference.as_ref() {
        conference.close();
    }
}

/// Applies a join answer the way tdesktop's
/// `handlePossibleCreateOrJoinResponse` does: the connection mode first,
/// then (for RTC) the answer itself.
fn apply_answer(api: &Loader, handle: Native<qc_group>, answer: &CString, mode: GroupCallMode) {
    match mode {
        GroupCallMode::Rtc => {
            // SAFETY: a live instance; `answer` is NUL-ended and outlives
            // these synchronous calls.
            unsafe {
                (api.qc_group_set_connection_mode)(handle.as_ptr(), QC_GROUP_MODE_RTC, 0);
                (api.qc_group_set_join_response)(handle.as_ptr(), answer.as_ptr());
            }
        }
        GroupCallMode::Stream | GroupCallMode::Rtmp => {
            let unified = i32::from(mode == GroupCallMode::Rtmp);
            // SAFETY: a live instance.
            unsafe {
                (api.qc_group_set_connection_mode)(
                    handle.as_ptr(),
                    QC_GROUP_MODE_BROADCAST,
                    unified,
                )
            };
        }
    }
}

/// Stops sending the camera and frees Quill's reference, as turning the
/// camera off does, so the next join can open the device without waiting
/// for this instance's destroy behind the reaper's backlog.
fn stop_camera(api: &Loader, call: &mut GroupCall) {
    if call.camera.is_some() {
        // SAFETY: a live instance; NULL stops sending video.
        unsafe { (api.qc_group_set_video_capture)(call.handle.as_ptr(), std::ptr::null_mut()) };
    }
    if let Some(capture) = call.camera.take() {
        // SAFETY: Quill's live reference; once the preview is cleared no
        // picture reaches the context, then the reference goes.
        unsafe {
            (api.qc_video_capture_set_preview)(capture.as_ptr(), None, std::ptr::null_mut());
            (api.qc_video_capture_free)(capture.as_ptr());
        }
    }
}

/// Ends the presentation: no screen picture reaches it afterwards.
fn stop_presentation(api: &'static Loader, call: &mut GroupCall) {
    let Some(presentation) = call.presentation.take() else {
        return;
    };
    call.gate.set_screen(None);
    presentation.retire();
    reaper::hand_off(move || presentation.destroy(api));
}

impl Presentation {
    fn retire(&self) {
        self.context.retire();
        close_conference(&self.context);
    }

    fn destroy(self, api: &Loader) {
        // SAFETY: the presentation's live handle, destroyed once and never
        // from its own callback; then Quill's reference to its screen
        // source, which the gate no longer borrows. The context stays alive
        // until both return.
        unsafe {
            (api.qc_group_destroy)(self.handle.as_ptr());
            (api.qc_video_capture_free)(self.screen.as_ptr());
        }
    }
}

pub(super) fn retire_group(api: &'static Loader, mut call: GroupCall) {
    call.context.retire();
    close_conference(&call.context);
    if let Some(presentation) = call.presentation.as_ref() {
        presentation.retire();
    }
    call.gate.close();
    call.context.sound.store(None);
    stop_camera(api, &mut call);
    reaper::hand_off(move || destroy_group(api, &mut call));
}

fn destroy_group(api: &Loader, call: &mut GroupCall) {
    call.sound = None;
    if let Some(presentation) = call.presentation.take() {
        presentation.destroy(api);
    }
    // SAFETY: a live instance destroyed once, never from its own callback
    // (this runs on the reaper).
    unsafe { (api.qc_group_destroy)(call.handle.as_ptr()) };
}

impl TgcallsEngine {
    fn group(&self, group_call_id: i32) -> Result<&GroupCall, EngineError> {
        self.groups
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))
    }

    fn group_mut(&mut self, group_call_id: i32) -> Result<&mut GroupCall, EngineError> {
        self.groups
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))
    }

    /// A fresh instance for `group_call_id` (a rejoin replaces the old
    /// one) and its join payload, tdesktop's `GroupJoinPayload` JSON,
    /// which `joinVideoChat` carries as is.
    pub(super) fn create_group(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
        role: GroupCallRole,
        crypto: Option<Arc<dyn FrameCrypto>>,
    ) -> Result<String, EngineError> {
        if let Some(old) = self.groups.remove(&group_call_id) {
            retire_group(self.api, old);
        }
        let mut context = GroupContext::new(group_call_id, chat_id, false, &self.callback);
        if let Some(crypto) = crypto.as_ref() {
            context = context.conference(Arc::clone(crypto));
        }
        let context = Arc::new(context);
        let (handle, payload) = create_instance(self.api, &context, std::ptr::null_mut()).ok_or(
            EngineError::Engine {
                op: "qc_group_create",
                code: -1,
            },
        )?;
        crate::advanced::debug_mode::call_log(&format!(
            "[call] group {group_call_id} tgcalls create ({role:?})"
        ));
        self.groups.insert(
            group_call_id,
            GroupCall {
                chat_id,
                role,
                crypto,
                context,
                handle,
                gate: Arc::new(sender::Gate::default()),
                connected: false,
                mode: GroupCallMode::Rtc,
                muted: true,
                camera_id: None,
                camera: None,
                speaker: None,
                playback_muted: false,
                sound: None,
                video_sources: Vec::new(),
                requested_video: None,
                outputs: HashSet::new(),
                presentation: None,
                stream_quality: None,
            },
        );
        Ok(payload)
    }

    /// Applies the `joinVideoChat` answer once, then the call's sound,
    /// microphone and `camera`. A stream is receive-only: nothing is sent.
    pub(super) fn connect_group(
        &mut self,
        group_call_id: i32,
        answer: &str,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        if call.connected {
            return Ok(());
        }
        let answer = text("qc_group_set_join_response", answer)?;
        let mode = join_mode(&answer.to_string_lossy());
        apply_answer(api, call.handle, &answer, mode);
        crate::advanced::debug_mode::call_log(&format!(
            "[call] group {group_call_id} tgcalls joined ({mode:?})"
        ));
        call.connected = true;
        call.mode = mode;
        call.camera_id = camera.map(str::to_owned);
        if call.role == GroupCallRole::Member && !mode.is_stream() {
            call.gate.open_group(api, call.handle.as_ptr());
        }
        // The mode decides where pictures go and whether an RTMP stream's
        // unified video is asked for: route every video again.
        call.requested_video = None;
        let sources = call.video_sources.clone();
        self.apply_group_mute(group_call_id)?;
        self.open_group_sound(group_call_id)?;
        self.sync_group_videos(group_call_id, &sources)?;
        self.apply_group_camera(group_call_id)
    }

    /// Whether the call sends anything: a member's RTC call does.
    fn group_sends(call: &GroupCall) -> bool {
        call.connected && call.role == GroupCallRole::Member && !call.mode.is_stream()
    }

    fn apply_group_mute(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group(group_call_id)?;
        // A viewer's instance stays muted, as it starts.
        let muted = call.muted || call.role == GroupCallRole::Viewer;
        // SAFETY: a live instance.
        unsafe { (api.qc_group_set_muted)(call.handle.as_ptr(), i32::from(muted)) };
        Ok(())
    }

    /// Opens (or reopens, for another speaker) the output the call's
    /// sound plays on. A failed open leaves the call silent and logs why.
    fn open_group_sound(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let stream_volume = self.stream_volume;
        let call = self.group_mut(group_call_id)?;
        let name = devices::speaker_name(call.speaker.as_deref());
        // A stream plays at the media viewer's volume, as in tdesktop.
        let volume = if call.mode.is_stream() {
            stream_volume
        } else {
            1.0
        };
        match StreamPlayer::open(name.as_deref(), volume, call.playback_muted) {
            Ok(player) => {
                call.context.sound.store(Some(player.audio().clone()));
                call.sound = Some(player);
            }
            Err(err) => {
                crate::advanced::debug_mode::call_log(&format!(
                    "[call] group {group_call_id} sound output did not open ({err})"
                ));
                call.context.sound.store(None);
                call.sound = None;
            }
        }
        Ok(())
    }

    /// Sends the call's camera, or stops sending video without one.
    /// Only a member's connected RTC call sends; the choice is kept
    /// for the connect otherwise.
    fn apply_group_camera(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        let sends = Self::group_sends(call);
        let wanted = call.camera_id.clone().filter(|_| sends);
        match (wanted, call.camera) {
            (Some(device), Some(capture)) => {
                let device = text("qc_video_capture_switch_camera", &device)?;
                // SAFETY: `capture` is Quill's live reference.
                unsafe { (api.qc_video_capture_switch_camera)(capture.as_ptr(), device.as_ptr()) };
                Ok(())
            }
            (Some(device), None) => {
                let device = text("qc_video_capture_camera", &device)?;
                // SAFETY: a NUL-ended id; the library returns a new source.
                let capture = unsafe { (api.qc_video_capture_camera)(device.as_ptr()) };
                let capture = Native::new(capture).ok_or(EngineError::NoCamera)?;
                // SAFETY: the context outlives the preview: it is cleared
                // before the capture is freed (`stop_camera`).
                unsafe {
                    (api.qc_video_capture_set_preview)(
                        capture.as_ptr(),
                        Some(on_local_video),
                        call.context.as_user(),
                    );
                }
                call.camera = Some(capture);
                // SAFETY: both live; the instance takes its own reference.
                unsafe { (api.qc_group_set_video_capture)(call.handle.as_ptr(), capture.as_ptr()) };
                Ok(())
            }
            (None, Some(_)) => {
                stop_camera(api, call);
                Ok(())
            }
            (None, None) => Ok(()),
        }
    }

    pub(super) fn set_group_camera_device(
        &mut self,
        group_call_id: i32,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let call = self.group_mut(group_call_id)?;
        let previous = std::mem::replace(&mut call.camera_id, camera.map(str::to_owned));
        if let Err(err) = self.apply_group_camera(group_call_id) {
            self.group_mut(group_call_id)?.camera_id = previous;
            return Err(err);
        }
        Ok(())
    }

    /// Receives exactly `sources`' videos, plus the unified video of a
    /// connected RTMP stream (tdesktop's `RtmpEndpointId`): tgcalls is told
    /// only when the set (or its order, or the stream's quality) changed,
    /// and each new endpoint gets an output before stale ones lose
    /// theirs. In a stream every picture is the stream's, as the stream
    /// window shows one; in a call each goes to its participant's tile.
    pub(super) fn sync_group_videos(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        let unified = (call.connected && call.mode == GroupCallMode::Rtmp)
            .then(|| unified_min_quality(call.stream_quality));
        if call.video_sources != sources {
            call.video_sources = sources.to_vec();
        }
        let stream = call.connected && call.mode.is_stream();
        // A stream shows one picture: the RTMP feed alone, or one speaker
        // (their screen first), so the feed never flips between people.
        let chosen: Vec<GroupVideoSource> = match (stream, unified.is_some()) {
            (false, _) => sources.to_vec(),
            (true, true) => Vec::new(),
            (true, false) => sources
                .iter()
                .find(|source| source.screen)
                .or_else(|| sources.first())
                .into_iter()
                .map(|source| GroupVideoSource {
                    large: true,
                    ..source.clone()
                })
                .collect(),
        };
        let sources = chosen.as_slice();
        let requested = requested_video_json(sources, unified);
        if call.requested_video.as_deref() == Some(requested.as_str()) {
            return Ok(());
        }
        let key = |source: &GroupVideoSource| {
            if stream {
                LiveKey::Stream {
                    chat_id: call.chat_id,
                }
            } else {
                LiveKey::Video {
                    call_id: group_call_id,
                    user_id: source.user_id,
                    screen: source.screen,
                }
            }
        };
        let mut owners: HashMap<String, LiveKey> = sources
            .iter()
            .map(|source| (source.endpoint.clone(), key(source)))
            .collect();
        let mut endpoints = sources
            .iter()
            .map(|source| {
                text("qc_group_add_incoming_video", &source.endpoint)
                    .map(|native| (source.endpoint.clone(), native))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if unified.is_some() {
            owners.insert(
                RTMP_ENDPOINT.to_owned(),
                LiveKey::Stream {
                    chat_id: call.chat_id,
                },
            );
            endpoints.push((
                RTMP_ENDPOINT.to_owned(),
                text("qc_group_add_incoming_video", RTMP_ENDPOINT)?,
            ));
        }
        let channels = text("qc_group_set_requested_video", &requested)?;
        // SAFETY: a live instance; `channels` is NUL-ended.
        let accepted =
            unsafe { (api.qc_group_set_requested_video)(call.handle.as_ptr(), channels.as_ptr()) };
        if accepted == 0 {
            return Err(EngineError::Engine {
                op: "qc_group_set_requested_video",
                code: -1,
            });
        }
        // Pictures route by the new owners from here on.
        *call.context.endpoints.lock_or_recover() = owners;
        for (endpoint, native) in &endpoints {
            if call.outputs.insert(endpoint.clone()) {
                // SAFETY: a live instance; `native` is NUL-ended.
                unsafe { (api.qc_group_add_incoming_video)(call.handle.as_ptr(), native.as_ptr()) };
            }
        }
        let wanted: HashSet<&String> = endpoints.iter().map(|(endpoint, _)| endpoint).collect();
        let stale: Vec<String> = call
            .outputs
            .iter()
            .filter(|endpoint| !wanted.contains(endpoint))
            .cloned()
            .collect();
        for endpoint in stale {
            call.outputs.remove(&endpoint);
            let native = text("qc_group_remove_incoming_video", &endpoint)?;
            // SAFETY: a live instance; `native` is NUL-ended.
            unsafe { (api.qc_group_remove_incoming_video)(call.handle.as_ptr(), native.as_ptr()) };
        }
        call.requested_video = Some(requested);
        Ok(())
    }

    pub(super) fn set_group_mute(
        &mut self,
        group_call_id: i32,
        muted: bool,
    ) -> Result<(), EngineError> {
        self.group_mut(group_call_id)?.muted = muted;
        self.apply_group_mute(group_call_id)
    }

    /// The output device and your local mute of the call's sound: a new
    /// device reopens the output, a mute only silences it.
    pub(super) fn set_group_output(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
        muted: bool,
    ) -> Result<(), EngineError> {
        let call = self.group_mut(group_call_id)?;
        let reopen = call.speaker.as_deref() != speaker && call.connected;
        call.speaker = speaker.map(str::to_owned);
        call.playback_muted = muted;
        if let Some(player) = call.sound.as_ref() {
            player.audio().gain().set_muted(muted);
        }
        if reopen {
            self.open_group_sound(group_call_id)?;
        }
        Ok(())
    }

    /// The presentation's instance and its payload for
    /// `startGroupCallScreenSharing`; a running one is replaced.
    pub(super) fn start_presentation(&mut self, group_call_id: i32) -> Result<String, EngineError> {
        let api = self.api;
        let shared = Arc::clone(&self.callback);
        let call = self.group_mut(group_call_id)?;
        stop_presentation(api, call);
        // SAFETY: returns a new external screencast source owned here.
        let screen = unsafe { (api.qc_video_capture_external)(1) };
        let screen = Native::new(screen).ok_or(EngineError::NoScreenSource)?;
        let mut context = GroupContext::new(group_call_id, call.chat_id, true, &shared);
        if let Some(crypto) = call.crypto.as_ref() {
            context = context.conference(Arc::clone(crypto));
        }
        let context = Arc::new(context);
        let Some((handle, payload)) = create_instance(api, &context, screen.as_ptr()) else {
            // SAFETY: Quill's only reference to the source made above.
            unsafe { (api.qc_video_capture_free)(screen.as_ptr()) };
            return Err(EngineError::Engine {
                op: "qc_group_create",
                code: -1,
            });
        };
        call.presentation = Some(Presentation {
            context,
            handle,
            screen,
            connected: false,
        });
        Ok(payload)
    }

    /// Applies the `startGroupCallScreenSharing` answer once; screen
    /// pictures reach the presentation from then on.
    pub(super) fn connect_presentation(
        &mut self,
        group_call_id: i32,
        answer: &str,
    ) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.group_mut(group_call_id)?;
        let presentation = call.presentation.as_mut().ok_or(EngineError::Engine {
            op: "qc_group_set_join_response",
            code: -1,
        })?;
        if presentation.connected {
            return Ok(());
        }
        let answer = text("qc_group_set_join_response", answer)?;
        // tdesktop's screen instance is always RTC.
        apply_answer(api, presentation.handle, &answer, GroupCallMode::Rtc);
        presentation.connected = true;
        call.gate.set_screen(Some(presentation.screen.as_ptr()));
        Ok(())
    }

    pub(super) fn stop_group_presentation(
        &mut self,
        group_call_id: i32,
    ) -> Result<(), EngineError> {
        let api = self.api;
        stop_presentation(api, self.group_mut(group_call_id)?);
        Ok(())
    }

    pub(super) fn has_presentation(&self, group_call_id: i32) -> bool {
        self.groups
            .get(&group_call_id)
            .is_some_and(|call| call.presentation.is_some())
    }

    /// A conference call created under a provisional key before the join
    /// answer named it: from here on it is `to`. Pictures route by the new
    /// id from the next video sync.
    pub(super) fn rekey_group(&mut self, from: i32, to: i32) -> Result<(), EngineError> {
        if from == to {
            return self.group(from).map(|_| ());
        }
        let mut call = self
            .groups
            .remove(&from)
            .ok_or(EngineError::NoSuchCall(from))?;
        if let Some(old) = self.groups.remove(&to) {
            retire_group(self.api, old);
        }
        call.context.set_group_call_id(to);
        if let Some(presentation) = call.presentation.as_ref() {
            presentation.context.set_group_call_id(to);
        }
        call.requested_video = None;
        crate::advanced::debug_mode::call_log(&format!("[call] group {from} is call {to}"));
        self.groups.insert(to, call);
        Ok(())
    }

    /// Who sends each audio ssrc of a conference call: the media channel
    /// requests waiting on them are answered.
    pub(super) fn set_ssrc_owners(&mut self, group_call_id: i32, owners: &SsrcOwners) {
        let api = self.api;
        let Some(conference) = self
            .groups
            .get(&group_call_id)
            .and_then(|call| call.context.conference.as_ref())
        else {
            return;
        };
        for (id, channels) in conference.set_owners(owners) {
            conference.answer(api, id, &channels);
        }
    }

    pub(super) fn leave_group(&mut self, group_call_id: i32) {
        if let Some(call) = self.groups.remove(&group_call_id) {
            crate::advanced::debug_mode::call_log(&format!(
                "[call] group {group_call_id} tgcalls leave"
            ));
            retire_group(self.api, call);
        }
    }

    /// The sender for a member's connected RTC call: a stream (and a
    /// viewer) sends nothing.
    pub(super) fn group_sender(&self, group_call_id: i32) -> Option<Arc<dyn ExternalFrameSender>> {
        let call = self.groups.get(&group_call_id)?;
        Self::group_sends(call).then(|| Arc::clone(&call.gate) as Arc<dyn ExternalFrameSender>)
    }

    pub(super) fn connected_group_mode(&self, chat_id: i64) -> Option<GroupCallMode> {
        self.groups
            .values()
            .find(|call| call.chat_id == chat_id && call.connected)
            .map(|call| call.mode)
    }

    /// The stream tile's quality; an RTMP stream asks for its unified
    /// video again at once when the least quality it needs changed.
    pub(super) fn set_group_stream_quality(&mut self, chat_id: i64, quality: StreamQuality) {
        let Some((&group_call_id, call)) = self
            .groups
            .iter_mut()
            .find(|(_, call)| call.chat_id == chat_id)
        else {
            return;
        };
        call.stream_quality = Some(quality);
        let sources = call.video_sources.clone();
        if let Err(err) = self.sync_group_videos(group_call_id, &sources) {
            crate::advanced::debug_mode::call_log(&format!(
                "[call] group {group_call_id} stream quality not applied ({err})"
            ));
        }
    }

    pub(super) fn group_stream_quality(&self, chat_id: i64) -> Option<StreamQuality> {
        self.groups
            .values()
            .find(|call| call.chat_id == chat_id)
            .and_then(|call| call.stream_quality)
    }

    /// The media viewer's volume for every stream, now and later.
    pub(super) fn set_streams_volume(&mut self, volume: f32) {
        self.stream_volume = volume;
        for call in self.groups.values() {
            if call.mode.is_stream()
                && let Some(player) = call.sound.as_ref()
            {
                player.audio().gain().set_volume(volume);
            }
        }
    }

    pub(super) fn group_stream_health(
        &self,
        group_call_id: i32,
    ) -> Option<crate::calls::stream_audio::watch::PlaybackHealth> {
        let call = self.groups.get(&group_call_id)?;
        if !call.mode.is_stream() {
            return None;
        }
        call.sound.as_ref().map(|player| player.audio().health())
    }

    /// There is no device output inside the library: reopen Quill's.
    pub(super) fn reopen_group_sound(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        self.open_group_sound(group_call_id)
    }
}

#[path = "group_stream.rs"]
mod stream;

#[cfg(test)]
#[path = "group_tests.rs"]
mod tests;
