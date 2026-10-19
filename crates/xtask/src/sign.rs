//! `cargo xtask sign-update`: signs each updater archive of a release with
//! the version in its signature's trusted comment.
//!
//! The Tauri CLI signs the archive it writes, but its trusted comment names
//! only the time and the file (`timestamp:<t>\tfile:<name>`). The app's
//! updater (method-update, decisions.md entry 83) requires the comment to
//! name the version the manifest announces, so a manifest can't pair a new
//! version number with an older signed archive. This signs again with the
//! same key, the same way and the same crate as the CLI, adding
//! `\tversion:<version>`, and writes `<archive>.sig` over the CLI's.
//!
//! The key comes from TAURI_SIGNING_PRIVATE_KEY (the key text, base64 of a
//! minisign secret-key box, or a path to a file holding it, as the CLI
//! accepts) and TAURI_SIGNING_PRIVATE_KEY_PASSWORD, the secrets the release
//! workflow already has. Every signature is checked against the public key
//! in tauri.conf.json before it is written: a key the app doesn't trust
//! writes nothing.

use std::path::Path;

use method_release::minisign::{decode_base64, encode_base64};

/// The untrusted comment the Tauri CLI writes.
const UNTRUSTED: &str = "signature from tauri secret key";

/// The trusted comment: the CLI's two fields, then the version.
pub fn trusted_comment(timestamp: u64, file_name: &str, version: &str) -> String {
    format!("timestamp:{timestamp}\tfile:{file_name}\tversion:{version}")
}

fn text_of(what: &str, base64: &str) -> Result<String, String> {
    String::from_utf8(decode_base64(what, base64)?).map_err(|_| format!("{what}: not text"))
}

/// The private key from TAURI_SIGNING_PRIVATE_KEY's value and its password.
pub fn load_secret_key(value: &str, password: &str) -> Result<minisign::SecretKey, String> {
    let text = if Path::new(value).is_file() {
        std::fs::read_to_string(value).map_err(|e| format!("could not read the key file: {e}"))?
    } else {
        value.to_string()
    };
    let boxed = minisign::SecretKeyBox::from_string(&text_of("the updater key", &text)?)
        .map_err(|e| format!("the updater key: {e}"))?;
    boxed
        .into_secret_key(Some(password.to_string()))
        .map_err(|_| "the updater key's password is wrong".to_string())
}

/// The public key `tauri.conf.json` carries (base64 of a minisign box).
pub fn load_public_key(pubkey: &str) -> Result<minisign::PublicKey, String> {
    minisign::PublicKeyBox::from_string(&text_of("plugins.updater.pubkey", pubkey)?)
        .and_then(|b| b.into_public_key())
        .map_err(|e| format!("plugins.updater.pubkey: {e}"))
}

/// The `.sig` file's content for `data`, published as `file_name`, signed
/// for `version` and checked against `pk`.
pub fn sign(
    sk: &minisign::SecretKey,
    pk: &minisign::PublicKey,
    data: &[u8],
    file_name: &str,
    version: &str,
    timestamp: u64,
) -> Result<String, String> {
    let sig = minisign::sign(
        Some(pk),
        sk,
        std::io::Cursor::new(data),
        Some(&trusted_comment(timestamp, file_name, version)),
        Some(UNTRUSTED),
    )
    .map_err(|e| format!("{file_name}: {e} (is this the key the app trusts?)"))?;
    Ok(encode_base64(sig.to_string().as_bytes()))
}

