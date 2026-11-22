# Release secrets and repository protection

2026-11-22, before adding a second maintainer.

## Where the secrets live

| Secret | Where | Used by |
|---|---|---|
| `MACOS_SIGNING_P12`, `MACOS_SIGNING_P12_PASSWORD`, `MACOS_NOTARY_KEY`, `MACOS_NOTARY_KEY_ID`, `MACOS_NOTARY_ISSUER_ID` | `release` environment | `package-macos.yml` when it builds a release (a version is set) |
| `CHOCO_API_KEY`, `FLATPAK_GPG_KEY`, `AUR_SSH_KEY` (not set yet) | `publish` environment | `chocolatey.yml` and `aur.yml` when they push, `pages.yml` build |
| `TELEGRAM_API_ID`, `TELEGRAM_API_HASH` | repository | release packaging; these are compiled into every shipped binary, so they are not treated as signing secrets |

Both environments accept only `main` and `v*` tags, so a workflow pushed on any
other branch never sees these secrets. `release` also needs the owner
(shinycake) to approve each run: every release, dry runs included, waits at
the macOS package job until he clicks **Review deployments → Approve** on the
run page. Release threads must not approve on his behalf.

CI builds of `package-macos.yml` (from `ci.yml`) get no environment and stay
ad-hoc signed, as before.

A reusable workflow cannot take an environment from its caller, so the
environment is set on the called job, and environment secrets override any
the caller passes. That is why `release.yml` no longer passes the `MACOS_*`
secrets.

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

## Collaborators

`shinycake/quill` is a personal repository: a collaborator always gets write
access (no maintain or triage roles, which only organisations have), and 2FA
cannot be required of collaborators. Write access cannot change settings,
secrets, environments or protection rules.
