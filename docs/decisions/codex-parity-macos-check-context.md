# Match the protected macOS UI check context

Main requires linux-fmt-clippy-test, adversarial-review and macos-ui-build. The two existing macOS workflows were publishing macos-compile-smoke, leaving the protected macOS context absent even after successful debug UI, release UI and package builds. Their job names now match macos-ui-build. Their build steps and validation remain intact. The separate adversarial-review requirement is retained and cannot be replaced by a fabricated successful status.
