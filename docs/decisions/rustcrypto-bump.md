# RustCrypto bump: chacha20poly1305 0.11, pbkdf2 0.13, sha2 0.11

tdesktop does not apply here: it uses OpenSSL/its own crypto for the local
passcode. Quill's passcode (docs/decisions/codex-local-passcode.md) and its
SHA-256 uses are Quill's own code, so this is a dependency update with no
behavior change.

## What changed

- `chacha20poly1305` 0.10 -> 0.11 (aead 0.6, hybrid-array) with the new
  `zeroize` feature. 0.10 always zeroized the key when the cipher dropped;
  0.11 only does so with that feature, so it is enabled to keep the behavior.
  `Key::from_slice` / `XNonce::from_slice` (deprecated) became
  `<&Key>::from(&[u8; 32])` and `<&XNonce>::try_from(&[u8])` (a wrong-length
  nonce is `Corrupt`, as the length is already checked before).
- `pbkdf2` 0.12 -> 0.13. `pbkdf2_hmac::<Sha512>` is unchanged for callers;
  Quill passes the stored iteration count, so the new default parameters do
  not apply.
- Quill's own `sha2` 0.10 -> 0.11, and the `sha2_next` alias is gone (one
  `sha2 = "0.11"`). digest 0.11 output has no `LowerHex`, so the two
  `format!("{:x}", ...)` sites (`hex_sha256` in `src/lib.rs`, `digest_file` in
  `src/update_install.rs`) hex-encode byte by byte, like the other sites.
- The on-disk format is untouched: `QPW1` + 24-byte nonce + ciphertext||tag,
  account id as AAD, PBKDF2-HMAC-SHA512 KEK.

## Verification

Fixed-input regression tests were added and committed first, against the old
crates (pbkdf2 0.12, chacha20poly1305 0.10, sha2 0.10), then re-run unchanged
after the bump:

- `passcode::tests::fixed_vector_kek_is_stable`: PBKDF2 output for a constant
  passcode/salt/iterations (also cross-checked with Python `hashlib`).
- `fixed_vector_sealed_blob_is_byte_identical` and
  `fixed_vector_sealed_blob_opens`: constant key/nonce/AAD give exact sealed
  bytes, and that blob opens (wrong AAD fails).
- `sha256_known_vector` (`hex_sha256`), `digest_file_is_lowercase_hex_sha256`,
  `root_digest_is_stable` (single instance), `data_store_id_has_a_fixed_value`
  (mini-app data store id). The emoji pack hash sites already hex-encode byte by
  byte and only changed crate version; the existing tests cover the mismatch path.

## Duplicates

`cargo tree -d --target all` for the RustCrypto crates, against main at 7250673e.

Before: aead (0.5.2, 0.6.1), aes (0.8.4, 0.9.3), block-buffer, cbc, chacha20
(0.9.1, 0.10.2), cipher, cpufeatures, crypto-common, digest, hkdf, hmac, inout,
sha2 (0.10.9, 0.11.1), universal-hash (0.5.1, 0.6.1). Each had an older and a
newer copy.

After: the same set minus aead, chacha20 and universal-hash (chacha20poly1305
0.11 moves to the same aead 0.6 / chacha20 0.10 / universal-hash 0.6 that
aes-gcm 0.11 already used, and poly1305 follows), plus pbkdf2 (0.12.2 and
0.13.0), which was a single version before. Every remaining old copy
(sha2 0.10.9, hmac 0.12, pbkdf2 0.12.2, aes 0.8, cbc 0.1, ...) comes from `oo7` 0.6 via
`gpui-pre-linux` (verified with `cargo tree -i`), which Quill cannot change.
Quill's own code is on a single generation, and `sha2_next` is gone.

## Unverified

macOS and Windows are not built in CI here; the changed code is
platform-independent.
