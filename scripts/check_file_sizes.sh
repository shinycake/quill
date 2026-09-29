#!/usr/bin/env bash
# check_file_sizes.sh — no dumping grounds.
# Fails if any .rs file under src/, crates/, or tests/ exceeds the hard cap, or if a waived file grows
# past its recorded line count. Waivers only shrink: each split PR lowers
# (or removes) its entry here.
set -euo pipefail
cd "$(dirname "$0")/.."

HARD_CAP=2000

# "path:lines" — grandfathered violators, recorded 2026-09-29.
# Waiver-bump rule (Idan 2026-09-29): a baseline may grow ONLY for
# type-coherent extensions that cannot live in a named module (new enum
# variants, new match arms on existing matches, new methods on existing
# types); the bump is exact and the PR body says why the code can't live
# elsewhere.
WAIVERS=(
  "src/ui/mod.rs:49279"
  "src/connect.rs:24949"
  "src/state.rs:21863"
  "src/telegram/envelope.rs:18722"
  "src/telegram/requests.rs:10531"
  # Slice S17 (2026-09-29, merge-pipeline rebase post-#177): type-coherent
  # extensions only — ui/mod.rs +11 (49090 → 49101: peer-activity label
  # lookup on `conversation_header` + 3 "typing…" label swaps to
  # `activity_label.unwrap_or("typing…")` + 1 gate-widening line + 2-line
  # comment (review fixup: sticker-picking sets no typing senders, so the
  # header gate must be `typing || activity_label.is_some()`));
  # state.rs +63 (21658 → 21721:
  # `choosing_sticker_senders` field + init on `ChatSummary`,
  # `set_sender_action` routing match, `peer_activity_label` method,
  # `sidebar_preview` label swap, `chat_action_choosing_sticker_label`
  # test); envelope.rs +4 (18600 → 18604: `ChatAction::ChoosingSticker`
  # variant + parse arm).
  # Slice A10 (2026-09-29): type-coherent extensions only — ui/mod.rs +3
  # (`mod auth_recovery` decl, `QuillApp` recovery fields + ctor init,
  # `recovery_mode` reset + wait-password screen arms); connect.rs +44
  # (`request_password_recovery` / `submit_recovery_code` driver methods on
  # the existing `Connect` type + 2 import lines); state.rs +20
  # (`RequestPurpose` variants, `is_auth_submit` arms incl. making it `pub`
  # for the moved test, `AuthRequestError::user_message` arms);
  # requests.rs +27 (two builder fns on existing patterns). A10's three
  # tests live in `src/auth.rs`, not in waived files. (Corrects the A10
  # ui/mod.rs waiver: the A10 fixup commit removed 16 net lines after the
  # 48767 measurement; true post-A10 count is 48751.)
  # Slice S14 (2026-09-29): type-coherent extensions only — state.rs +19
  # (story-restriction `RequestPurpose` variant + reducer match arms +
  # `user_message` arms), envelope.rs +17 (restriction-notice payload
  # parsing arms on the existing `EnvelopePayload` match).
  # Slice S3 (2026-09-29, rebased): type-coherent extensions only —
  # ui/mod.rs +67 (privacy overlay + per-rule editor + exception-picker
  # render arms on the existing settings-surface match); connect.rs +193
  # (get/set privacy-rule + blocked-sender driver methods on the existing
  # `Connect` type); state.rs +166 (`PrivacyRuleDetail`/`PrivacyKeyState`
  # types + reducer match arms); envelope.rs +63 (privacy-rule payload
  # parsing arms on the existing `EnvelopePayload` match).
  # Slice S13 (2026-09-29, rebased post-S3): type-coherent extensions only —
  # connect.rs +111 (`set_story_custom_emoji_reaction` driver method on the
  # existing `Connect` type + `driver_story_custom_emoji_reaction_gates`
  # test in the private `mod tests` harness); state.rs +1
  # (`chosen_reaction_extra` field init in existing fixture); envelope.rs
  # +39 / requests.rs +36 (custom-emoji + paid chosen-reaction parse arms
  # on the existing matches; ~85 lines of public-API-only tests moved to
  # `tests/story_reactions.rs`).
  # Slice G9 (2026-09-29, rebased post-S3/S13): type-coherent extensions only —
  # ui/mod.rs +66 (community service-message row dispatch arms +
  # `show_sender` arms on the existing service-row match); envelope.rs +55
  # (`messageChatJoinFromCommunity` / `messageChatAddedToCommunity` /
  # `messageChatRemovedFromCommunity` parse arms on the existing
  # `EnvelopePayload` match + 2 tests in the existing
  # `mod channel_envelope_tests` — `parse_message` is private).
  # Slice S4 (2026-09-29, rebased post-S3/S13/G9): type-coherent extensions
  # only — connect.rs +142 (auto-download / less-data / storage-stats
  # driver methods on the existing `Connect` type); state.rs +105
  # (`DataStorageState` fields + reducer match arms); envelope.rs +55
  # (storage-statistics payload parsing arms on the existing match).
  # Slice S15 (2026-09-29, review fixup): type-coherent extensions only —
  # state.rs +32 (`UpdateInstalledStickerSets` reducer match arm +
  # `apply_installed_sticker_set_order` method on `Session`);
  # envelope.rs +25 (`UpdateInstalledStickerSets` enum variant + parse arm
  # on the existing `parse_payload` match); requests.rs +7
  # (`send_sticker` options wiring inside the existing function — the slice's
  # core behavior, cannot live elsewhere); ui/mod.rs +1
  # (`..SendOptions::default()` in the existing `composer_send_options`
  # literal — required to cover the new field, text sends keep default
  # false). All S15 tests moved to `tests/sticker_dynamic_order.rs`.
  # Slice communities-search-filter (2026-09-29, merge-pipeline rebase
  # post-#180): type-coherent extensions only — requests.rs +14
  # (10517 → 10531: `community_filter: Option<i64>` param + inline
  # chat_type_filter constructor in `search_messages` — an edit to the
  # existing function only, no new functions); connect.rs +27
  # (24922 → 24949: `set_search_community_filter` method on the existing
  # `Connect` type + import + call-site wiring); state.rs +9
  # (21854 → 21863: `SearchState::community_filter` field + Default init +
  # resets); ui/mod.rs +75 (49204 → 49279: `search_community_filter_chips`
  # render fn + `set_search_community_filter` handler methods on the
  # existing app type + chips-row hook in `search_results`). All new tests
  # live in `tests/community_search_filter.rs`.
  "src/calls/engine.rs:3212"
  "tests/replay.rs:3505"
)

waiver_for() {
  local f="$1" w
  for w in "${WAIVERS[@]}"; do
    [[ "${w%%:*}" == "$f" ]] && { echo "${w##*:}"; return 0; }
  done
  return 1
}

fail=0
while IFS= read -r f; do
  lines=$(wc -l < "$f")
  if cap=$(waiver_for "$f"); then
    if (( lines > cap )); then
      echo "FAIL: $f has $lines lines, over waived $cap (waivers only shrink)"
      fail=1
    fi
  elif (( lines > HARD_CAP )); then
    echo "FAIL: $f has $lines lines, over hard cap $HARD_CAP (split it into named modules)"
    fail=1
  fi
done < <(find src crates tests -name '*.rs' | sort)

(( fail == 0 )) && echo "file sizes OK"
exit "$fail"
