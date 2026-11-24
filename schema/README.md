Vendored from official TDLib commit `1b9efe84d6e9a2e2f6eadeeea32b0726d2b8c25d` (`td/generate/scheme/td_api.tl`).

- CMake version: 1.8.71
- Size: 1,277,521 bytes
- SHA-256: `bf68db118f42010b1359dc6776128567d8de18fc793aa5280f10d2912384c1b1`
- Upstream: https://raw.githubusercontent.com/tdlib/td/1b9efe84d6e9a2e2f6eadeeea32b0726d2b8c25d/td/generate/scheme/td_api.tl
- License: Boost Software License 1.0 (see TDLib `LICENSE_1_0.txt`)

Refresh with `bash scripts/vendor-td-schema.sh`. Do not mix this file with a different tdjson binary. A 1,000,001-byte copy is truncated and invalid (`vector<T>` parameters get stripped).
