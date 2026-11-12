//! Cancelling the requests of a session-wide list a change just made
//! out of date.

use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Drop the requests in flight for the list of `purpose`. Used for the
    /// sticker and GIF lists, one per session: once a mutation changed one,
    /// anything still out for it is older than the change and its answer
    /// would carry the old contents, so the answer finds no request and is
    /// ignored. The stage of the list was outdated by the reducer already
    /// and names no request, so the registry is asked
    /// ([`RequestRegistry::supersede`]).
    pub(super) fn drop_in_flight(&mut self, purpose: RequestPurpose) {
        self.session.requests.supersede(purpose);
    }
}
