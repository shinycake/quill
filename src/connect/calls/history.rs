//! Connect driver: call debug logs, call history, call privacy and busy-call handling.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    pub(crate) fn call_debug_information(&self) -> Result<String, ConnectSendError> {
        let summary = self
            .session
            .call_summary
            .as_ref()
            .filter(|summary| summary.need_debug_information && !summary.debug_information_sent)
            .ok_or(ConnectSendError::InvalidRequest)?;
        Ok(self.call_log_payload(summary).to_string())
    }

    /// Phase C2i: the local call-log payload shared by
    /// `sendCallDebugInformation` (inline text) and `sendCallLog` (the
    /// same text as a file). The honest local record: app/engine
    /// identity, call outcome, transport states — never invented media
    /// stats.
    fn call_log_payload(&self, summary: &crate::state::CallSummary) -> serde_json::Value {
        let engine_available = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available());
        let transport = summary.final_transport.map(|state| match state {
            TransportState::Connecting => "connecting",
            TransportState::Reconnecting => "reconnecting",
            TransportState::Connected => "connected",
            TransportState::Failed => "failed",
            TransportState::Closed => "closed",
        });
        let mut payload = serde_json::json!({
            "app": env!("CARGO_PKG_NAME"),
            "app_version": crate::version::APP,
            "os": std::env::consts::OS,
            "engine_available": engine_available,
            "call_id": summary.call_id,
            "duration_secs": summary.duration_secs,
            "had_audio": summary.had_audio,
            "final_transport_state": transport,
            "reconnect_attempts": summary.reconnect_attempts,
            "muted": summary.muted,
            "microphone_device_id": self.selected_devices.0,
            "speaker_device_id": self.selected_devices.1,
        });
        if engine_available {
            let protocol = self
                .call_engine
                .as_ref()
                .expect("available engine")
                .protocol();
            payload["engine_protocol"] = serde_json::json!({
                "udp_p2p": protocol.udp_p2p,
                "udp_reflector": protocol.udp_reflector,
                "min_layer": protocol.min_layer,
                "max_layer": protocol.max_layer,
                "library_versions": protocol.library_versions,
            });
        }
        payload
    }

    /// Phase C2d: upload real local call diagnostics for the last discarded
    /// call (`schema/td_api.tl:14237`).
    pub fn send_call_debug_information(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = self
            .session
            .call_summary
            .as_ref()
            .map(|summary| summary.call_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let debug_information = self.call_debug_information()?;
        let extra = self
            .session
            .request(RequestPurpose::SendCallDebugInformation, None);
        if let Err(err) = self.sender.send_json(&send_call_debug_information(
            extra,
            call_id,
            &debug_information,
        )) {
            self.session.requests.take(extra);
            if let Some(summary) = self.session.call_summary.as_mut() {
                summary.debug_information_error = Some(match err {
                    ConnectSendError::InvalidRequest => {
                        "Could not upload diagnostics: invalid request".into()
                    }
                    ConnectSendError::Native => {
                        "Could not upload diagnostics: TDLib send failed".into()
                    }
                    // MED4: caption-length errors can't arise from a
                    // diagnostics upload; categorized as invalid request.
                    ConnectSendError::CaptionTooLong { .. }
                    | ConnectSendError::TextTooLong { .. } => {
                        "Could not upload diagnostics: invalid request".into()
                    }
                });
            }
            return Err(err);
        }
        if let Some(summary) = self.session.call_summary.as_mut() {
            summary.debug_information_sent = true;
            summary.debug_information_error = None;
        }
        Ok(extra)
    }

    /// Phase C2i: `sendCallLog` (schema 1.8.67 :14240) — uploads the
    /// ended call's log file. The file is the local diagnostics
    /// payload written under the account's exports dir (schema allows
    /// only `inputFileLocal` / `inputFileGenerated`).
    pub fn send_call_log(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let summary = self
            .session
            .call_summary
            .as_ref()
            .filter(|s| s.need_log && !s.log_sent)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let log_text = self.call_log_payload(summary).to_string();
        let call_id = summary.call_id;
        let path = self.paths.exports.join(format!("call-{call_id}.log"));
        std::fs::create_dir_all(&self.paths.exports).map_err(|_| ConnectSendError::Native)?;
        std::fs::write(&path, log_text).map_err(|_| ConnectSendError::Native)?;
        let extra = self.session.request(RequestPurpose::SendCallLog, None);
        let path_str = path.to_string_lossy().into_owned();
        if let Err(err) = self
            .sender
            .send_json(&send_call_log(extra, call_id, &path_str))
        {
            self.session.requests.take(extra);
            if let Some(summary) = self.session.call_summary.as_mut() {
                summary.log_error = Some("Could not upload the call log".into());
            }
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2i: `searchCallMessages` (schema 1.8.67 :11903) — first
    /// page of the server-side recent-calls list. Called when the
    /// Recent-calls tab opens.
    pub fn fetch_call_history(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.recent_calls.clear();
        self.session.recent_calls_offset.clear();
        self.session.recent_calls_error = false;
        self.fetch_call_history_page()
    }

    /// "Clear all" on the Calls list: `deleteAllCallMessages`. The cached
    /// list empties when TDLib answers `ok`, never before. `Ok(None)`:
    /// nothing to clear, or a clear already in flight.
    pub fn clear_call_history(
        &mut self,
        revoke: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !crate::chatlist_calls::can_clear(
            self.session.recent_calls.len(),
            self.session.recent_calls_clearing,
        ) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteAllCallMessages, None);
        if let Err(err) = self
            .sender
            .send_json(&delete_all_call_messages(extra, revoke))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.recent_calls_clearing = true;
        Ok(Some(extra))
    }

    /// Phase C2i: next `searchCallMessages` page, continuing from the
    /// stored `next_offset`.
    pub fn fetch_more_call_history(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.fetch_call_history_page()
    }

    fn fetch_call_history_page(&mut self) -> Result<RequestId, ConnectSendError> {
        if self.session.recent_calls_loading {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SearchCallMessages, None);
        let offset = self.session.recent_calls_offset.clone();
        if let Err(err) = self
            .sender
            .send_json(&search_call_messages(extra, &offset, 40))
        {
            self.session.requests.take(extra);
            self.session.recent_calls_error = true;
            return Err(err);
        }
        self.session.recent_calls_loading = true;
        Ok(extra)
    }

    /// Phase C2i: fetch both call privacy settings
    /// (`userPrivacySettingAllowCalls` /
    /// `userPrivacySettingAllowPeerToPeerCalls`, schema 1.8.67
    /// :15620).
    pub fn fetch_call_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.call_privacy_loading = true;
        self.session.call_privacy_error = false;
        for setting in [
            CallPrivacySetting::AllowCalls,
            CallPrivacySetting::PeerToPeer,
        ] {
            let extra = self.session.request(
                RequestPurpose::Calls(CallsPurpose::GetCallPrivacyRules { setting }),
                None,
            );
            if let Err(err) = self
                .sender
                .send_json(&get_user_privacy_setting_rules(extra, setting))
            {
                self.session.requests.take(extra);
                self.session.call_privacy_loading = false;
                self.session.call_privacy_error = true;
                return Err(err);
            }
            self.session.call_privacy_pending += 1;
        }
        Ok(())
    }

    /// Phase C2i: change a call privacy setting (schema 1.8.67
    /// :15617). Applied optimistically; the `ok` / error response
    /// confirms or clears it.
    pub fn set_call_privacy(
        &mut self,
        setting: CallPrivacySetting,
        who: PrivacyWho,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::SetCallPrivacyRules { setting }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_user_privacy_setting_rules(extra, setting, who))
        {
            self.session.requests.take(extra);
            self.session.call_privacy_loading = false;
            self.session.call_privacy_error = true;
            return Err(err);
        }
        match setting {
            CallPrivacySetting::AllowCalls => self.session.call_privacy_allow_calls = Some(who),
            CallPrivacySetting::PeerToPeer => self.session.call_privacy_p2p = Some(who),
        }
        self.session.call_privacy_loading = true;
        self.session.call_privacy_pending += 1;
        Ok(extra)
    }

    /// Phase C1: drain `Session::call_busy_decline_queue` — incoming
    /// calls that arrived while another call was active are declined
    /// (busy) with `discardCall`. Called from `ingest`.
    /// Phase C2i: the declined peer is recorded in
    /// `Session::call_busy_declined` so the UI can say so honestly
    /// instead of declining silently (TDLib has no hold/swap API —
    /// hold-and-answer is not possible).
    pub(crate) fn maybe_decline_busy_calls(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<(i32, i64, bool)> =
            std::mem::take(&mut self.session.call_busy_decline_queue);
        for (call_id, user_id, is_video) in queued {
            let extra = self.session.request(RequestPurpose::DiscardCall, None);
            if let Err(err) = self
                .sender
                .send_json(&discard_call_request(extra, call_id, false, 0, is_video))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            // ponytail: cap the banner list — it is drained by the UI.
            if self.session.call_busy_declined.len() < 4 {
                self.session.call_busy_declined.push((user_id, is_video));
            }
        }
        Ok(())
    }
}
