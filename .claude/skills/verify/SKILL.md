---
name: verify
description: Drive the real Quill desktop app (Rust/GPUI Telegram client) and capture a PNG of a feature, to prove a UI change works before calling it ready. Uses the offline screenshot demos (--screenshot-demo) with in-process capture, scripted clicks, hovers and keys. Use on macOS for any UI feature or fix; in a headless Linux cloud session use quill-run instead.
---

# Verify a Quill feature in the real app

Quill has no live-account test path for agents. Every feature is proven through
a **screenshot demo**: the real `QuillApp` and window, started over an injected
offline session (no TDLib, no Telegram account, no network), then captured from
inside the process with `Window::render_to_image`. `screencapture` does not work
here (no Screen Recording permission), so never use it.

## Launch

From the checkout root (builds go to the SSD, see quill-tools/rules.txt):

```bash
bash /Users/idan/Developer/Projects/quill-tools/ssd-target.sh
cargo build --features demo-capture --bin quill
```

Copy the binary out before anything else rebuilds `target/debug/quill` (a core
`cargo test` without features overwrites it):

```bash
OUT=/Volumes/Idan1TB/quill-work/scratch/$(git branch --show-current)/verify
mkdir -p "$OUT" && cp target/debug/quill "$OUT/quill"
```

Run one demo. It opens a window, waits `ready_after_ms`, runs the scripted
input, writes the PNG, drops `$OUT/.quill-ready-<kind>`, then quits on its own:

```bash
KIND=ready-video-controls
QUILL_DEMO_CAPTURE="$OUT/$KIND.png" QUILL_DEMO_WINDOW_SIZE=1200x800 \
  "$OUT/quill" --screenshot-demo "$KIND" "$OUT"
```

Ready means the marker file exists and the PNG is non-empty. Each run gets a
throwaway data root in `$TMPDIR/quill-demo-<pid>`, so it never touches the
user's real Quill settings or accounts.

## Doctor

Before driving, and whenever output looks off:

```bash
"$OUT/quill" --screenshot-demo no-such-kind "$OUT" 2>&1 | head -3   # lists every kind; exit 2
pgrep -fl -- "--screenshot-demo"                                   # no stale demo windows
df -h / /Volumes/Idan1TB | tail -2                                  # 30 GB floor on both
```

If the kind list does not include the demo you expect, the binary is stale or
was built without `--features demo-capture`. Rebuild.

## Drive

Pick the demo kind that puts the feature on screen (see `features/`). If none
exists, add a `DemoSpec` next to the feature's code with `register_demos!`
(src/ui/screenshot_demo.rs) only when it will be reused; otherwise drive an
existing one with input.

Scripted input, `QUILL_DEMO_CLICK` (window points, `;`-separated, run in
order after the window is ready):

| Step | Meaning |
| --- | --- |
| `x,y` | left click |
| `m:x,y` | move the pointer (hover states, tooltips, scrubber bubbles) |
| `dn:x,y` / `dm:x,y` / `up:x,y` | press, drag (button held) and release the left button as separate steps; end on `dm:` to capture a drag mid-flight |
| `mid:x,y` | middle click |
| `r:x,y` | right click (context menus) |
| `k:key` | one key press, GPUI key names (`escape`, `enter`, `space`, `cmd-f`) |
| `t:text` | type text into the focused field, one character at a time (spaces kept) |
| `w:x,y,dx,dy,s\|m\|e[,cmd]` | one phased trackpad scroll event (`cmd` holds Cmd, Ctrl off macOS) |
| `z:x,y,delta` | one trackpad pinch centred on the point (positive `delta` zooms in) |
| `p:ms` | pause |

Other switches: `QUILL_DEMO_THEME=dark|high-contrast`,
`QUILL_DEMO_WINDOW_SIZE=WxH`, `QUILL_DEMO_LINGER_MS`,
`QUILL_DEMO_CAPTURE_WINDOWS=1` (also writes `<png>.window<N>.png` for PiP and
call windows), `QUILL_DEMO_CLICK_LOG=path`. Feature-specific `QUILL_DEMO_*`
flags are documented in the demo file that reads them (`rg QUILL_DEMO_ src`).

Coordinates are logical points; the PNG is 2x on Retina. Find a target's point
by reading the PNG and halving pixel positions. Crop with
`sips -c H W --cropOffset Y X file.png`.

## Evidence

A proof is the PNG of the state after the user action, read back with the Read
tool and checked against the feature's expected end state, in light and dark.
For a click path, capture before and after (two runs, one without
`QUILL_DEMO_CLICK`). State in the report what the capture could not show:
real TDLib data, OS full screen, real video decode on other platforms, sound.

Evidence stays in `$OUT` for the review. Never commit screenshots and never
capture every demo kind (owner rule).

## Cleanup

Demos quit themselves. If a run hangs, kill only the PID you started
(`kill <pid>`), never by process name. When the work is done, delete
`$OUT` and the scratch dir, and clean the target per quill-tools/rules.txt.
