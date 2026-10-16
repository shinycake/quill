# CI launch smoke (Linux and Windows)

Quality sweep finding P1: no CI job ever opened a window on Linux or Windows. Packaging
only ran `--version`, `--build-info`, library loading and `--video-probe`, so a startup
crash or blank window could ship unnoticed.

## What changed

Both jobs run on pushes only (main and `cursor/**`). Pull requests keep the Linux check
and nothing else, as before. Neither is a required check.

- Linux: three steps at the end of the `linux` job in `ci.yml` (skipped on pull requests).
  It reuses the `target/` the headless UI tests just built, so only the binary link is
  new. `scripts/ci-launch-smoke.sh` starts Xvfb, points Vulkan at Mesa lavapipe, and for
  each of `ready-chats`, `ready-chats-composer` and `ready-chat-list-search` runs
  `quill --screenshot-demo <kind>`. It waits for the `.quill-ready-<kind>` marker,
  screenshots the virtual display with ImageMagick `import` (in-app `render_to_image`
  is not implemented for the X11 backend), and fails on a missing marker, a non-zero exit
  or a flat picture. PNGs are uploaded as `launch-smoke-linux`.
- Windows: a separate `windows-launch-smoke` job in `ci.yml`. The existing Windows
  build is `--release --features ui`, which has no `demo-capture`, and shipping it with
  `demo-capture` would change the release binary, so the job builds a debug
  `demo-capture` binary with its own rust-cache. `scripts/ci-launch-smoke.ps1` runs
  the same fixtures with `QUILL_DEMO_CAPTURE=<png>` (the Windows backend implements
  `render_to_image`), so the picture is the app's own frame. PNGs are uploaded as
  `launch-smoke-windows`.
- `scripts/ci-image-check.py` (std-only Python, reads PPM or 8-bit PNG) is the blank
  check: at least 64 distinct colours and no colour covering more than 90% of pixels.

## Why WARP works without extra code

The vendored Windows backend (`third_party/gpui-pre-windows/src/directx_devices.rs`)
walks every DXGI adapter and takes the first that creates a D3D11 device with the
required features. Hosted Windows runners expose the Microsoft Basic Render Driver (WARP)
as an adapter, so no code change is needed.

## Not done

- The Wayland job (weston or sway headless) and the "kill the GPU device" env hook from
  the finding are not part of this change.
- A red-on-purpose run. The blank-picture check was exercised locally instead: it
  accepts the Windows capture (2132 colours, dominant share 0.60) and rejects a flat white image.

## How verified

Pushed to `cursor/ci-launch-smoke` (run https://github.com/shinycake/quill/actions/runs/38861788212):
`linux-fmt-clippy-test` and `windows-launch-smoke` both green on the first run. The uploaded
`launch-smoke-linux` and `launch-smoke-windows` artifacts hold real Quill windows (chat list,
messages, composer) for all three fixtures. Windows rendered through DXGI with no GPU on the
hosted runner, so WARP works unmodified.
