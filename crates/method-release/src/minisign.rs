//! The two minisign containers the updater deals in, read but never
//! verified.
//!
//! `cargo tauri build` with updater artifacts on signs each artifact and
//! writes `<artifact>.sig` beside it: the base64 of a minisign signature box.
//! The updater plugin checks that signature at install time against the
//! public key in `tauri.conf.json` (`plugins.updater.pubkey`), which is the
//! base64 of a minisign public key box. Neither step is ours. What is ours is
//! everything in between: the release manifest pastes each `.sig`'s content
//! into `latest.json`, and the app and the release workflow ask whether the
//! configured key is a key at all. Each wants to know "is this the container
//! it claims to be, and which key does it name?" without any cryptography.
//!
//! Shapes:
//!
//! ```text
//! signature box   untrusted comment: <text>
//!                 <base64 of 74 bytes: algorithm(2) key id(8) signature(64)>
//!                 trusted comment: <text>
//!                 <base64 of 64 bytes: the global signature>
//!
//! public key box  untrusted comment: <text>
//!                 <base64 of 42 bytes: algorithm(2) key id(8) key(32)>
//! ```
//!
//! The key id is the 8 bytes read little-endian and printed as uppercase
//! hex, as minisign and the Tauri CLI print it. It always comes from the
//! bytes, never from a comment line, which minisign itself calls untrusted.

/// What `tauri.conf.json` carries as `plugins.updater.pubkey` until the
/// release key exists (docs/RELEASING.md, "The updater key"). It is not a
/// key, so [`parse_public_key_box`] refuses it: the app never checks for
/// updates with it, and the release tooling writes no update manifest.
pub const PLACEHOLDER_PUBLIC_KEY: &str = "REPLACE-WITH-THE-RELEASE-PUBLIC-KEY";

const SIGNATURE_BYTES: usize = 74;
const PUBLIC_KEY_BYTES: usize = 42;
const GLOBAL_SIGNATURE_BYTES: usize = 64;

/// What a `.sig` file says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureBox {
    /// Uppercase hex of the little-endian key id.
    pub key_id: String,
    /// The `file:` field of the trusted comment, when the CLI wrote one.
    pub file_name: Option<String>,
    /// The `version:` field of the trusted comment: the version the archive
    /// was signed for (`cargo xtask sign-update` writes it; the app's
    /// updater requires it to match the manifest's version).
    pub version: Option<String>,
}

/// What a public key says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKeyBox {
    pub key_id: String,
}

