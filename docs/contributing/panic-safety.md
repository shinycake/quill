# Panic safety

Quill must not crash. On every platform a panic on the UI thread or in a GPUI
task ends the process (macOS and Windows abort when the panic reaches the
native callback; on Linux it unwinds out of the event loop). The background
and the full site inventory are in
[`docs/decisions/no-crash-audit.md`](../decisions/no-crash-audit.md).

These rules apply to every PR, human or agent. Tests are exempt: `unwrap`,
`expect`, `assert!` and indexing are fine in `#[cfg(test)]` code.

## Rules

1. **No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or
   `unimplemented!` in production code.** Use `?`, `let .. else { return }`,
   `if let`, `unwrap_or`, `unwrap_or_default`. When a value was "checked
   above", restructure so the type carries the check
   (`if let Some(live) = self.live.as_mut()`, not `is_some()` then `expect`).
2. **No indexing or slicing that can go out of range.** Use `.get(i)`,
   `.get(a..b)`, `.first()`, `.last()`, `split_first()`, iterators, or
   `chunks_exact`. Fixed-size arrays with constant indices are fine. Clippy
   enforces this workspace-wide (`indexing_slicing`); test code is exempt.
3. **Strings are sliced only on char boundaries.** Never slice with an offset
   that came from TDLib (UTF-16 entity offsets), the input engine, or
   arithmetic, without `str::get(a..b)` or `is_char_boundary`. Convert UTF-16
   offsets with the existing helpers that return `Option`.
4. **No division or modulo by a value that can be zero.** Media sizes,
   durations, counts and widths from TDLib can all be zero. Use
   `checked_div`, `.max(1)` on the divisor, or return early. Float division is
   fine, but guard the result before it reaches layout (`is_finite()`).
5. **Arithmetic on received values saturates.** Use `saturating_sub`,
   `saturating_add`, `checked_mul` for sizes, offsets, counts and timestamps
   from the network or disk. Release builds wrap silently, and a wrapped value
   becomes the next out-of-range index.
6. **`RefCell` borrows are short.** Never hold a `borrow()` or `borrow_mut()`
   guard across a call into GPUI, a callback, a native player, or another
   module. Copy what you need out and drop the guard. In paint and prepaint
   closures prefer `try_borrow_mut()` with an early return.
7. **Respect GPUI's re-entrancy contract.** Never query a `ListState` or
   scroll handle from inside that list's own row closures (the #747 crash);
   read what you need before the list lays out. Never `update` or `read` an
   entity from code that runs inside that entity's own `update`.
8. **Every `unsafe` block has a `// SAFETY:` comment** saying what the other
   side guarantees and why this call upholds it.
9. **Rust called from C never unwinds.** Every `extern "C"` / `extern
   "system"` function we implement wraps its body in the shared
   `ffi_boundary` helper (`catch_unwind`, log, return a default).
10. **Locks recover from poisoning.** Use the shared `lock_or_recover` helper,
    not `lock().unwrap()` / `lock().expect(..)`.
11. **Threads are spawned through `spawn_guarded`**, which catches a panic,
    logs it, and tells the owner the thread is gone.
12. **Objective-C ranges come from the same `NSString`.** An `NSException`
    aborts the process.
13. **Recursive walks over received data carry a depth limit.**

The helpers for rules 9 to 11 live in `crate::thread_guard`
(`ffi_boundary`, `spawn_guarded`) and `crate::locks` (`lock_or_recover`,
`read_or_recover`, `write_or_recover`, `try_lock_or_recover`, and the
`LockExt` / `RwLockExt` method forms). A panic caught by either guard is
logged as `panic-caught-<ts>.txt` and does not count as a crash.

When something truly cannot fail and the alternative would be worse code,
write `#[expect(clippy::<lint>, reason = "<the invariant>")]` on the smallest
item that needs it. The reason is reviewed like code.

## Lints (phased)

The lints live in the root `Cargo.toml` under `[workspace.lints.clippy]`, and
every first-party crate (the root package, `crates/quill-calls-sys`,
`crates/webview-protocol`, `crates/webview-host`) inherits them with
`[lints] workspace = true`. The vendored `third_party/` crates are not
workspace members and do not get them. A phase starts only when its sites are
at zero, so main never carries a warning.

`clippy.toml` sets `allow-unwrap-in-tests`, `allow-expect-in-tests`,
`allow-panic-in-tests` and `allow-indexing-slicing-in-tests`. Clippy only sees
a test module as test code when it carries a plain `#[cfg(test)]`; it does not
look inside `#[cfg(all(test, ..))]`. Write a gated test module as two
attributes:

```rust
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;
```

Helpers in `tests/*.rs` integration tests are outside any `#[test]` function,
so those files carry a file-level `#![allow(clippy::unwrap_used, ..)]` with a
reason. `unreachable!` and `undocumented_unsafe_blocks` have no test
exemption: use `panic!` with a message in tests, and give test `unsafe`
blocks a `// SAFETY:` comment too.

| Phase | Lints (`deny`) | State |
| --- | --- | --- |
| A | `unwrap_used`, `panic`, `todo`, `unimplemented`, `unreachable`, `exit` (with an `expect` on `main`'s CLI paths) | **On** (`panic-lints`) |
| B | `expect_used`, `undocumented_unsafe_blocks` | **On** (`panic-lints`) |
| C | `string_slice`, `unchecked_time_subtraction` | **On** (`panic-lints-c`) |
| D | `indexing_slicing` | **On (workspace)** (`panic-lints-d-final`). Rolled out in batches `panic-lints-d1` to `d7`, each adding `#![deny(clippy::indexing_slicing)]` per module root; those attributes stay and now duplicate the workspace lint. Details in docs/decisions/panic-lints.md |

Phase D counts are production code on macOS from
`cargo clippy --features ui --bin quill`, which lints the library with the UI
features on as well as the binary (the core-only library alone has 128
`string_slice` and 437 `indexing_slicing`), counted on the `panic-lints`
branch.

`arithmetic_side_effects` stays off: 1,781 hits, nearly all on local values
(pixel math, animation curves). Rule 5 covers the ones that matter.

Until a phase is on, reviewers check its rules by hand. New code must follow
all rules now, whatever the phase.
