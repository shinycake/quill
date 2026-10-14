---
name: quill-run
description: Launch the Quill app in a Claude Code cloud session (headless Linux, no GPU) and screenshot it, to see a UI change working in the real app. Covers the synthetic screenshot demos, the virtual X display, the software Vulkan driver and screen capture. Use when asked to run, start or screenshot Quill in the cloud.
---

# Running Quill headless in a cloud session

The SessionStart hook (`.claude/hooks/session-start.sh`) installs Xvfb, Mesa's
software Vulkan driver and ImageMagick, and exports
`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`. If that variable is
missing, set it yourself: GPUI needs Vulkan and the container has no GPU.

## Build

```bash
cargo build --features demo-capture --locked --bin quill
```

## Screenshot a demo fixture

Every `ui::ScreenshotDemo` kind opens a synthetic, offline window (no Telegram
account, no TDLib). An unknown kind prints the full list. Once the window is
ready, the app writes `<out>/.quill-ready-<kind>`.

`QUILL_DEMO_CAPTURE` (the in-app capture) does not work on Linux:
`render_to_image` is not implemented for this platform. Capture the virtual
display instead:

```bash
KIND=ready-chat-list-search
OUT="$(mktemp -d)"
Xvfb :99 -screen 0 1400x900x24 >/dev/null 2>&1 & XVFB=$!
sleep 1
DISPLAY=:99 ./target/debug/quill --screenshot-demo "$KIND" "$OUT" >/dev/null 2>&1 & APP=$!
for _ in $(seq 1 120); do [ -f "$OUT/.quill-ready-$KIND" ] && break; sleep 1; done
sleep 2   # let the first frames settle
DISPLAY=:99 import -window root "$OUT/$KIND.png"
kill $APP $XVFB
```

Then read the PNG to check the UI. Useful env vars: `QUILL_DEMO_THEME=dark`,
`QUILL_DEMO_WINDOW_SIZE=1200x1100`, `QUILL_REDUCED_MOTION=1`.

The "DRI3" libEGL warnings on stderr are harmless.

## Live Telegram

A live connection needs `TELEGRAM_API_ID`, `TELEGRAM_API_HASH` and a built
`libtdjson` (`scripts/build-tdlib.sh`, `QUILL_TDJSON_PATH`); see
`docs/credentials.md`. Sign-in also needs a phone code, so it is not something
a cloud session can finish on its own. Prefer the demo fixtures and the
headless UI tests (`cargo test --features demo-capture --locked --bin quill`).
