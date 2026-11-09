# Changes made by the Quill project

This directory is what `scripts/vendor-widgets.sh` copies from the upstream
commits in `VERSION`: widget-sandbox (no license file published at this tag,
see the decision doc), its pinned QuickJS fork (MIT, `quickjs/LICENSE`) and
twidget (BSD-3-Clause, `twidget/LICENSE`). The script then
applies `native/patches/twidget-quill.patch`, which is every change to the
upstream files; changed code is marked `QUILL`. This file stays across a bump.
Background: `docs/decisions/message-widgets.md`.

## Changes

- `twidget/core/src/renderer.cpp`: a frozen widget gets no badge when the
  host's reason is empty. Quill draws its own "Widget stopped" pill in the
  middle of the widget, as Telegram Desktop does, and the renderer's badge
  would sit under the message time in the bottom corner.

## Added by the script

- `twidget/generated/tw_ua_stylesheet.h`: the profile's UA layer from
  widget-sandbox's `packages/core/profile.json`, as twidget's CMakeLists.txt
  generates it at configure time.

Quill's glue (`native/widget/quill_widget.cpp`) and Rust backend
(`src/ui/widget_runtime/`) are Quill's own code under its MIT license.
