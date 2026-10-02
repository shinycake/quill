# Build reviewed releases on the personal Mac

The required macos-ui-build check now comes from the existing write-authorized workflow_dispatch workflow on the online idan-m1pro runner (self-hosted, macOS, ARM64, quill-ui-build). It performs debug/release compilation, the TGS renderer build and package smoke. CI retains Linux checks and parity validation; its public pull_request workflow never targets the personal Mac. The hosted Mac fallback remains available only when this runner is offline.

After reviewing a PR, dispatch quill-ui-build.yml with --ref set to that PR's branch and the ref input set to its exact head SHA. This attaches the build check to that head, and the workflow asserts the checked-out SHA. Do not dispatch unreviewed code. The Mac job uses the runner's checkout; it does not replace or close the user's running app. Linux and Mac required checks remain in branch protection.
