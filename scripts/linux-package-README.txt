Quill for Linux
===============

Run in place:   ./quill
Install:        ./install.sh            (to ~/.local; add a prefix argument to change)
Uninstall:      ./install.sh --uninstall

This directory is relocatable. Quill finds its native libraries in lib/ beside
the executable (libtdjson.so, libntgcalls.so, libquillvideo.so with the
FFmpeg libraries it plays video through, plus a bundled OpenSSL 3), so no environment variables are needed. The ffmpeg and ffprobe
programs beside it (same FFmpeg build) run the video editor, photo resizing
and the camera; Quill uses them before any ffmpeg on PATH. This build has no
H.264 or VP9 encoder (those are GPL or external libraries): to edit and send
videos or make video stickers, also install your distribution's ffmpeg package. Do not move quill out of
this directory without lib/.

System requirements (not bundled): a glibc-based distribution for this package's CPU (x86_64 or arm64) at least
as new as the one that built this package (see the release notes), a Vulkan
capable GPU driver, and these libraries, present on any mainstream desktop:
libgtk-3, libxkbcommon, X11 or Wayland client libraries, fontconfig, freetype,
ALSA (libasound), PulseAudio client (libpulse), zlib.

  Debian/Ubuntu: sudo apt install libgtk-3-0 libxkbcommon-x11-0 libvulkan1 \
                     libfontconfig1 libasound2 mesa-vulkan-drivers

Optional: mini apps (bot web apps) open in the quill-webview helper, which
needs WebKitGTK 4.1 and libsoup 3. Quill runs without them and tells you what
to install the first time you open a mini app.

  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-0 libsoup-3.0-0
  Fedora:        sudo dnf install webkit2gtk4.1 libsoup3

Telegram API credentials: see docs/credentials.md in the source repository.
Licenses: Quill is MIT (LICENSE). THIRD_PARTY.md lists every bundled
component and its license. The license texts are in licenses/, and the Rust
crate licenses are in THIRD_PARTY_LICENSES.md. FFmpeg (LGPL-2.1+, dynamically
linked, replaceable): licenses/ffmpeg/.

Quill is an independent, unofficial Telegram client. It is not affiliated with,
endorsed by, or sponsored by Telegram. It is early, experimental software and
comes with no warranty (see LICENSE).
