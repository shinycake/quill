//! Sponsored messages.
use super::*;
use crate::premium_hub::PremiumFeature;

impl Session {
    /// Record a `joinChat` outcome. `Success` flips status optimistically;
    /// `updateChatMember` confirms. The other variants keep the old status and
    /// are logged (the UI shows a fixed note).
    pub fn accept_join_chat_result(&mut self, chat_id: ChatId, result: ChatJoinResult) {
        match result {
            ChatJoinResult::Success { .. } => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.set_member_status(ChannelMemberStatus::Member, None);
                }
                self.note_channel_joined(chat_id);
            }
            other => {
                self.diagnostics.record(Diagnostic {
                    category: "reducer",
                    type_name: Some("joinChat".into()),
                    extra: Some(chat_id.0 as u64),
                    seq: None,
                    note: match other {
                        ChatJoinResult::RequestSent => "join-request-sent",
                        ChatJoinResult::GuardBotApprovalRequired => "join-guard-bot-approval",
                        ChatJoinResult::Declined => "join-declined",
                        ChatJoinResult::Success { .. } => "join-success",
                    },
                });
            }
        }
    }

    /// Store `sponsoredMessages` for a chat (TDLib display order kept; files
    /// remembered for the download sandbox).
    pub fn accept_sponsored_messages(
        &mut self,
        chat_id: ChatId,
        messages: Vec<SponsoredMessage>,
        messages_between: i32,
        files: &[ParsedFile],
    ) {
        self.remember_files(files);
        // A refetch keeps what the user already did with an ad that is
        // still in the list (viewed once, reported/hidden).
        let previous = self
            .messages
            .sponsored
            .remove(&chat_id.0)
            .unwrap_or_default();
        let keep = |ids: &std::collections::HashSet<i64>| -> std::collections::HashSet<i64> {
            ids.iter()
                .copied()
                .filter(|id| messages.iter().any(|m| m.message_id == *id))
                .collect()
        };
        let (viewed, dismissed) = (keep(&previous.viewed), keep(&previous.dismissed));
        // Ads already in the history stay where they are.
        let placed = previous
            .placed
            .into_iter()
            .filter(|spot| messages.iter().any(|m| m.message_id == spot.ad_id))
            .collect();
        self.messages.sponsored.insert(
            chat_id.0,
            ChatSponsoredMessages {
                messages,
                messages_between,
                viewed,
                dismissed,
                fetched_at: Some(std::time::Instant::now()),
                placed,
            },
        );
    }

    /// `chat_id` is no longer kept (`history_prune`): its ads go and are
    /// fetched again when it opens (an answer still on its way lands only
    /// for a chat a view shows). The ids the user reported or hid stay, so
    /// the same ad in the next list stays hidden.
    pub(crate) fn forget_sponsored_messages(&mut self, chat_id: ChatId) {
        let Some(entry) = self.messages.sponsored.get_mut(&chat_id.0) else {
            return;
        };
        if entry.dismissed.is_empty() {
            self.messages.sponsored.remove(&chat_id.0);
        } else {
            *entry = ChatSponsoredMessages {
                dismissed: std::mem::take(&mut entry.dismissed),
                ..ChatSponsoredMessages::default()
            };
        }
    }

    /// Whether `getChatSponsoredMessages` should be sent for `chat_id`: not
    /// after the user hid ads, and not again within five minutes of the last
    /// response (tdesktop `TooEarlyForRequest`).
    pub fn sponsored_fetch_due(&self, chat_id: ChatId, now: std::time::Instant) -> bool {
        if self.messages.sponsored_hidden {
            return false;
        }
        match self
            .messages
            .sponsored
            .get(&chat_id.0)
            .and_then(|entry| entry.fetched_at)
        {
            Some(at) => now.saturating_duration_since(at) >= SPONSORED_REFETCH_AFTER,
            None => true,
        }
    }

    /// How the open chat's ads enter its history. `None` without a fetched
    /// list, after ads were hidden, and in topic views.
    pub fn open_sponsored_layout(&self) -> Option<SponsoredLayout> {
        if self.messages.sponsored_hidden || self.open_topic.is_some() {
            return None;
        }
        let entry = self.messages.sponsored.get(&self.open_chat?.0)?;
        Some(match u32::try_from(entry.messages_between) {
            Ok(between) if between > 0 => SponsoredLayout::BetweenPosts(between),
            _ => SponsoredLayout::AppendAtEnd,
        })
    }

    /// The open chat's ads in its history, in placement order, each with
    /// the message it follows. An ad shows once its media is downloaded
    /// (TDLib: content "must be fully downloaded before the message is
    /// shown"; tdesktop waits for it, `AppendResult::MediaLoading`), and
    /// never after it was reported or hidden.
    pub fn open_sponsored_in_history(&self) -> Vec<(MessageId, &SponsoredMessage)> {
        if self.open_sponsored_layout().is_none() {
            return Vec::new();
        }
        let Some(entry) = self
            .open_chat
            .and_then(|chat| self.messages.sponsored.get(&chat.0))
        else {
            return Vec::new();
        };
        entry
            .placed
            .iter()
            .filter(|spot| !entry.dismissed.contains(&spot.ad_id))
            .filter_map(|spot| {
                let ad = entry.messages.iter().find(|m| m.message_id == spot.ad_id)?;
                self.sponsored_media_ready(ad).then_some((spot.after, ad))
            })
            .collect()
    }

    fn sponsored_media_ready(&self, ad: &SponsoredMessage) -> bool {
        ad.content_file_ids().iter().all(|id| {
            self.media
                .files
                .get(&id.0)
                .is_some_and(|file| file.usable_path().is_some())
        })
    }

    /// The next ad of the open chat's list that is not in its history yet
    /// (tdesktop: the first entry without an item).
    fn next_unplaced_sponsored(&self) -> Option<i64> {
        let entry = self.messages.sponsored.get(&self.open_chat?.0)?;
        entry.messages.iter().map(|ad| ad.message_id).find(|id| {
            !entry.dismissed.contains(id) && !entry.placed.iter().any(|spot| spot.ad_id == *id)
        })
    }

    /// The reader scrolled past the open chat's last message (tdesktop
    /// `setBottomContentRequest` → `SponsoredMessages::append`): the next
    /// ad joins the end of the history. Only for lists without
    /// `messages_between`, fetched within the last five minutes, while the
    /// window holds the latest message. Returns whether an ad was placed.
    pub fn place_sponsored_at_end(&mut self, now: std::time::Instant) -> bool {
        if self.open_sponsored_layout() != Some(SponsoredLayout::AppendAtEnd) {
            return false;
        }
        let Some(chat_id) = self.open_chat else {
            return false;
        };
        if self.history_has_newer(chat_id) || !self.sponsored_list_fresh(chat_id, now) {
            return false;
        }
        let Some(after) = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.values().next_back())
            .map(|message| message.id)
        else {
            return false;
        };
        self.place_sponsored_after(after)
    }

    /// Put the open chat's next unplaced ad after `after` (tdesktop
    /// `inject`, which the UI drives: where an ad goes depends on the rows'
    /// heights). Returns whether an ad was placed.
    pub fn place_sponsored_after(&mut self, after: MessageId) -> bool {
        let Some(ad_id) = self.next_unplaced_sponsored() else {
            return false;
        };
        let Some(entry) = self
            .open_chat
            .and_then(|chat| self.messages.sponsored.get_mut(&chat.0))
        else {
            return false;
        };
        entry.placed.push(SponsoredPlacement { ad_id, after });
        true
    }

    /// Where the open chat's next ad between posts is counted from
    /// (tdesktop `inject`): the last ad placed, else the first unread
    /// message the chat opened at. The flag says whether an ad is placed
    /// already. `None` when no ad is left to place or nothing is unread.
    pub fn sponsored_inject_start(&self) -> Option<(MessageId, bool)> {
        let chat_id = self.open_chat?;
        if !matches!(
            self.open_sponsored_layout(),
            Some(SponsoredLayout::BetweenPosts(_))
        ) {
            return None;
        }
        self.next_unplaced_sponsored()?;
        let entry = self.messages.sponsored.get(&chat_id.0)?;
        if let Some(last) = entry.placed.last() {
            return Some((last.after, true));
        }
        // `history->firstUnreadMessage()`: the message after the last read.
        let history = self.histories.get(&chat_id.0)?;
        let anchor = history.unread_anchor?;
        let first_unread = history
            .messages
            .range((
                std::ops::Bound::Excluded(anchor.0),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .map(|(_, message)| message.id)?;
        Some((first_unread, false))
    }

    /// tdesktop `hasUnshownFor`: an ad of a list without
    /// `messages_between`, fetched within five minutes, still waits to join
    /// the end of the open chat's history (not placed, or placed with its
    /// media still downloading). The bottom of the history is not its end
    /// yet, so the pull to the next channel waits.
    pub fn sponsored_unshown_at_end(&self, chat_id: ChatId, now: std::time::Instant) -> bool {
        if self.open_chat != Some(chat_id)
            || self.open_sponsored_layout() != Some(SponsoredLayout::AppendAtEnd)
            || !self.sponsored_list_fresh(chat_id, now)
        {
            return false;
        }
        let Some(entry) = self.messages.sponsored.get(&chat_id.0) else {
            return false;
        };
        entry
            .messages
            .iter()
            .filter(|ad| !entry.dismissed.contains(&ad.message_id))
            .any(|ad| {
                !entry.placed.iter().any(|spot| spot.ad_id == ad.message_id)
                    || !self.sponsored_media_ready(ad)
            })
    }

    /// tdesktop `TooEarlyForRequest(list.received)`.
    fn sponsored_list_fresh(&self, chat_id: ChatId, now: std::time::Instant) -> bool {
        self.messages
            .sponsored
            .get(&chat_id.0)
            .and_then(|entry| entry.fetched_at)
            .is_some_and(|at| now.saturating_duration_since(at) < SPONSORED_REFETCH_AFTER)
    }

    /// The chat was left (tdesktop `clearItems`): its ads leave the history
    /// and are placed again when it opens.
    pub(crate) fn clear_sponsored_placements(&mut self, chat_id: ChatId) {
        if let Some(entry) = self.messages.sponsored.get_mut(&chat_id.0) {
            entry.placed.clear();
        }
    }

    /// The UI shows these ads on screen (the whole text, button excluded).
    /// Returns the ids not yet counted, marking them viewed so a scroll away
    /// and back never sends a second `viewMessages`. Ids that are not in the
    /// fetched list of an open chat are ignored.
    pub fn take_sponsored_views(&mut self, chat_id: ChatId, shown: &[i64]) -> Vec<i64> {
        if self.open_chat != Some(chat_id) {
            return Vec::new();
        }
        let Some(entry) = self.messages.sponsored.get_mut(&chat_id.0) else {
            return Vec::new();
        };
        let mut due = Vec::new();
        for id in shown {
            if entry.messages.iter().any(|m| m.message_id == *id) && entry.viewed.insert(*id) {
                due.push(*id);
            }
        }
        due
    }

    /// A `viewMessages` send failed: let the next frame retry these ids.
    pub fn untake_sponsored_views(&mut self, chat_id: ChatId, ids: &[i64]) {
        if let Some(entry) = self.messages.sponsored.get_mut(&chat_id.0) {
            for id in ids {
                entry.viewed.remove(id);
            }
        }
    }

    /// The user chose "Hide ads" on a shown ad. Hiding is the Premium
    /// `toggleHasSponsoredMessagesEnabled(false)` setting (tdesktop's
    /// `HideSponsoredClickHandler`). `None`: the ad is gone. `Some(true)`:
    /// Premium, the caller sends the request. `Some(false)`: not Premium,
    /// the "No ads" preview is asked for and nothing is sent.
    pub fn begin_sponsored_hide(&mut self, chat_id: ChatId, message_id: i64) -> Option<bool> {
        self.sponsored_message(chat_id, message_id)?;
        self.messages.sponsored_report = None;
        self.messages.sponsored_report_target = None;
        self.messages.last_sponsored_report = None;
        if self.my_is_premium() {
            return Some(true);
        }
        self.request_no_ads_preview(chat_id);
        Some(false)
    }

    /// tdesktop `ShowPremiumPreviewBox(PremiumFeature::NoAds)`, opened by
    /// the UI on its next frame.
    fn request_no_ads_preview(&mut self, chat_id: ChatId) {
        self.payments.hub.promo_request = Some(crate::premium_hub::PromoRequest {
            feature: PremiumFeature::DisabledAds.into(),
            chat_id,
        });
    }

    /// `toggleHasSponsoredMessagesEnabled(false)` answered `ok`: no ads are
    /// shown or fetched for the rest of the session.
    pub fn accept_sponsored_hidden(&mut self, chat_id: ChatId) {
        self.messages.sponsored_hidden = true;
        self.messages.last_sponsored_report = Some(SponsoredReportOutcome {
            chat_id,
            message_id: 0,
            result: ReportSponsoredResult::AdsHidden,
        });
    }

    /// Sponsored rows for the open chat in TDLib's response order. Empty for
    /// gated chats without a fetch and for chats that never had one.
    pub fn open_sponsored_rows(&self) -> Vec<&SponsoredMessage> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        self.messages
            .sponsored
            .get(&chat_id.0)
            .map(ChatSponsoredMessages::ordered)
            .unwrap_or_default()
    }

    pub fn sponsored_message(&self, chat_id: ChatId, message_id: i64) -> Option<&SponsoredMessage> {
        self.messages
            .sponsored
            .get(&chat_id.0)
            .and_then(|entry| entry.messages.iter().find(|m| m.message_id == message_id))
    }

    /// Begin a `reportChatSponsoredMessage` flow. Returns the request
    /// identifiers when the row exists and `can_be_reported` is set.
    pub fn begin_sponsored_report(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
    ) -> Option<(ChatId, i64)> {
        let reportable = self
            .sponsored_message(chat_id, message_id)
            .is_some_and(|message| message.can_be_reported);
        if !reportable {
            return None;
        }
        self.messages.sponsored_report = None;
        self.messages.sponsored_report_target = Some(SponsoredReportTarget {
            chat_id,
            message_id,
            request: None,
        });
        self.messages.last_sponsored_report = None;
        Some((chat_id, message_id))
    }

    /// The report request went out as `request`: only its answer applies.
    pub fn note_sponsored_report_request(&mut self, request: RequestId) {
        if let Some(target) = self.messages.sponsored_report_target.as_mut() {
            target.request = Some(request);
        }
    }

    /// Apply a `ReportSponsoredResult` for a finished `ReportChatSponsoredMessage`.
    /// Apply a `ReportSponsoredResult` for a finished `ReportChatSponsoredMessage`.
    /// `OptionRequired` arms the option picker; any other result closes it.
    pub fn accept_sponsored_report(
        &mut self,
        pending: &PendingRequest,
        result: ReportSponsoredResult,
    ) {
        // Only the answer to the request the flow waits on: after Cancel (or
        // a newer report) the target is gone or waits on another request,
        // and a late answer re-arms nothing and reports no ad.
        let Some(SponsoredReportTarget {
            chat_id,
            message_id,
            request: Some(request),
        }) = self.messages.sponsored_report_target
        else {
            return;
        };
        if request != pending.id {
            return;
        }
        match result {
            ReportSponsoredResult::OptionRequired { title, options } => {
                self.messages.sponsored_report = Some(SponsoredReportFlight {
                    extra: pending.id,
                    chat_id,
                    message_id,
                    title,
                    options,
                });
            }
            result => {
                self.messages.sponsored_report = None;
                self.messages.sponsored_report_target = None;
                match &result {
                    // The reported ad leaves the history (tdesktop removes
                    // it), also when its list went while the report was on
                    // its way (`forget_sponsored_messages`).
                    ReportSponsoredResult::Ok => {
                        self.messages
                            .sponsored
                            .entry(chat_id.0)
                            .or_default()
                            .dismissed
                            .insert(message_id);
                    }
                    ReportSponsoredResult::AdsHidden => self.messages.sponsored_hidden = true,
                    ReportSponsoredResult::PremiumRequired => {
                        self.messages.last_sponsored_report = None;
                        self.request_no_ads_preview(chat_id);
                        return;
                    }
                    _ => {}
                }
                self.messages.last_sponsored_report = Some(SponsoredReportOutcome {
                    chat_id,
                    message_id,
                    result,
                });
            }
        }
    }

    /// The chat of the ad being reported, from the first request until the
    /// last answer or the picker's Cancel: the UI holds it from pruning so
    /// the option picked still finds the ad.
    pub fn sponsored_report_chat(&self) -> Option<ChatId> {
        let target = self.messages.sponsored_report_target;
        let flight = self.messages.sponsored_report.as_ref();
        target
            .map(|target| target.chat_id)
            .or(flight.map(|flight| flight.chat_id))
    }

    /// Drop the report picker flight and its target. Called when the user
    /// cancels, when a send fails, or when a result is applied elsewhere —
    /// a dismissed report must not attribute a late response to a stale row.
    pub fn dismiss_sponsored_report(&mut self) {
        self.messages.sponsored_report = None;
        self.messages.sponsored_report_target = None;
    }

    pub fn clear_sponsored_report_outcome(&mut self) {
        self.messages.last_sponsored_report = None;
    }
}
