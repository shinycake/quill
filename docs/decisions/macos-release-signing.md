# Signing macOS releases with a stable identity

Idan reported on 2026-10-24 that after every update macOS asks for the login keychain password: "Quill wants to use your confidential information stored in 'org.shinycake.quill' in your keychain".

## Why it prompts

The TDLib database key is a generic password in the login keychain (`org.shinycake.quill` / `db-key:<account>`, `src/platform.rs`). A legacy keychain item carries a partition list, and since macOS 10.12 an app outside that list gets the password prompt even when its ACL entry allows it. The partition ID comes from the code signature:

- Apple-issued certificate: `teamid:<TEAM>`, the same for every build.
- Ad-hoc or self-signed: `cdhash:<hash>`, new for every build.

GitHub releases were ad-hoc signed (`package-macos.yml`), so each update had a new cdhash and a new prompt. On the owner's Mac the item had collected 31 ACL entries and 35 `cdhash:` partitions, one per installed or local build, next to a single `teamid:72D8U72V55` from local builds signed with the Apple Development identity. A self-signed certificate does not help (tried 2026-10-05): it still gets a `cdhash:` partition.

## Decision

Release builds are signed with the owner's Apple Development identity (team `72D8U72V55`) when the `MACOS_SIGNING_P12` and `MACOS_SIGNING_P12_PASSWORD` repository secrets are set. `package-macos.yml` imports the PKCS#12 into a throwaway keychain, adds Apple's WWDR G3 intermediate so codesign embeds the full chain, and passes the certificate's SHA-1 to `scripts/macos-package-smoke.sh` as `QUILL_CODESIGN_IDENTITY`. The script timestamps certificate signatures so they stay valid after the certificate expires. The build checks that the bundle carries a `TeamIdentifier`. Without the secret, release builds stay ad-hoc and the workflow warns; CI builds stay ad-hoc silently.

- The partition becomes `teamid:72D8U72V55` for every release, so users approve once and never again. The owner's item already lists that team, so the owner gets no further prompt at all.
- The updater accepts the switch: an ad-hoc running copy installs any strictly valid bundle matching the release digest (`codesign::judge`). From then on the running copy is certificate-signed, and a staged bundle must satisfy its designated requirement (`identifier "org.shinycake.quill" and anchor apple generic and certificate leaf[subject.CN] = "Apple Development: …" and certificate 1[field.1.2.840.113635.100.6.2.1]`). A renewed certificate keeps the same common name and team, so renewals keep updating. An ad-hoc release published after signed ones would be refused as "signed differently", so the secret must stay set.
- Gatekeeper treats an un-notarized Apple Development signature like an ad-hoc one; the install paths in `macos-install-without-notarization.md` are unchanged.
- The certificate's common name, which includes the owner's Apple ID email, is visible in the signature (`codesign -dv`).

Rejected: storing the key outside the keychain (what tdesktop does with `tdata`) gives up the keychain's protection against other processes reading the key. The data protection keychain needs a `keychain-access-groups` entitlement, which needs a provisioning profile from the same team, so it needs the certificate anyway. Rewriting the partition list from the old copy during an update needs the keychain password, which is the prompt being removed.

## Verified

On the owner's Mac (2026-10-23), with the installed ad-hoc Quill quit: two copies of the release bundle with different contents (different cdhashes), each signed `--timestamp` with the Apple Development identity, opened one after the other from a Finder-like environment. Neither raised a SecurityAgent prompt. The second satisfied the first's designated requirement under `codesign --verify --deep --strict -R`. A third copy re-signed ad-hoc raised the prompt, as releases did.

## Setting the secret

Once, on the Mac that holds the identity:

```bash
security export -k ~/Library/Keychains/login.keychain-db -t identities -f pkcs12 -P "$P12_PASSWORD" -o /tmp/quill-signing.p12
base64 -i /tmp/quill-signing.p12 | gh secret set MACOS_SIGNING_P12 --repo shinycake/quill
printf %s "$P12_PASSWORD" | gh secret set MACOS_SIGNING_P12_PASSWORD --repo shinycake/quill
rm /tmp/quill-signing.p12
```
