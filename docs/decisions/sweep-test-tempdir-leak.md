# Test temp directories are removed at exit

Test helpers built `temp_dir()/quill-<name>-<pid>` paths and never deleted them.
Each test run left one directory per helper (`quill-engine-test`,
`quill-audio-test`, `quill-wallpaper`, `quill-image-open`, `quill-composer-sends`,
`quill-emoji-style-ui`, `quill-notify-windows`, `quill-advanced-ui`) and one per
`prepared_tmp` call in the connect tests (`quill-connect-*`).

`src/scratch.rs` now hands out these directories (`process_dir`, `unique_dir`)
and removes them from an `atexit` hook, which also runs when tests fail or
panic. Some of them back the process-wide isolated app root, so a per-test
guard type would not fit. No runtime (non-test) code creates directories with
these prefixes. A SIGKILL still skips the hook.
