Quill for Linux
===============

Run in place:   ./quill
Install:        ./install.sh            (to ~/.local; add a prefix argument to change)
Uninstall:      ./install.sh --uninstall

This directory is relocatable. Quill finds its native libraries in lib/ beside
the executable (libtdjson.so, libntgcalls.so, librlottie.so, libquillvideo.so
with the FFmpeg libraries it plays video through, plus a bundled
OpenSSL 3), so no environment variables are needed. Do not move quill out of
this directory without lib/.

System requirements (not bundled): a glibc-based x86_64 distribution at least
as new as the one that built this package (see the release notes), a Vulkan
capable GPU driver, and these libraries, present on any mainstream desktop:
libgtk-3, libxkbcommon, X11 or Wayland client libraries, fontconfig, freetype,
ALSA (libasound), zlib.

  Debian/Ubuntu: sudo apt install libgtk-3-0 libxkbcommon-x11-0 libvulkan1 \
                     libfontconfig1 libasound2 mesa-vulkan-drivers

Telegram API credentials: see docs/credentials.md in the source repository.
Licenses: LICENSE and THIRD_PARTY.md (libntgcalls and OpenSSL notices);
FFmpeg (LGPL-2.1+, dynamically linked, replaceable): licenses/ffmpeg/.
