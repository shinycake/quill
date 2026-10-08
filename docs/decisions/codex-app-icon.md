# App icon

Quill ships one icon design on all three platforms. The sources live in `assets/icons/`:

| File | Used by |
|---|---|
| `Quill.icns` | macOS bundle: `scripts/macos-package-smoke.sh` copies it to `Contents/Resources/Quill.icns` and sets `CFBundleIconFile` to `Quill`. |
| `Quill.ico` (16–256 px) | Windows: to be embedded in `quill.exe` as a resource by the Windows packaging work. |
| `hicolor/<size>/apps/quill.png` (16–1024 px) | Linux: `scripts/linux-package.sh` installs every size under `share/icons/hicolor/`, and `install.sh` copies them into `$PREFIX/share/icons/hicolor` (the `.desktop` file uses `Icon=quill`). |
| `quill-1024-fullbleed.png`, `quill-1024-macos.png` | Masters: full-bleed, and with macOS grid padding and shadow. |

The placeholder `assets/quill.svg` from the Linux packaging PR is gone. Linux uses the PNG theme sizes rather than a scalable SVG, because the design is a raster.
