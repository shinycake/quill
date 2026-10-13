//! TON addresses: validation, the explorer link and the grouped form the
//! transfer card shows.

/// Telegram Desktop's fallback for the `ton_blockchain_explorer_url` app
/// config (`TonAddressUrl`, `boxes/gift_premium_box.cpp`).
pub const DEFAULT_EXPLORER_URL: &str = "https://tonviewer.com/";

/// Length of a user-friendly address: 36 bytes in base64url.
const ADDRESS_LEN: usize = 48;

/// Whether `address` is a user-friendly TON address the official iOS app
/// acts on (`WalletContext.transferAddress`): 48 base64url characters
/// that decode to a flag byte, a workchain byte, a 32-byte hash and a
/// valid CRC16 (XMODEM) of those 34 bytes.
pub fn is_valid_ton_address(address: &str) -> bool {
    if address.len() != ADDRESS_LEN {
        return false;
    }
    let Some(bytes) = decode_base64url(address.as_bytes()) else {
        return false;
    };
    let (Some(body), Some(crc)) = (bytes.get(..34), bytes.get(34..36)) else {
        return false;
    };
    let expected = crc16_xmodem(body);
    crc == expected.to_be_bytes()
}

/// Where "View in Explorer" opens `address`: the app-config explorer
/// prefix (or [`DEFAULT_EXPLORER_URL`]) with a trailing slash, then the
/// address with anything outside the URL path alphabet percent-encoded
/// (the iOS app's `urlPathAllowed` minus `/`).
pub fn explorer_url(base: Option<&str>, address: &str) -> String {
    let base = base
        .map(str::trim)
        .filter(|base| base.starts_with("https://") || base.starts_with("http://"))
        .unwrap_or(DEFAULT_EXPLORER_URL);
    let mut url = base.to_string();
    if !url.ends_with('/') {
        url.push('/');
    }
    for byte in address.trim().bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~:=+@!$&'()*,;".contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

/// `ton_blockchain_explorer_url` from a `jsonValueObject` app config
/// (`getApplicationConfig`); `None` when missing or not a web address.
pub fn explorer_url_from_config(config: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    config
        .get("members")
        .and_then(Value::as_array)?
        .iter()
        .find(|member| {
            member.get("key").and_then(Value::as_str) == Some("ton_blockchain_explorer_url")
        })
        .and_then(|member| member.get("value")?.get("value")?.as_str())
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .map(str::to_string)
}

/// The address in groups of four, as the transfer card prints it.
pub fn group_address(address: &str) -> String {
    let chars: Vec<char> = address.chars().collect();
    chars
        .chunks(4)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_base64url(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &byte in input {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
pub(crate) fn encode_test_address(flag: u8, workchain: u8, hash: [u8; 32]) -> String {
    let mut bytes = vec![flag, workchain];
    bytes.extend_from_slice(&hash);
    let crc = crc16_xmodem(&bytes);
    bytes.extend_from_slice(&crc.to_be_bytes());
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte));
        for shift in [18, 12, 6, 0] {
            let index = ((n >> shift) & 0x3F) as usize;
            out.push(char::from(ALPHABET.get(index).copied().unwrap_or(b'A')));
        }
    }
    out
}
