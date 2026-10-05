# codex/remove-conductor — drop the shadow-mode Quill Conductor workflows

- Removed `quill-conductor.yml` (the `shadow-reconcile` job, failing on every PR because the
  `quill-control` issue no longer exists), plus `quill-worker.yml`, `quill-publisher.yml`,
  `quill-notify.yml` and `.github/scripts/quill-shadow-projection.sh`. They were the
  never-activated shadow-mode scaffolding of the conductor migration. None of them mutated
  anything, and none was a required check.
- `dashboard-data.yml` now triggers on `workflow_run` of `ci` only.
- Required checks are unchanged: `linux-fmt-clippy-test` and `macos-ui-build`.
