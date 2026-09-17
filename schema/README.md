Vendored from official TDLib commit `d1085f9cebc5a62379991ae1652673954f229c1f` (`td/generate/scheme/td_api.tl`).

- CMake version: 1.8.67
- Size: 1,152,505 bytes
- SHA-256: `326b65b41442901ad6bf0ca2f7c356ae54365d6c343956a62e06a8b3cb305e87`
- Upstream: https://raw.githubusercontent.com/tdlib/td/d1085f9cebc5a62379991ae1652673954f229c1f/td/generate/scheme/td_api.tl
- License: Boost Software License 1.0 (see TDLib `LICENSE_1_0.txt`)

Refresh with `bash scripts/vendor-td-schema.sh`. Do not mix this file with a different tdjson binary. A 1,000,001-byte copy is truncated and invalid (`vector<T>` parameters get stripped).
