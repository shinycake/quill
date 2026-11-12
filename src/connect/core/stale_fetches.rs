//! Cancelling the requests of a session-wide list a change just made
//! out of date.

use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Drop the request in flight for `purpose`. Used for the sticker and
    /// GIF lists, one per session: once a mutation changed one, anything
    /// still out for it is older than the change and its answer would carry
    /// the old contents, so the answer finds no request and is ignored.
    pub(super) fn drop_in_flight(&mut self, purpose: RequestPurpose) {
        drop(self.session.requests.take_purpose(purpose));
    }
}
