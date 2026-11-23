# Release secrets and repository protection

2026-11-22, before adding a second maintainer.

## Where the secrets live

| Secret | Where | Used by |
|---|---|---|
| `MACOS_SIGNING_P12`, `MACOS_SIGNING_P12_PASSWORD`, `MACOS_NOTARY_KEY`, `MACOS_NOTARY_KEY_ID`, `MACOS_NOTARY_ISSUER_ID` | `release` environment | `package-macos.yml` when it builds a release (a version is set) |
| `CHOCO_API_KEY`, `FLATPAK_GPG_KEY`, `AUR_SSH_KEY` (not set yet) | `publish` environment | `chocolatey.yml` and `aur.yml` when they push, `pages.yml` build |
| `UPDATE_SIGNING_KEY` | `update-signing` environment | `release.yml` assemble job (`scripts/release-sign-updates.sh`) |
| `TELEGRAM_API_ID`, `TELEGRAM_API_HASH` | `build-credentials` (main only, no reviewer) and `release` | release packaging (release.yml prepare, the Linux and Windows build jobs, the macOS job). They are compiled into every shipped binary, but a repository-level secret can be read by a workflow pushed to any branch, so no secret is kept at repository level |

All three environments accept only `main` (not tags: a release recreated on
an old tag would run that old commit's workflows), so a workflow pushed on any
other branch or tag never sees these secrets. `release` also needs the owner
(shinycake) to approve each run: every release, dry runs included, waits at
the macOS package job until he clicks **Review deployments → Approve** on the
run page. Release threads must not approve on his behalf.

CI builds of `package-macos.yml` (from `ci.yml`) get no environment and stay
ad-hoc signed, as before.

A reusable workflow cannot take an environment from its caller, so the
environment is set on the called job. That job sees the environment's secrets
only when the caller passes `secrets: inherit` (tested 2026-11-22: with
declared-but-unpassed secrets, or without `inherit`, they come through empty
and the build is silently ad-hoc signed).

## Branch and tag protection

- `main`: non-admins need a pull request; changes to the files in
  `.github/CODEOWNERS` (workflows, release and packaging scripts) need the
  owner's review. The required `linux-fmt-clippy-test` check stays. Admins are
  not enforced, so the owner's direct pushes, and release threads pushing as
  him, work as before.
- `v*` tags: only admins can create, move or delete them (ruleset
  "Release tags"). Publishing a draft release as the owner creates the tag.

Anything merged to `main` runs with the release secrets at the next release,
so a maintainer's pull request is the line of defence for code outside
`CODEOWNERS` too (build scripts, `build.rs`, crates).

## Signed updates

Anyone with write access can replace a release file, and GitHub's asset
digest then describes the replacement. So every update package carries a
`.sig` beside it: an Ed25519 signature over
`quill-update-v1\n<version>\n<file name>\n<sha256>\n`, made by the
release workflow with `UPDATE_SIGNING_KEY`, and Quill installs an update
only when it verifies against the public key in
`src/update_install/signature.rs`. The private key's backup is in the
owner's `~/.quill-signing/update-ed25519.pem`. Releases made before this
change are not signed, so the first signed release still reaches older
installs through their old, digest-only check.

## Nothing trusts a release by itself

Write access can also delete a release and recreate it on an old tag, mark
any release "latest", and replace files on releases published before
immutable releases were turned on. So since signing began every release
file has `<file>.sig`, and every consumer checks it:

- the updater (`src/update_install/signature.rs`);
- the site: `scripts/site-release-data.py` shows the newest published
  release whose files all verify (`scripts/release_verify.py pick`), not
  GitHub's "latest", and writes `data/SHA256SUMS.txt` and
  `data/release-tag.txt` for the install scripts; its Flatpak repo imports
  only bundles that match their signatures;
- Chocolatey and AUR publish only the newest verified release. They run
  from `main` on a `release-published` dispatch (sent by
  `release-published.yml`), never on an old tag's `release` event.

`release-guard.yml` audits every signed release hourly and on each publish,
and opens an issue for the owner if a file no longer matches its signature
or "latest" is not the newest verified release.

## Immutable releases and checked drafts

Published releases are immutable (repository setting, 2026-11-22): no file
on them can be added, replaced or removed, and their tag can't move. So
everything is attached while the release is still a draft: `release.yml`
uploads its files and its `flatpak` job attaches the Flatpak bundles, built
from the draft's own tarballs.

A draft can still be changed by anyone with write access. Publish with
`scripts/release-publish.sh <tag>`, never the Publish button or
`gh release edit --draft=false`. It checks that the draft holds exactly the
files the release.yml run on `main` built, with the same SHA-256 and all
uploaded by github-actions, and only then publishes. `--check-only` checks
without publishing. Editing the draft's notes is fine; they are not checked.

## No self-hosted runners

The owner's Mac (`idan-m1pro`) was a self-hosted runner for
`quill-ui-build.yml`. Anyone with write access could dispatch that workflow on
their own commit, or push a branch with a workflow aimed at the runner, and run
code on the Mac as the owner. The runner and the workflow were removed on
2026-11-22; `macos-smoke-fallback.yml` (GitHub-hosted) and `ci.yml`'s
`macos-package` cover the macOS build. Do not register a self-hosted runner on
this repository again.

## Collaborators

`shinycake/quill` is a personal repository: a collaborator always gets write
access (no maintain or triage roles, which only organisations have), and 2FA
cannot be required of collaborators. Write access cannot change settings,
secrets, environments or protection rules.

For now `* @shinycake` in CODEOWNERS makes every merge to main wait for the
owner's review, so a collaborator cannot approve and merge someone else's
change to the app either. Releases made before signing existed are deleted
together with their tags once the first signed release is published: those
releases cannot be made immutable, and a collaborator could otherwise mark one
"latest" and swap its files for clients that only check digests.
