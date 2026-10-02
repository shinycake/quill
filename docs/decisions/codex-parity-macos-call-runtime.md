# Native call media runtime on macOS ARM64

The v3.0.0 upstream release has an official macOS ARM64 shared library. Procurement now chooses the host asset and verifies its pinned SHA-256, matching GitHub's asset digest: 20cac9a1516c75e08d81049d2d8126e7d156ad800aaa1314f14f66b13f83508a. The Linux x86_64 digest and behavior remain. Sidecars are unmodified, dynamically loaded and ignored by Git; third-party notices retain the LGPL source offer.

The loader uses Rust's native library name and searches development vendor directories and bundle Frameworks directories. The package smoke builds the current UI release, optionally includes the unmodified native sidecar, and declares camera/microphone usage descriptions. It does not grant recording permission or start a call.

Validation on this Mac: all 76 native symbols are exported; loader tests resolve the complete bound API and version 3.0.0; a copied test executable loads the actual dylib from an isolated app bundle without an environment override or checkout vendor directory. Quill's adapter loaded the real protocol and enumerated 2 microphones, 1 speaker, 2 cameras and 1 screen source. No call, microphone recording, camera recording or screen recording was started.

This removes the missing-native-runtime blocker on this Mac. It does not complete the live group-video or screen-sharing checkboxes: those still require a designated private Telegram test group and a second test participant. The user was asked for that target; no external call was placed. The header's media-description struct has source/device fields but no echo-cancellation or noise-suppression switches, so the audio-effects checkbox remains unchanged rather than exposing controls that cannot change the engine.

Source: [official NTgCalls v3.0.0 release](https://github.com/pytgcalls/ntgcalls/releases/tag/v3.0.0), its published macos-arm64 shared-library asset digest, and the bundled ntgcalls.h header.
