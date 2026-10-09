# Parity sync 2026-10-09

README `## Status` checklist brought in line with the 2026-10-08 gap audit and the 2026-10-09 refresh.

- Added 412 items in new and existing subsections, each with a unique kebab-case `parity:` id. Existing items and ids are untouched; nothing was unchecked.
- 45 items were checked because the code is present on main (message links and entities, context-menu media actions, report, seen-by, translate, action bars, audio player bar and playlist, passcode, proxy list, storage clear, login alerts, service notifications, presence).
- 367 items are unchecked: open, partial (noted in text), blocked (reason in text) or deferred (low impact).
- Added the scheduled-message and deep-link work-in-progress ids.
- Applied `parity-fragments/codex-secret-capture-block.txt` with `scripts/apply-parity-fragment.sh`.
- Format kept compatible with the dashboard parser (`### ` area headings, `- [ ]`/`- [x]` lines, `(blocked: ...)` wording for blocked items).
- Parity went from 510/525 to 556/937 (59.3%); the drop is the newly listed gaps, not regressions.
