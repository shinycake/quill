# Flaky core test: clipboard paste temp names

Failing test: `composer::attachments::tests::clipboard_image_attachment_names_are_unique`
(seen as `GATE FAIL tests core=(3154 1)`; reproduced about 1 in 70 runs with six
parallel `cargo test` loops).

tdesktop: pasted images are written to a fresh uniquely named temp file per paste.

Root cause: `clipboard_image_attachment` named the temp file from the nanosecond
clock plus pid. Two pastes landing in the same clock tick (two tests on parallel
threads) got the same path, so one test deleted the file the other was about to
canonicalize and `pick` returned `None`. It could also overwrite a prior paste in
production.

Fix: add a process-wide atomic sequence number to the file name.

Verification: core test suite repeated under load after the change (see PR).
