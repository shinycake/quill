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

`cargo tree -d --target all` for the RustCrypto crates: before, sha2, digest,
hmac, hkdf, block-buffer, cipher, crypto-common, inout, cpufeatures each had an
older and a newer copy. After, the same set remains, plus pbkdf2 (0.12 and
0.13), which was a single version before. The old copies are only used by `oo7` 0.6 (via GPUI on
Linux: sha2 0.10, hmac 0.12, pbkdf2 0.12), which we cannot change. Quill's own
code is on a single generation. `aead`, `chacha20`, `poly1305` and
`universal-hash` are not duplicated before or after.

## Unverified

macOS and Windows are not built in CI here; the changed code is
platform-independent.
