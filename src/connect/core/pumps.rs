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
        let now = std::time::Instant::now();
        let call = self.pump_call_engine(active_call_before, bridge_signaling, now);
        let group_call = self.pump_group_call_transport(group_calls_before, now);
        self.pump_call_media(now);
        let parameters = self.maybe_send_parameters();
        let membership = self.probe_membership_everywhere();
        self.claim_account_owner(now);
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
    pub(in crate::connect) fn claim_account_owner(&mut self, now: std::time::Instant) {
        let Some(me) = self.session.my_user_id else {
            return;
        };
        let retrying = match self.owner_claim {
            OwnerClaim::Saved(user) if user == me => return,
            OwnerClaim::Unsaved { user, retry_at } if user == me => {
                if now < retry_at {
                    return;
                }
                true
            }
            _ => false,
        };
        // `<app root>/accounts/<key>`.
        let Some(app_root) = self.paths.root.parent().and_then(std::path::Path::parent) else {
            return;
        };
        let owner_error = if retrying {
            crate::account_data::write_owner(app_root, &self.session.account, me).err()
        } else {
            crate::account_data::claim(
                app_root,
                &self.session.account,
                me,
                self.session.sign_in_origin,
            )
            .owner_error
        };
        self.owner_claim = match owner_error {
            None => OwnerClaim::Saved(me),
            Some(err) => {
                // Not silent: without the owner file the next start cannot
                // tell whose the folder is, so it drops what is in it. The
                // write is retried after `OWNER_RETRY`; it is reported
                // once, not on every attempt while the folder cannot be
                // written.
                if !retrying {
                    eprintln!("quill: could not save the account owner: {err}");
                }
                OwnerClaim::Unsaved {
                    user: me,
                    retry_at: now + OWNER_RETRY,
                }
            }
        };
    }
}
