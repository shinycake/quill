# Verified checklist reconciliation

The shadow publisher does not mutate the checklist. Feature declarations therefore remained unconsumed after successful merges. The existing merge helper now runs on macOS as well as Linux. A reconciliation PR may update README only when its entire contents exactly match running that helper against deleted fragments already present in the merge base. It cannot add or modify declarations, include code changes, or edit DECISIONS. Ordinary feature PRs still declare completion through fragments. The runnable smoke checks generated output and rejects manual, mixed and unmerged claims.

At the user's explicit request, GitHub's adversarial-review status requirement was removed. The Linux test and macOS build requirements remain. Each feature PR has a recorded source review and native verification evidence; the renderer lookup finding from that review was fixed before merging.
