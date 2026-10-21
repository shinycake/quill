# rlottie shape budget patch

Animated stickers drew without some layers (a sticker's eyes, mouth and
waving hand were missing while Telegram showed them). Upstream rlottie
34465a9 added a 15000-point shape-content budget and charges every
keyframed path the 1024-point maximum, whatever its real size. A sticker
with 96 animated paths was charged ~98k, so rlottie dropped the layers it
built last, which are the top ones. Roughly 100 of 220 cached TGS
stickers went over the budget.

Telegram Desktop's rlottie fork predates the budget, so switching to it
would lose upstream's hardening. Pinning before 34465a9 would too. Instead
both build scripts apply `native/patches/rlottie-animated-shape-budget.patch`,
the `lottieitem.cpp` part of upstream's open fix
(https://github.com/Samsung/rlottie/pull/602): animated paths and polystars
cost their largest keyframe point count, still capped at 1024, and the
budget stays at 15000.

Drop the patch when the pin moves past a commit containing #602; `git apply`
fails loudly if it no longer applies. The ignored test
`animated_paths_are_not_dropped_by_the_shape_budget` checks the bundled
library (run with `--ignored` and `QUILL_RLOTTIE_PATH`).
