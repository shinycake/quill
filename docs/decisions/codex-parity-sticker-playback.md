# Animated and video stickers

History and picker use the same bounded decoder/cache. TGS uses the pinned Samsung rlottie C API; WebM uses the existing cancellable ffmpeg extraction pipeline with libvpx-vp9 to preserve transparency. GPUI advances the decoded image frames and honors Reduce Motion. Loop Animated Stickers defaults on; disabling it plays once and holds the final frame. The preference persists with MediaPrefs.

Limits: 128px surfaces, 120 frames, 30 seconds, sixteen resident clips, two decoders. Chat/account switches and app teardown cancel outstanding jobs. TGS gzip and JSON bounds reject oversized payloads and external image resources. Missing runtimes retain the static fallback and report the error.

Build the renderer with `bash scripts/build-rlottie.sh`. Development binaries use `QUILL_RLOTTIE_PATH=vendor/rlottie/prefix/lib/librlottie.dylib`; macOS packaging copies the runtime and all upstream notices into the app bundle. ffmpeg/ffprobe remain the existing media playback requirements.

Validation: core suite, core clippy, UI compile, native decoder check (`QUILL_RLOTTIE_PATH=... cargo test --no-default-features --locked sticker_playback -- --include-ignored`), generated vector/transparent VP9 fixtures, and `bash scripts/macos-sticker-smoke.sh` native pixel checks.

Final release review found that standalone binaries searched only beside the executable and in app Frameworks, while native demos set an explicit renderer path. The loader now also searches vendor/rlottie/prefix/lib under executable ancestors, matching the repository release layout. The real TGS/WebM decoder regression passes with QUILL_RLOTTIE_PATH unset. A renderer sidecar is also present beside the running release.