/// Strict base64 (standard alphabet, `=` padding only at the end, length a
/// multiple of four). Anything else is an error, never skipped over: a
/// decoder that ignores what it doesn't understand turns "not base64 at
/// all" into some bytes.
pub fn decode_base64(what: &str, encoded: &str) -> Result<Vec<u8>, String> {
    let s = encoded.trim().as_bytes();
    if s.is_empty() {
        return Err(format!("{what}: empty, nothing to decode"));
    }
    if !s.len().is_multiple_of(4) {
        return Err(format!(
            "{what}: not base64 (its length isn't a multiple of four)"
        ));
    }
    let pad = s.iter().rev().take_while(|&&b| b == b'=').count();
    if pad > 2 {
        return Err(format!("{what}: not base64 (too much padding)"));
    }
    fn val(b: u8) -> Option<u32> {
        match b {
            b'A'..=b'Z' => Some(u32::from(b - b'A')),
            b'a'..=b'z' => Some(u32::from(b - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(b - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let body = &s[..s.len() - pad];
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for chunk in body.chunks(4) {
        let mut acc = 0u32;
        for (i, &b) in chunk.iter().enumerate() {
            let v = val(b).ok_or_else(|| {
                format!("{what}: not base64 (it has characters outside the base64 alphabet)")
            })?;
            acc |= v << (18 - 6 * i);
        }
        let bytes = acc.to_be_bytes();
        match chunk.len() {
            4 => out.extend_from_slice(&bytes[1..4]),
            3 => out.extend_from_slice(&bytes[1..3]),
            2 => out.push(bytes[1]),
            _ => return Err(format!("{what}: not base64 (a stray character at the end)")),
        }
    }
    Ok(out)
}

fn key_id_of(bytes: &[u8]) -> String {
    let mut id = [0u8; 8];
    id.copy_from_slice(&bytes[2..10]);
    format!("{:X}", u64::from_le_bytes(id))
}

/// Reads a `.sig` file's content (the base64 the CLI wrote) and says which
/// key signed it. Errors name the defect.
pub fn parse_signature_box(encoded: &str) -> Result<SignatureBox, String> {
    let raw = decode_base64("signature", encoded)?;
    let text =
        String::from_utf8(raw).map_err(|_| "signature: not text once decoded".to_string())?;
    let lines: Vec<&str> = text.trim_end().split('\n').collect();
    if lines.len() != 4 {
        return Err(format!(
            "signature: expected four lines (untrusted comment, signature, trusted comment, global signature), found {}",
            lines.len()
        ));
    }
    if !lines[0].starts_with("untrusted comment: ") {
        return Err("signature: doesn't begin with \"untrusted comment: \"".into());
    }
    let sig = decode_base64("signature line", lines[1])?;
    if sig.len() != SIGNATURE_BYTES {
        return Err(format!(
            "signature: the signature line decodes to {} bytes, not the {SIGNATURE_BYTES} of a minisign signature",
            sig.len()
        ));
    }
    let trusted = lines[2]
        .strip_prefix("trusted comment: ")
        .ok_or("signature: the third line isn't a \"trusted comment: \" line")?;
    let global = decode_base64("global signature line", lines[3])?;
    if global.len() != GLOBAL_SIGNATURE_BYTES {
        return Err(format!(
            "signature: the global signature decodes to {} bytes, not {GLOBAL_SIGNATURE_BYTES}",
            global.len()
        ));
    }
    let field = |name: &str| {
        trusted
            .split('\t')
            .find_map(|f| f.strip_prefix(name))
            .map(str::to_string)
    };
    Ok(SignatureBox {
        key_id: key_id_of(&sig),
        file_name: field("file:"),
        version: field("version:"),
    })
}

/// Reads a public key as `tauri.conf.json` carries it (the base64 the CLI
/// printed) and says which key it is. Errors name the defect; the
/// placeholder this repository ships until the release key exists is one of
/// them.
pub fn parse_public_key_box(encoded: &str) -> Result<PublicKeyBox, String> {
    if encoded.trim().is_empty() {
        return Err("public key: empty".into());
    }
    let raw = decode_base64("public key", encoded)?;
    let text =
        String::from_utf8(raw).map_err(|_| "public key: not text once decoded".to_string())?;
    let lines: Vec<&str> = text.trim_end().split('\n').collect();
    if lines.len() != 2 || !lines[0].starts_with("untrusted comment: ") {
        return Err(format!(
            "public key: expected an \"untrusted comment: \" line and a key line, found {} line(s)",
            lines.len()
        ));
    }
    let key = decode_base64("public key line", lines[1])?;
    if key.len() != PUBLIC_KEY_BYTES {
        return Err(format!(
            "public key: the key line decodes to {} bytes, not the {PUBLIC_KEY_BYTES} of a minisign public key",
            key.len()
        ));
    }
    Ok(PublicKeyBox {
        key_id: key_id_of(&key),
    })
}

/// Standard base64 with padding, for tests and fixtures.
pub fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Fixtures in the exact shapes the Tauri CLI writes, for tests here and in
/// the tools: a public key box and a signature box for key id `key_id`.
/// The key and signature bytes are filler; nothing here verifies them.
pub mod fixture {
    use super::encode_base64;

    fn id_bytes(key_id: u64) -> [u8; 8] {
        key_id.to_le_bytes()
    }

    /// `tauri.conf.json`'s `pubkey` for a key with id `key_id`.
    pub fn public_key(key_id: u64) -> String {
        let mut key = vec![b'E', b'd'];
        key.extend_from_slice(&id_bytes(key_id));
        key.extend_from_slice(&[7u8; 32]);
        let text = format!(
            "untrusted comment: minisign public key: {key_id:X}\n{}\n",
            encode_base64(&key)
        );
        encode_base64(text.as_bytes())
    }

    /// A `.sig` file's content for `file`, signed (in shape) by `key_id`,
    /// with no version in its trusted comment (as the Tauri CLI signs).
    pub fn signature(key_id: u64, file: &str) -> String {
        signature_with(key_id, &format!("timestamp:1760000000\tfile:{file}"))
    }

    /// The same, signed for `version` (as `cargo xtask sign-update` signs).
    pub fn signature_for_version(key_id: u64, file: &str, version: &str) -> String {
        signature_with(
            key_id,
            &format!("timestamp:1760000000\tfile:{file}\tversion:{version}"),
        )
    }

    fn signature_with(key_id: u64, trusted: &str) -> String {
        let mut sig = vec![b'E', b'D'];
        sig.extend_from_slice(&id_bytes(key_id));
        sig.extend_from_slice(&[9u8; 64]);
        let text = format!(
            "untrusted comment: signature from tauri secret key\n{}\ntrusted comment: {trusted}\n{}\n",
            encode_base64(&sig),
            encode_base64(&[3u8; 64])
        );
        encode_base64(text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_and_is_strict() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let enc = encode_base64(data);
            if !data.is_empty() {
                assert_eq!(decode_base64("t", &enc).unwrap(), data);
            }
        }
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        for bad in [
            "",
            "Zm9",
            "Zm9v YmFy",
            "Zm9v!mFy",
            "Z===",
            "Zm9vYmFy=",
            "=Zm9",
        ] {
            assert!(decode_base64("t", bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn reads_a_signature_and_its_key_id() {
        let sig = fixture::signature(0xE4D0668D54F6EB34, "Method-Budget.app.tar.gz");
        let parsed = parse_signature_box(&sig).unwrap();
        assert_eq!(parsed.key_id, "E4D0668D54F6EB34");
        assert_eq!(
            parsed.file_name.as_deref(),
            Some("Method-Budget.app.tar.gz")
        );
        assert_eq!(parsed.version, None, "the Tauri CLI names no version");
        let signed = fixture::signature_for_version(1, "a.app.tar.gz", "0.4.0");
        let parsed = parse_signature_box(&signed).unwrap();
        assert_eq!(parsed.version.as_deref(), Some("0.4.0"));
        assert_eq!(parsed.file_name.as_deref(), Some("a.app.tar.gz"));
    }

    #[test]
    fn reads_a_public_key_and_its_key_id() {
        let key = fixture::public_key(0xE4D0668D54F6EB34);
        assert_eq!(
            parse_public_key_box(&key).unwrap().key_id,
            "E4D0668D54F6EB34"
        );
    }

    #[test]
    fn refuses_malformed_signatures() {
        let good = fixture::signature(1, "a.app.tar.gz");
        let text = String::from_utf8(decode_base64("t", &good).unwrap()).unwrap();
        let lines: Vec<&str> = text.trim_end().split('\n').collect();
        let rebuild = |ls: &[&str]| encode_base64(format!("{}\n", ls.join("\n")).as_bytes());
        // Three lines, not four.
        assert!(parse_signature_box(&rebuild(&lines[..3])).is_err());
        // A signature line one byte short.
        let short = encode_base64(&decode_base64("t", lines[1]).unwrap()[..73]);
        assert!(parse_signature_box(&rebuild(&[lines[0], &short, lines[2], lines[3]])).is_err());
        // The comment lines swapped round.
        assert!(parse_signature_box(&rebuild(&[lines[2], lines[1], lines[0], lines[3]])).is_err());
        // Not base64, and a public key where a signature belongs.
        assert!(parse_signature_box("not a signature").is_err());
        assert!(parse_signature_box(&fixture::public_key(1)).is_err());
        assert!(parse_signature_box("").is_err());
    }

    #[test]
    fn refuses_what_isnt_a_public_key() {
        assert!(parse_public_key_box("").is_err());
        assert!(parse_public_key_box("   ").is_err());
        assert!(parse_public_key_box(PLACEHOLDER_PUBLIC_KEY).is_err());
        assert!(parse_public_key_box(&fixture::signature(1, "x")).is_err());
        let one_line = encode_base64(b"untrusted comment: minisign public key: 1\n");
        assert!(parse_public_key_box(&one_line).is_err());
    }
}
