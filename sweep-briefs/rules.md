# Cloud worker rules

- Merge origin/main into your branch when you start and before your last push. Never rebase or force-push.
- Commit and push after each step. Open no PR and never comment on GitHub.
- Fix each finding at its root, one commit per finding. If a finding turns out wrong, skip it and say why.
- Match Telegram Desktop's behavior where it has one. Fixes must work on macOS, Windows and Linux.
- The UI is a pure function of state from one source of truth. Animation runs on the frame clock.
- docs/contributing/panic-safety.md applies: no unwrap, expect, panic or indexing in src.
- Tests show what the user sees, rely on no sleep, timeout or wall clock, and fail without the fix.
- Comments only for a non-obvious why. Match the code around you. Keep files small, as docs/contributing/structure.md says.
- New UI text goes through the lang system. The app is called Quill in copy.
- Measure before and after where a finding gives a number. Say what could not be measured here.
- Run cargo fmt, clippy with -D warnings for `--features demo-capture --all-targets` and `--no-default-features --all-targets`, and the tests you touched. The chmod 0555 test fails as root, ignore it.
- scripts/hotspots.txt lists shared files. Keep out of them, or add a `Hotspot-change: <reason>` trailer directly above the other trailers.
- Commit trailer: `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. No model names anywhere else.

Last step: push branch claude/prtext-<name> holding one file, PR.md. Line 1 is the PR title, a plain sentence naming what changes for the user. Then the body: what was wrong, what changed, what ran, what did not run, what stays open. No em-dashes, no attribution lines.

Bundles in sweep-briefs/bundles come from a read-only audit of main at 5072e9662. Check each item against the code first, since line numbers can move. The owner's word: "100% of the defects found are fixed", so leave no item out and hold nothing for later. An item you show is not a defect gets its evidence in the PR text. Under "Owner decisions", apply the default. Work on macOS-only code as far as you can read it, name what you could not compile, and keep it small so a Mac check can finish it. Windows code: `cargo check --target x86_64-pc-windows-gnu` where it works. After your last push, delete nothing.