/// `cargo xtask sign-update --dir <artifacts> --version X.Y.Z`: every
/// archive in `dir` (each name carrying `_<version>_`) gets a `.sig` signed
/// for `version`. All or nothing: every signature is made before any is
/// written.
pub fn command(
    dir: &Path,
    version: &str,
    pubkey: &str,
    key: &str,
    password: &str,
) -> Result<String, String> {
    let pk = load_public_key(pubkey)?;
    let sk = load_secret_key(key, password)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("can't read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with(".sig"))
        .collect();
    names.sort();
    if names.is_empty() {
        return Err(format!("{}: no updater archives to sign", dir.display()));
    }
    let mut signed = Vec::new();
    for name in &names {
        crate::manifest::platforms_of(name, version)?;
        let data = std::fs::read(dir.join(name)).map_err(|e| format!("can't read {name}: {e}"))?;
        signed.push((name, sign(&sk, &pk, &data, name, version, timestamp)?));
    }
    let mut out = String::new();
    for (name, sig) in signed {
        std::fs::write(dir.join(format!("{name}.sig")), sig)
            .map_err(|e| format!("can't write {name}.sig: {e}"))?;
        out.push_str(&format!("signed {name} for {version}\n"));
    }
    Ok(out)
}

/// `cargo xtask verify-update --dir <artifacts> --version X.Y.Z`: every
/// archive in `dir` must have a `.sig` that the app's own updater
/// (method-update) accepts for `version` with the public key the app
/// trusts. Says what passed, or every archive that didn't.
pub fn verify_command(dir: &Path, version: &str, pubkey: &str) -> Result<String, String> {
    use method_update::verify::verify_signature;
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("can't read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with(".sig"))
        .collect();
    names.sort();
    if names.is_empty() {
        return Err(format!("{}: no updater archives to check", dir.display()));
    }
    let mut out = String::new();
    let mut problems = Vec::new();
    for name in &names {
        let sig = match std::fs::read_to_string(dir.join(format!("{name}.sig"))) {
            Ok(s) => s,
            Err(_) => {
                problems.push(format!("{name}: no signature beside it"));
                continue;
            }
        };
        let data = std::fs::read(dir.join(name)).map_err(|e| format!("can't read {name}: {e}"))?;
        match verify_signature(&data, sig.trim(), pubkey, version, true) {
            Ok(trusted) => out.push_str(&format!("{name}: signed for {version} ({trusted})\n")),
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(format!(
            "the app would refuse:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TempRepo;
    use method_release::minisign::parse_signature_box;

    /// A throwaway key pair, as `cargo tauri signer generate` writes it:
    /// (private, public), both base64 of their minisign boxes.
    fn keygen(password: &str) -> (String, String) {
        let kp = minisign::KeyPair::generate_encrypted_keypair(Some(password.to_string())).unwrap();
        let pk = kp.pk.to_box().unwrap().to_string();
        let sk = kp.sk.to_box(None).unwrap().to_string();
        (encode_base64(sk.as_bytes()), encode_base64(pk.as_bytes()))
    }

    const NAME: &str = "Method-Budget_0.4.0_aarch64.app.tar.gz";

    #[test]
    fn signs_each_archive_for_its_version_with_the_trusted_key() {
        let (sk, pk) = keygen("pass");
        let r = TempRepo::new("sign-update");
        r.write(&format!("dist/{NAME}"), "an update archive");
        // The Tauri CLI's signature, about to be replaced.
        r.write(&format!("dist/{NAME}.sig"), "old");
        let out = command(&r.root().join("dist"), "0.4.0", &pk, &sk, "pass").unwrap();
        assert_eq!(out, format!("signed {NAME} for 0.4.0\n"));
        let sig = r.read(&format!("dist/{NAME}.sig"));
        let parsed = parse_signature_box(&sig).unwrap();
        assert_eq!(parsed.version.as_deref(), Some("0.4.0"));
        assert_eq!(parsed.file_name.as_deref(), Some(NAME));
        // A real signature of these bytes by this key, with the version in
        // the part the key signs.
        let boxed = minisign::SignatureBox::from_string(&text_of("sig", &sig).unwrap()).unwrap();
        assert!(boxed
            .trusted_comment()
            .unwrap()
            .ends_with("\tversion:0.4.0"));
        let key = load_public_key(&pk).unwrap();
        let check = |data: &[u8]| {
            minisign::verify(&key, &boxed, std::io::Cursor::new(data), true, false, false)
        };
        assert!(check(b"an update archive").is_ok());
        assert!(check(b"another archive").is_err());
    }

    /// What `sign-update` writes passes the app's own check (method-update,
    /// which requires the version); the Tauri CLI's signature alone doesn't.
    #[test]
    fn the_app_accepts_what_this_signs_and_not_the_cli_s_alone() {
        use method_update::verify::verify_signature;
        let (sk_text, pk) = keygen("");
        let sk = load_secret_key(&sk_text, "").unwrap();
        let key = load_public_key(&pk).unwrap();
        let data = b"an update archive";
        let sig = sign(&sk, &key, data, NAME, "0.4.0", 1).unwrap();
        let trusted = verify_signature(data, &sig, &pk, "0.4.0", true).unwrap();
        assert_eq!(trusted, format!("timestamp:1\tfile:{NAME}\tversion:0.4.0"));
        assert!(verify_signature(data, &sig, &pk, "0.4.1", true).is_err());
        assert!(verify_signature(b"other", &sig, &pk, "0.4.0", true).is_err());
        // The CLI's trusted comment: timestamp and file only.
        let cli = minisign::sign(
            Some(&key),
            &sk,
            std::io::Cursor::new(&data[..]),
            Some(&format!("timestamp:1\tfile:{NAME}")),
            Some(UNTRUSTED),
        )
        .unwrap();
        let cli = encode_base64(cli.to_string().as_bytes());
        assert!(verify_signature(data, &cli, &pk, "0.4.0", false).is_ok());
        assert!(verify_signature(data, &cli, &pk, "0.4.0", true).is_err());
    }

    #[test]
    fn refuses_a_key_the_app_doesn_t_trust_and_writes_nothing() {
        let (sk, _) = keygen("");
        let (_, other_pk) = keygen("");
        let r = TempRepo::new("sign-update-other");
        r.write(&format!("dist/{NAME}"), "bytes");
        r.write(&format!("dist/{NAME}.sig"), "old");
        let e = command(&r.root().join("dist"), "0.4.0", &other_pk, &sk, "").unwrap_err();
        assert!(e.contains("is this the key the app trusts?"), "{e}");
        assert_eq!(r.read(&format!("dist/{NAME}.sig")), "old");
    }

    #[test]
    fn refuses_a_wrong_password_another_version_and_an_empty_folder() {
        let (sk, pk) = keygen("right");
        let r = TempRepo::new("sign-update-bad");
        let dir = r.root().join("dist");
        r.write("dist/.keep.sig", "");
        assert!(command(&dir, "0.4.0", &pk, &sk, "right")
            .unwrap_err()
            .contains("no updater archives"));
        r.write(&format!("dist/{NAME}"), "bytes");
        assert_eq!(
            command(&dir, "0.4.0", &pk, &sk, "wrong").unwrap_err(),
            "the updater key's password is wrong"
        );
        assert!(command(&dir, "0.5.0", &pk, &sk, "right")
            .unwrap_err()
            .contains("doesn't carry the version"));
        assert!(command(
            &dir,
            "0.4.0",
            "REPLACE-WITH-THE-RELEASE-PUBLIC-KEY",
            &sk,
            "right"
        )
        .is_err());
    }

    /// `verify-update`: what `sign-update` writes passes the app's own check;
    /// a changed archive, another version, a missing signature or another key
    /// don't.
    #[test]
    fn verify_passes_what_sign_update_wrote_and_nothing_else() {
        let (sk, pk) = keygen("pw");
        let r = TempRepo::new("verify-update");
        let dir = r.root().join("dist");
        assert!(verify_command(&dir, "0.4.0", &pk).is_err());
        r.write(&format!("dist/{NAME}"), "bytes");
        assert!(verify_command(&dir, "0.4.0", &pk)
            .unwrap_err()
            .contains("no signature"));
        command(&dir, "0.4.0", &pk, &sk, "pw").unwrap();
        let ok = verify_command(&dir, "0.4.0", &pk).unwrap();
        assert!(ok.contains(&format!("{NAME}: signed for 0.4.0")), "{ok}");
        assert!(verify_command(&dir, "0.4.1", &pk).is_err());
        let (_, other_pk) = keygen("");
        assert!(verify_command(&dir, "0.4.0", &other_pk).is_err());
        r.write(&format!("dist/{NAME}"), "changed bytes");
        assert!(verify_command(&dir, "0.4.0", &pk)
            .unwrap_err()
            .contains(NAME));
    }

    #[test]
    fn a_key_file_path_works_like_the_key_text() {
        let (sk, _) = keygen("");
        let r = TempRepo::new("sign-update-file");
        r.write("updater.key", &sk);
        let path = r.root().join("updater.key");
        assert!(load_secret_key(path.to_str().unwrap(), "").is_ok());
        assert!(load_secret_key("not a key", "").is_err());
    }
}
