# Working on Quill

Rules every person and coding agent follows in this repository.

- **No implicit state machines.** When something moves through stages
  (a request, a box, a call, a send, a player), the stages are one enum
  value, never a set of bools and options kept in step by hand. Every
  stage has a way out (the answer, the error, a cancel, the owner going
  away), every answer is matched to what asked, and state lives with its
  owner: `OnScreen` per account, `ChatUi` per chat, the `WindowSlot` per
  window. Read docs/contributing/state-machines.md before you add state.
  CI (`scripts/check-implicit-state.py`) rejects new stage flags such as
  `loading: bool` or `pending: bool`.
- **Never crash.** docs/contributing/panic-safety.md: no new
  `unwrap`/`expect`/`panic!`, unchecked indexing or long-held borrows.
- **Where code goes.** docs/contributing/structure.md: file size limit,
  hotspot files, and how to add features without editing shared files.
- **Tests are deterministic.** No sleeps, timeouts or wall-clock waits in
  tests; use the manual clocks and fakes.
- **Verify UI changes in the real app** with the offline screenshot demos
  (.claude/skills/verify).

Before pushing, run the CI checks locally:

```bash
python3 scripts/check-implicit-state.py
python3 scripts/check-screen-state.py
python3 scripts/check-window-scope.py
bash scripts/check-file-size.sh
```
