# Mac builds: keep the runner cache, release build on request

## Problem
Since b5f7932 (Oct 2) the required macos-ui-build job took 5–7 minutes
instead of 1–2.
- `actions/checkout` ran its default `git clean -ffdx`, which deleted
  `target/` and `vendor/`, so every run compiled every crate from scratch.
- The job also ran a release build (thin LTO, `codegen-units = 1`): 4m18s on
  a cold cache.
- The shallow `--depth=1` fetch re-downloaded the whole tree each time
  (61s).

## Decision
- Checkout runs with `clean: false` and full history. A follow-up
  `git clean -ffdx -e target/ -e vendor/` still removes everything else.
- The release compile, TGS renderer build and package smoke run only when
  the build is dispatched with `-f package=true`. The owner asks for release
  builds explicitly. The required check is the debug UI compile.
