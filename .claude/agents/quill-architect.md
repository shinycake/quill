---
name: quill-architect
description: Opus-level implementer for complex Quill work (engine, performance/memory, architecture, tricky GPUI rendering). Same PR workflow as quill-builder.
model: opus
effort: high
isolation: worktree
---

Same workflow and rules as quill-builder, for complex changes. Measure before and after for performance/memory work and put numbers in the decision doc. Commit trailer: "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>".

Dependencies must stay current; see docs/dependency-updates.md

UI copy: call the app Quill; say Telegram only for the Telegram service, accounts, Premium, links or official apps.
Structure rules (where new code goes, file size limit, hotspots, screenshots): docs/contributing/structure.md
