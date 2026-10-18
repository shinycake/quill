# One-line installers for Linux and Windows

The macOS installer (`macos-install-without-notarization.md`) gets Quill past
Gatekeeper with one pasted command. Linux and Windows get the same: one line
that downloads the latest release, checks it, installs it for the current user
and opens it.

## Linux

`curl -fsSL https://raw.githubusercontent.com/shinycake/quill/main/scripts/linux-web-install.sh | bash`

`scripts/linux-web-install.sh` picks `quill-linux-<arch>-bundle.tar.gz` from
`uname -m` (only x86_64 is published today; on aarch64 it says there's no
build yet, before downloading anything), checks it against `SHA256SUMS.txt`,
quits the installed copy if it is running (`quill --quit`, matched by
`/proc/<pid>/exe`, so a Quill run from elsewhere is left alone), and runs the
package's own `install.sh`. Reusing that script keeps one install layout:
`~/.local/opt/quill`, `~/.local/bin/quill`, the desktop entry and icons, and
`install.sh --uninstall` to remove them. It refuses root without
`QUILL_PREFIX`, because a root install would put Quill in root's home, and
refuses musl systems, which can't load the glibc build. After installing it
runs `ldd` on the binary and names any system library that is missing, so a
bare system says what to install instead of failing silently from the menu.

## Windows

`irm https://raw.githubusercontent.com/shinycake/quill/main/scripts/windows-install.ps1 | iex`

`scripts/windows-install.ps1` runs on Windows PowerShell 5.1, which is what a
fresh Windows 10 or 11 opens, as well as PowerShell 7. It forces TLS 1.2 and
turns off the progress bar, which slows 5.1 downloads badly. The body is one
script block, so `iex` runs nothing until all of it has arrived, and a failure
prints a message instead of calling `exit`, which would close the user's
window.

It installs to `%LOCALAPPDATA%\Programs\Quill`, the per-user location Windows
uses for apps installed without admin rights. The in-app updater already works
there (`src/update_install/layout.rs` stages updates inside the install
folder). A running copy is asked to quit first, since Windows locks a running
program's files. The new folder is staged beside the destination and swapped
in with renames. It adds a Start menu shortcut and a Settings > Apps entry.
The uninstall command is stored in the registry as an encoded PowerShell
command rather than as a file in the install folder, because in-app updates
replace that folder. Uninstalling removes the folder, the shortcut, the toast
identity key, and the autostart and `tg://` keys when they point at this
install. Account data in `%APPDATA%` stays.

SmartScreen's "Windows protected your PC" check runs on files that carry the
Mark of the Web (the `Zone.Identifier` stream browsers add). PowerShell's
downloads don't carry it, and the script runs `Unblock-File` in case something
added it, so Quill starts without the prompt. Smart App Control (Windows 11,
when turned on) is a separate check that doesn't depend on that mark and may
still block an unsigned app; only a code-signing certificate avoids it.

## Trust

Same as macOS: the checksum comes from the same release as the archive, so it
catches a corrupt or truncated download, not a compromised release. That is
the same trust as the download button. The scripts are short and linked from
the site so people can read them first.

## Testing

`.github/workflows/installers.yml` runs both scripts against the latest
published release on every change to them and on every release. It covers a
fresh install, an update while the installed Quill is running, a bad checksum
(which must install nothing), and removal. Windows runs under both
PowerShell 5.1 and 7. Linux also runs as a normal user in bare Debian, Fedora
and Arch containers, first without the desktop libraries (the missing-library
message must appear) and then with them, and checks that Alpine and root
without a prefix are refused. All of this is on hosted runners and containers;
neither script has been run on a physical Linux or Windows machine.
