//! A file's bytes in the command contract's JSON: one base64 string
//! (standard alphabet, padded), which a browser makes and reads with
//! `btoa` and `atob` or `Uint8Array.fromBase64`. Kept here rather than
//! taking a dependency for two small functions.

use serde::{Deserialize, Deserializer, Serializer};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` as base64.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The bytes base64 `text` stands for. Whitespace is skipped; anything
/// else outside the alphabet, or padding in the wrong place, is refused.
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    let bad = || "the file's bytes aren't base64".to_string();
    let digits: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if digits.len() % 4 != 0 {
        return Err(bad());
    }
    let mut out = Vec::with_capacity(digits.len() / 4 * 3);
    let quads = digits.len() / 4;
    for (q, quad) in digits.chunks(4).enumerate() {
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && q + 1 != quads) {
            return Err(bad());
        }
        let mut n = 0u32;
        for &c in &quad[..4 - pad] {
            let v = ALPHABET.iter().position(|&a| a == c).ok_or_else(bad)?;
            n = (n << 6) | v as u32;
        }
        n <<= 6 * pad as u32;
        let three = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&three[..3 - pad]);
    }
    Ok(out)
}

/// For `#[serde(with = "b64")]` on a `Vec<u8>`.
pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&encode(bytes))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let text = String::deserialize(d)?;
    decode(&text).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_refuses_what_isnt_base64() {
        for (bytes, text) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"\xff\x00\xfe", "/wD+"),
        ] {
            assert_eq!(encode(bytes), text);
            assert_eq!(decode(text).unwrap(), bytes);
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&all)).unwrap(), all);
        assert_eq!(decode("Zm9v\nYg==").unwrap(), b"foob");
        for bad in ["Zm9", "Zm9v!", "Zg==Zg==", "Z==="] {
            assert!(decode(bad).is_err(), "{bad}");
        }
    }
}
