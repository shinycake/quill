# Linux: emoji fallback without a color font, Vulkan hint at startup

## What tdesktop does
tdesktop draws every emoji from its own sprite sheets on all platforms, so a missing system emoji font never shows tofu. Its startup has no GPU requirement of this kind (Qt picks a backend).

## What changed
- `emoji_sets_style::font_probe`: on Linux, `fc-list :charset=1f600 color=true family` tells whether a color emoji font exists. When none does and the user has never saved an emoji set, startup downloads and switches to the Noto pack (release emoji-sets-v1). An explicit saved choice, or an unknown answer (no `fc-list`, other platforms), keeps the System set. If the download fails the System set stays and the box still offers retry.
- `startup_failure`: on Linux, a window error mentioning Vulkan, adapter, wgpu or GPU shows install steps (Mesa drivers, NVIDIA driver, lavapipe software fallback) before the raw error. String is `qll_misc_quill_could_not_open_a_gpu`.

## Verified
Unit tests for the probe output parser, the decision function and the error classifier. The Linux-only code paths are cfg-gated; the Linux package build on the cursor/ branch compiles them. Not run on a real minimal Linux install.
