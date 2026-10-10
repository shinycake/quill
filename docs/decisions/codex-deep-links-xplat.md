# Cross-platform tg:// links

Branch: `codex/deep-links-xplat`

## What was already there

Single-instance forwarding (`src/single_instance.rs`, `src/deep_link_inbox.rs`) and the opt-in "Open Telegram links with Quill" setting (`src/link_handler.rs`, see `codex-link-handler-opt-in.md`) were in place. A second launch with a `tg://` argument is validated by `sanitize_link` and handed to the running instance, which raises its window. That path is covered by in-process socket tests and was not changed.

## What tdesktop does

- `platform/win/specific_win.cpp` `psRegisterCustomScheme` writes `HKCU\Software\Classes\tg` on every start.
- `platform/linux/specific_linux.cpp` registers the `.desktop` file and its `x-scheme-handler/tg` association through `xdg-mime`.
- `Telegram.plist` declares `CFBundleURLTypes`.
- `core/local_url_handlers.cpp` `ResolveLoginCode` passes `tg://login?code=` to the code step, which fills the field and submits.

Quill keeps its own rule: it registers only when the user turns the setting on, so people who also run Telegram Desktop keep their links.

## What changed

- Linux: `xdg-mime default quill.desktop` fails when no `quill.desktop` exists, which is the case for an unpacked tarball or an AppImage. Turning the setting on now first checks `$XDG_DATA_HOME` and `$XDG_DATA_DIRS` for an existing entry. If there is none it writes `~/.local/share/applications/quill.desktop` atomically (the helper the autostart entry uses), with `Exec=<exe> %u` and `MimeType=x-scheme-handler/tg;`. A packaged or `linux-install.sh` entry is left alone. Under an AppImage the entry points at `$APPIMAGE`, because the mounted binary disappears on exit.
- Login code links: `login_code_from_link` accepts `tg://login?code=<digits>` and `https://t.me/login/<digits>` (digits only, at most 8). While the code step is showing, the code fills the field and is submitted when its length matches what Telegram announced. On other steps the link waits; once signed in it gets the existing "already signed in" answer.
- Boost links: `getChatBoostLinkInfo` finds the channel, then Quill opens it and its info panel, which has the boost level and the Boost button. Nothing is boosted without a press.
- Premium page links open the Premium screen.
- Voice chat links with a username open that group or channel. They do not join the call, because joining from a link would open the microphone unasked.

## Per OS status

| OS | Scheme registration | Forwarding to a running instance |
| --- | --- | --- |
| macOS | `CFBundleURLTypes` in the bundle plist; "Make Quill the default" uses `NSWorkspace` and the system asks to confirm | `application:openURLs:` and the local socket |
| Linux | `MimeType` in the desktop file; the switch writes a user entry if needed, then `xdg-mime default` | argv through the Unix socket |
| Windows | the switch writes `HKCU\Software\Classes\tg`; removing it deletes only a Quill registration | argv through the named pipe |

## Not done

- startgroup and startchannel: still an explanatory message (needs a group picker and `addChatMember`).
- Joining a voice chat, video chat or live stream from a link (see above).
- Theme links: Quill has no desktop theme files. Background links already work.
- Language pack links: Quill has no translations. Privacy policy links arrive as unknown deep links.
- Boost links open a panel rather than tdesktop's boost box.

## Tests

Pure unit tests cover the XDG directory order, the desktop entry text and quoting, the AppImage target, writing and skipping the entry in temp directories, and the login link parser. A driver test covers the boost link flow, and the route tests cover Premium and boost. Nothing in the tests touches the real registry or the real `xdg-mime`.

## Verified and unverified

Verified on macOS: unit tests, the full gate. Not run: the Linux and Windows registration against a real desktop, since this host is macOS. The Windows registry code was not changed.
