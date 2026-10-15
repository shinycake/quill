# Compatible crate updates (October 2026)

## What tdesktop does

Nothing relevant. This is Quill's own dependency policy (`docs/dependency-updates.md`).

## What changed

`cargo update`, `Cargo.lock` only. 25 crates move within their semver range, among them
`rustls` 0.23.46, `tokio` 1.53.3, `serde_json` 1.0.154, `syn` 3.0.9, `sha2` 0.11.1,
`zerocopy` 0.8.63 and `cc` 1.8.0. `proptest` 1.12 (only reached through GPUI) brings
`proptest-macro` 0.6 and `rand_xorshift` 0.5.

`gpui-component-macros` (0.7.3 available) and `gpui-kit-assets` (0.7.2) were set back to
0.7.1 with `cargo update --precise`. They belong to the `=`-pinned GPUI Kit family and
move with the GPUI Kit 0.7.3 bump, not on their own. The `gpui*` entries are otherwise
unchanged.

The majors that `scripts/deps-watch.py` lists (`chacha20poly1305`, `pbkdf2`, `cpal` with
`rodio`, `moxcms`, `x11rb`, `core-video`) are left for their own PRs or held.

## How verified

- `cargo clippy --no-default-features --all-targets --locked -- -D warnings`: clean.
- `cargo clippy --features demo-capture --all-targets --locked -p quill -p ntgcalls-sys -p quill-webview-protocol -- -D warnings`: clean.
- `cargo test --no-default-features --locked --no-fail-fast`: all pass except
  `atomic_write_failure_keeps_old_file`, which fails only when the tests run as root (the
  cloud container) and passes when the same binary runs as an unprivileged user.
- `cargo deny --locked check licenses` and `scripts/third-party-licenses.sh --check`.
