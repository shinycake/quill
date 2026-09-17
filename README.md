# quill

Independent, keyboard-friendly **Telegram desktop client** written in Rust (**GPUI Kit + official TDLib**). Working name **Quill**. Not a ZapFast fork.

Phase 0 is in progress: synthetic GPUI chat, ordered tdjson bridge, replay reducers. **Live Telegram login is disabled.**

## Build

See [docs/build.md](docs/build.md). Short version:

```bash
cargo test --no-default-features
cargo run --features ui          # synthetic chat; no network login
```

Toolchain: Rust **1.98.1**. UI pin: **gpui-kit 0.6.1**. TDLib schema: **1.8.67** (`d1085f9cebc5a62379991ae1652673954f229c1f`).

## Status

- [x] GPUI Kit hello-world shell + synthetic mixed-height chat / composer
- [x] Official tdjson ordered receive bridge (no raw-response logging)
- [x] Auth / chat / history / send reducers with replay tests
- [ ] Live login (needs owner `api_id` / `api_hash`)
- [ ] VoiceOver pass on macOS
- [ ] Channels / bots (blocked on sponsored-content implementation)

Decisions, pins, and blockers: [DECISIONS.md](DECISIONS.md).  
What credentials are needed next: [docs/credentials.md](docs/credentials.md).  
Phase 0 UI proof (real window, not a generated still): [docs/screenshots](docs/screenshots).
