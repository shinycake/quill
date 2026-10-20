# Linux arm64 packages

## What tdesktop does
Telegram Desktop ships Linux builds for x86_64 only (tarball, Snap, Flatpak); arm64 users rely on distribution or Flathub builds.

## What changed
- `package-linux.yml` builds the package twice, natively: `ubuntu-22.04` for x86_64 and `ubuntu-22.04-arm` for aarch64. Both use Ubuntu 22.04, so both packages need glibc 2.35 at most. Every check (ELF dependencies, dlopen, video probe, tarball smoke, updater asset name) runs on each CPU.
- `vendor-ntgcalls.sh` pins the official `ntgcalls.linux-arm64-shared_libs.zip` for v3.0.3 by its release digest. `build-ffmpeg.sh` only asks for nasm on x86_64 hosts; FFmpeg's ARM assembly needs no extra assembler.
- `release.yml` collects `quill-linux-aarch64-bundle.tar.gz` and the updater binary `quill-linux-aarch64`; `release-assemble.py` lists them in the notes, `SHA256SUMS.txt` and `latest.json` (`linux-aarch64` package and update binary). The file-list check expects them.
- The in-app updater already picks `quill-linux-<arch>` from `std::env::consts::ARCH`, and `linux-web-install.sh` already maps `aarch64`/`arm64` to the aarch64 asset, so neither changed.
- The product page offers both tarballs; the installer card says x86_64 or arm64.
- `installers.yml` runs the Linux installer test on `ubuntu-24.04-arm` too, skipping that leg while the latest published release has no aarch64 package.

## Other CPUs, not shipped
- **armv7 (32-bit ARM) and riscv64:** GitHub has no hosted runners for either, so TDLib, FFmpeg, rlottie and the Rust UI would have to be cross-compiled against a GTK/WebKit sysroot or built under QEMU (hours per TDLib build). ntgcalls publishes no build for them, so calls would be missing. GPUI needs a Vulkan driver; most riscv64 boards have none and 32-bit ARM boards have little Vulkan support, and GPUI is only exercised on 64-bit targets. Revisit if ntgcalls and hosted runners appear.
- **musl (Alpine):** unchanged; the installer still refuses non-glibc systems.

## Verified
Release dry run on the branch: both Linux packages, the assembled release and its file-list check passed. The arm64 package, installed with `linux-web-install.sh` (local tarball URL and SHA-256), ran the `ready-showcase` and `ready-reactions` screenshot demos in an Omarchy aarch64 VM (Hyprland, Wayland) on Mesa lavapipe; the VM has no hardware Vulkan driver, and the virtio Vulkan driver found no adapter.

## Installer fix found on the way
On Omarchy (Arch Linux ARM) the installer said `libvulkan.so.1` and `libgtk-3.so.0` were missing although both were installed. The check piped the whole `ldconfig -p` output into `grep -q`; grep exits at the first match, printf gets SIGPIPE, and `set -o pipefail` turned that into a failed lookup whenever the cache was large. It now uses a here-string, and the installer test fails if a fully provisioned runner gets that warning.
