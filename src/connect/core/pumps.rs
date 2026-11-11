//! The pumps every ingested update runs once the reducer applied it.
//! What the engine, the microphone or the devices report between updates
//! is pumped per poll instead (`request_tick`): your own level
//! (`pump_call_audio`), the engine's reports, the call media and the
//! connected group calls' transport.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Run every post-ingest pump, even when an earlier one fails to send:
    /// a 1:1 signaling send error must not skip the group-call transport
    /// (whose "before" ids exist only for this update), the call media,
    /// `setTdlibParameters` or the membership probe. The first error is
    /// returned once all have run.
    ///
    /// Each pump here reacts to what the update changed: the 1:1 and group
    /// call ids before it, the auth state, the open chat's membership.
    pub(super) fn run_post_ingest_pumps(
        &mut self,
        active_call_before: Option<i32>,
        bridge_signaling: Option<(i32, Vec<u8>)>,
        group_calls_before: [Option<i32>; 2],
    ) -> Result<(), ConnectSendError> {
        let call = self.pump_call_engine(active_call_before, bridge_signaling);
        let group_call = self.pump_group_call_transport(group_calls_before);
        self.pump_call_media(std::time::Instant::now());
        let parameters = self.maybe_send_parameters();
        let membership = self.probe_membership_everywhere();
        self.claim_account_owner();
        call.and(group_call).and(parameters).and(membership)
    }

    /// The own membership probe for the main window's chat and for every
    /// chat window's, each under its own view (a window's probe waits for
    /// `getMe` like the main one, and runs on whatever answered).
    fn probe_membership_everywhere(&mut self) -> Result<(), ConnectSendError> {
        let mut result = self.maybe_probe_channel_membership();
        let windows: Vec<ChatId> = self.session.window_chats.iter().copied().collect();
        for chat_id in windows {
            let probe =
                self.with_window_chat(chat_id, |driver| driver.maybe_probe_channel_membership());
            result = result.and(probe);
        }
        result
    }

    /// The account's user id is known: a folder last signed in as someone
    /// else drops what they left (`account_data::claim`), once per user.
    fn claim_account_owner(&mut self) {
        let Some(me) = self.session.my_user_id else {
            return;
        };
        let retry = match self.owner_claim {
            OwnerClaim::Claimed(claimed) if claimed == me => return,
            OwnerClaim::Unsaved(claimed) => claimed == me,
            OwnerClaim::Claimed(_) | OwnerClaim::Unclaimed => false,
        };
        // `<app root>/accounts/<key>`.
        let Some(app_root) = self.paths.root.parent().and_then(std::path::Path::parent) else {
            self.owner_claim = OwnerClaim::Unsaved(me);
            return;
        };
        let owner_error = if retry {
            // What the folder held was settled by the first try: only the
            // owner file is left to write.
            crate::account_data::save_owner(app_root, &self.session.account, me).err()
        } else {
            crate::account_data::claim(
                app_root,
                &self.session.account,
                me,
                self.session.sign_in_origin,
            )
            .owner_error
        };
        match owner_error {
            None => self.owner_claim = OwnerClaim::Claimed(me),
            Some(err) => {
                // Not silent: without the owner file the next start cannot
                // tell whose the folder is, so it drops what is in it. The
                // next ingest writes it again; the line shows once.
                if !retry {
                    eprintln!("quill: could not save the account owner: {err}");
                }
                self.owner_claim = OwnerClaim::Unsaved(me);
            }
        }
    }
}
