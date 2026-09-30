#!/usr/bin/env bash
# quill-shadow-projection.sh — read-only reconciliation for Quill Conductor (shadow mode).
# Per workflow-redesign.md §7 step 3: while the control issue says `mode: shadow`,
# this script takes NO mutations. It rebuilds the GitHub projection (open-PR queue
# order, heads, required check results, branches, README parity at the current
# main SHA) and emits it as JSON + a step summary, so shadow cycles can be
# compared against the legacy pipeline's view.
#
# ponytail: one script, stdlib tools only (bash, gh, jq, python3). No new deps.
set -euo pipefail

REPO="${REPO:-shinycake/quill}"
OUT="${OUT:-projection.json}"

# --- control mode -----------------------------------------------------------
CONTROL_ISSUE="$(gh api "repos/$REPO/issues?labels=quill-control&state=open&per_page=5" \
  --jq '[.[] | select(.title | test("Quill Conductor control"))][0].number')"
if [ -z "$CONTROL_ISSUE" ] || [ "$CONTROL_ISSUE" = "null" ]; then
  echo "::error::no open issue labeled quill-control titled 'Quill Conductor control' found"
  exit 1
fi
MODE="$(gh api "repos/$REPO/issues/$CONTROL_ISSUE" --jq '.body' | grep -m1 '^mode:' | awk '{print $2}')"
echo "control issue #$CONTROL_ISSUE, mode=$MODE"
if [ "$MODE" != "shadow" ]; then
  echo "::error::mode is '$MODE' (expected 'shadow'); active-mode mutation logic is not implemented yet (migration step 9+)"
  exit 2
fi

# --- main SHA + README parity ------------------------------------------------
MAIN_SHA="$(gh api "repos/$REPO/branches/main" --jq '.commit.sha')"
echo "main SHA: $MAIN_SHA"
gh api "repos/$REPO/contents/README.md?ref=$MAIN_SHA" --jq '.content' | base64 -d > /tmp/quill-conductor-readme.md
PARITY="$(bash scripts/parity_pct.sh /tmp/quill-conductor-readme.md)"
echo "$PARITY"

# --- open PR queue, ordered by (createdAt, number) ----------------------------
gh api "repos/$REPO/pulls?state=open&per_page=100" \
  --jq '[.[] | {number, title, created_at, head_sha: .head.sha, head_ref: .head.ref,
                base: .base.ref, draft: .draft, auto_merge: (.auto_merge != null)}]
        | sort_by(.created_at, .number)' > /tmp/quill-conductor-prs.json

# --- required check results per PR head --------------------------------------
python3 - <<'EOF' > /tmp/quill-conductor-checks.json
import json, subprocess
prs = json.load(open('/tmp/quill-conductor-prs.json'))
out = []
for pr in prs:
    sha = pr['head_sha']
    runs = json.loads(subprocess.run(
        ['gh', 'api', f'repos/shinycake/quill/commits/{sha}/check-runs'],
        capture_output=True, text=True, check=True).stdout)
    checks = {}
    for cr in runs.get('check_runs', []):
        # keep the latest completed run per check name
        cur = checks.get(cr['name'])
        if cur is None or (cr['status'] == 'completed' and cur.get('status') != 'completed'):
            checks[cr['name']] = {'status': cr['status'], 'conclusion': cr.get('conclusion')}
    out.append({'number': pr['number'], 'head_sha': sha, 'checks': checks})
print(json.dumps(out))
EOF

# --- active branches ----------------------------------------------------------
# (--paginate streams one JSON doc per page, so collect names line-wise, not as JSON)
gh api "repos/$REPO/branches?per_page=100" --paginate --jq '.[].name' \
  | sort -u > /tmp/quill-conductor-branch-names.txt
python3 -c "
import json
names = [l.strip() for l in open('/tmp/quill-conductor-branch-names.txt') if l.strip()]
json.dump(names, open('/tmp/quill-conductor-branches.json', 'w'))
"

# --- assemble projection -------------------------------------------------------
python3 - "$OUT" <<'EOF'
import json, sys
out_path = sys.argv[1]
prs = json.load(open('/tmp/quill-conductor-prs.json'))
checks = {c['number']: c for c in json.load(open('/tmp/quill-conductor-checks.json'))}
branches = json.load(open('/tmp/quill-conductor-branches.json'))
proj = {
    'mode': 'shadow',
    'repo': 'shinycake/quill',
    'queue': [
        {**p,
         'required_checks': checks.get(p['number'], {}).get('checks', {}),
         'fifo_position': i + 1}
        for i, p in enumerate(prs)
    ],
    'branch_count': len(branches),
    'branches': branches,
}
json.dump(proj, open(out_path, 'w'), indent=2)
print(f"wrote {out_path}: {len(prs)} open PRs, {len(branches)} branches")
EOF

# --- step summary --------------------------------------------------------------
{
  echo "## Quill Conductor shadow reconcile"
  echo ""
  echo "- mode: shadow (read-only, no mutations)"
  echo "- main: \`$MAIN_SHA\`"
  echo "- $PARITY"
  echo ""
  echo "### Merge queue (oldest first)"
  echo ""
  echo "| pos | PR | title | head | draft | auto-merge |"
  echo "| --- | -- | ----- | ---- | ----- | ---------- |"
  jq -r 'to_entries[] | "| \(.key + 1) | #\(.value.number) | \(.value.title[:60]) | \(.value.head_sha[:7]) | \(.value.draft) | \(.value.auto_merge) |"' \
    /tmp/quill-conductor-prs.json
} >> "$GITHUB_STEP_SUMMARY"

echo "shadow reconcile complete — no mutations performed"
