## codex/chat-top-bars (2026-10-08)

Gap-audit batch 8: the bars tdesktop stacks under the chat header.

### What was verified missing
- `chat.action_bar` / `updateChatActionBar` were not parsed at all.
- `updateChatPendingJoinRequests` kept only the count (no requester ids), and nothing showed it outside the group info panel.
- The voice-chat bar existed only for a joined call; a live chat the viewer has not joined had only the header icon.
- Group-call (joined) bar, pinned bar and call bar already existed and are unchanged.

### Design
- `ChatActionBar` (telegram/envelope/chat_action_bar.rs) mirrors schema 1.8.67 (ReportSpam, InviteMembers, ReportAddBlock, AddContact, SharePhoneNumber, JoinRequest). Unknown constructors (for example a newer ReportUnrelatedLocation) parse to "no bar" instead of a wrong one. Stored in `Session::chat_action_bars`, filled from `updateNewChat` and `updateChatActionBar`.
- `src/ui/chat_bars.rs` renders, in tdesktop's order: contact status, join requests, voice chat. Wording follows `history_view_contact_status.cpp`, in sentence case per the gpui-kit design guides:
  - ReportAddBlock: Add contact + Block user (Unarchive + Block user when `can_unarchive`).
  - AddContact: "Add {name} to contacts". SharePhoneNumber: "Share my phone number" with tdesktop's confirmation.
  - ReportSpam: "Report spam and leave" (or Unarchive + Report spam), confirmation text per group/channel.
  - InviteMembers: "Add members" (opens the member dialog) only if the viewer can add members.
  - JoinRequest: explanation text; click opens "Response to your join request"; no close button, as in tdesktop.
  - Close button sends `removeChatActionBar` and drops the bar locally.
- Add contact reuses the existing dialog; it now accepts a name without a phone, since a stranger's number is hidden (TDLib allows an empty phone for a known user).
- Block user opens a box like `PeerMenuBlockUserBox` (title, text, Report spam and Delete this chat checkboxes, both checked as in tdesktop): `setMessageSenderBlockList`, optional `reportChat`, optional `deleteChatHistory(remove_from_chat_list)`.
- Report spam and leave: `reportChat` then `leaveChat`; the bar then vanishes with the chat.
- Unarchive: `addChatToList(main)` + reset notification settings (schema comment on `can_unarchive`).
- Join-requests bar: up to three requester avatars + "N join requests"; shown only when the viewer can invite users. Click opens a box fed by `getChatJoinRequests` with per-row "Add to Group/Channel" / "Dismiss" (`processChatJoinRequest`, existing driver path) and tdesktop's "requested to join today at HH:MM" status.
- Voice-chat bar: for `chat.video_chat` when the viewer is not in a call: "Voice Chat" / "Live Stream", "Active now" or "Click to join", Join button (existing join flow). Participant counts are not in `chat.video_chat`, so none are shown.

### Not done
- Account-info notice of ReportAddBlock (`account_info`: registration date, country), emoji-status notice, business-bot manage bar, translate bar (batch 7 hook), `processChatJoinRequests` bulk and emoji-status/premium header badges. Pinned bar and header ⋮ menu items in the section are untouched.

### Verification
Recorded-JSON tests (state, request shapes, driver). Demo kind `ready-top-bars` with `QUILL_DEMO_BAR=<variant>`: add-block, unarchive-block, add-contact, share-phone, report-spam, unarchive-report, invite, join-request, requests, requests-box, voice, block-box; captured light and dark. No destructive action was clicked.
