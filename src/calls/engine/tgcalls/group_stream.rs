//! A stream's answers to tgcalls (`qc_group_answer_*`): the connect side
//! loads what the requests in `group_callbacks.rs` ask for and answers
//! here, on its own thread. An answer goes only to the instance that
//! asked: one replaced by a rejoin (or gone with its call) dropped its
//! open requests, so the answer is dropped too.
use super::*;
use quill_calls_sys::{
    QC_BROADCAST_PART_NOT_READY, QC_BROADCAST_PART_RESYNC_NEEDED, QC_BROADCAST_PART_SUCCESS,
};

/// tgcalls' `BroadcastPart::Status` for an answer.
pub(super) fn part_status(status: BroadcastPartStatus) -> i32 {
    match status {
        BroadcastPartStatus::Success => QC_BROADCAST_PART_SUCCESS,
        BroadcastPartStatus::NotReady => QC_BROADCAST_PART_NOT_READY,
        BroadcastPartStatus::ResyncNeeded => QC_BROADCAST_PART_RESYNC_NEEDED,
    }
}

impl TgcallsEngine {
    /// The live instance that gave out `request` in `chat_id`'s call.
    fn asking_instance(
        &self,
        chat_id: i64,
        request: BroadcastRequestId,
    ) -> Option<Native<qc_group>> {
        self.groups
            .values()
            .find(|call| call.chat_id == chat_id && call.context.instance == request.instance)
            .map(|call| call.handle)
    }

    pub(in super::super) fn answer_group_time(
        &mut self,
        chat_id: i64,
        request: BroadcastRequestId,
        timestamp_ms: i64,
    ) {
        let Some(handle) = self.asking_instance(chat_id, request) else {
            return;
        };
        // SAFETY: a live instance; the library ignores an id it no longer
        // has open.
        unsafe {
            (self.api.qc_group_answer_current_time)(handle.as_ptr(), request.id, timestamp_ms)
        };
    }

    pub(in super::super) fn answer_group_part(
        &mut self,
        chat_id: i64,
        answer: &BroadcastPartAnswer,
    ) {
        let Some(handle) = self.asking_instance(chat_id, answer.request) else {
            return;
        };
        // SAFETY: a live instance; the data pointer is null with length 0,
        // or `answer.data` (non-null even when empty) for its length, read
        // and copied before this synchronous call returns. The library
        // ignores an id it no longer has open.
        unsafe {
            (self.api.qc_group_answer_broadcast_part)(
                handle.as_ptr(),
                answer.request.id,
                part_status(answer.status),
                answer.timestamp_ms,
                answer.response_time,
                answer.native_data_ptr(),
                answer.native_data_len(),
            );
        }
    }
}
