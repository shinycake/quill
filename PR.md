Chat windows show status notes, and a link or folder that fails to open now says so
## What was wrong

- N91: the status toast was drawn only by the main window (`status_toast_layer` had one caller, `app_render/mod.rs`). A note raised from a chat window, such as a failed send or a copied link, showed in the main window, which may be behind the chat window or minimized.
- N92: 25 places opened links with GPUI's `cx.open_url`, which returns nothing. Help, FAQ, terms, release pages, Fragment, explorer links, support mail, subscription renewal and others did nothing visible when no browser or mail client took the link.
- N93: `reveal_in_file_manager` returned a bool that "Show log" on the crash notice and a click on a "saved" toast threw away. "Show folder" after a chat or account export used GPUI's `reveal_path`, which reports nothing at all.
- L42 (owner decision, default applied): successful opens left notes ("opened link", "opened mail", "opened file", "revealed in file manager", a story map's place name). Telegram Desktop shows nothing when an open works (`file_utilities_win.cpp:298-301`).

## What changed

- N91: the toast shows in the window where the last input happened, which is the same rule that places a kit dialog (`windows.last_input`). In a chat window it sits above the compose field, or above the media viewer's bar when the viewer is open there. The main window draws it only when the last input was its own.
- N92: `QuillApp::open_link` (`src/ui/os_open.rs`) is the one way to open a link. It sends web links to `platform::open_external_url` and Quill's own `mailto:` links to the new `platform::open_mailto_url`, and it shows "could not open link" (or the mail failure) when the OS refuses. `os_open::open_link_in` does the same for handlers that hold only the app's handle. All 25 callers moved to it. A `clippy.toml` `disallowed-methods` rule bans `App::open_url`.
- N93: `reveal_in_file_manager` returns an `OsOpen` and goes through the same gate as other opens (the agent-injection refusal and test scripting). `QuillApp::reveal_in_folder` reports "couldn't open the folder". The crash notice pushes that text as a kit notification, because it is built before the app exists. A clippy rule bans `App::reveal_path`.
- L42: every open that used to post a success note now goes through `open_link` or `report_failed_open`. `open_message_url` was deleted and its callers use `open_link`. The 7 strings that only those notes used were removed.
- Test builds record the last target handed to the OS (`platform::os_opened_last`). The Fragment withdrawal and wallet notice tests read that record now, because GPUI's `opened_url` no longer sees these links. This follow-up has its own commit.

## What ran

- `cargo fmt`, plus `cargo clippy -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`. Both are clean after the merge of main.
- The full `cargo test --features demo-capture --bin quill` run on the branch: 2444 passed, 2 ignored, 3 failed. The failures were the two `opened_url` tests (fixed in the follow-up commit, and they pass now) and `a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`, which is the chmod 0555 test that fails as root.
- New tests, each seen failing without its fix:
  - `a_note_from_a_chat_window_toasts_in_that_window`
  - `help_that_could_not_open_says_so`
  - `a_saved_toast_whose_folder_could_not_open_says_so`
- Other new tests:
  - `a_link_the_shell_took_says_nothing`
  - `a_link_that_opens_says_nothing`
  - `a_link_the_browser_could_not_take_says_so`
  - `a_mail_link_the_mail_client_could_not_take_says_so`
  - `a_link_of_another_kind_is_refused_and_says_so`
  - `a_mail_link_reaches_the_os_only_when_it_is_one`
- `scripts/check-file-size.sh`, `check-hotspots.sh`, `check-window-scope.py` and `check-lang.py` all pass.

## What did not run

- `cargo check --target x86_64-pc-windows-gnu` does not build here: the `gpui-pre` build script fails before Quill's code compiles. The Windows `reveal_now` branch (`explorer /select,`) only moved into a new function, and its body is unchanged.
- The macOS `open -R` branch has the same body as before and was not compiled for macOS.
- I took no screenshot of the toast in a chat window. The screenshot demos open a single window.
- No numbers were given, so there was nothing to measure.

## What stays open

- Nothing from B19. The "opened link" and "opened chat" notes in `deep_links.rs` and `chat_search_bar.rs` report an in-app chat opening, not an open handed to the OS, so L42 does not cover them and they are unchanged.
