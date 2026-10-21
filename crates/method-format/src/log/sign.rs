//! Signed changes on disk (decisions.md, entry 89; docs/FORMAT.md, "The
//! change log"): Ed25519 over a change's line in its segment file, and this
//! computer's key file.
//!
//! What is signed is the change's canonical line without its `sig` field
//! ([`super::unsigned_line`]), after a fixed prefix naming what it is, so a
//! signature over a change can't be passed off as one over anything else.
//! The line is canonical (the same change always gives the same bytes), so
//! a reader checks the line it rebuilds from what it read; a line written
//! any other way doesn't match and is kept aside.
//!
//! The key file holds the 32-byte secret seed as 64 hex characters, beside
//! this computer's actor id (`~/.method/signing-key`), readable by its
//! owner only and never in a shared folder. The command line and the app
//! read the same file, so a program writing for this computer's person
//! signs with the person's key.

use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature as EdSignature, SigningKey, VerifyingKey};
use method_core::log::{Change, PublicKey, Signature, Signer, Verifier};

use super::{unsigned_line, LogFileError};

/// What every signed message starts with.
const DOMAIN: &[u8] = b"method-budget-log change v1\n";

fn message(change: &Change) -> Vec<u8> {
    let line = unsigned_line(change);
    let mut m = Vec::with_capacity(DOMAIN.len() + line.len());
    m.extend_from_slice(DOMAIN);
    m.extend_from_slice(line.as_bytes());
    m
}

/// Checks Ed25519 signatures over changes (strict: no malleable or weak
/// keys).
#[derive(Clone, Copy, Debug, Default)]
pub struct Ed25519;

impl Verifier for Ed25519 {
    fn verify(&self, key: &PublicKey, change: &Change) -> bool {
        let Some(sig) = change.sig else {
            return false;
        };
        let Ok(vk) = VerifyingKey::from_bytes(&key.0) else {
            return false;
        };
        vk.verify_strict(&message(change), &EdSignature::from_bytes(&sig.0))
            .is_ok()
    }

    fn verify_message(&self, key: &PublicKey, message: &[u8], sig: &Signature) -> bool {
        let Ok(vk) = VerifyingKey::from_bytes(&key.0) else {
            return false;
        };
        vk.verify_strict(message, &EdSignature::from_bytes(&sig.0))
            .is_ok()
    }
}

// ---------------------------------------------------------------------------
// Invites (decisions.md, entry 97)

/// An invite's secret: 32 bytes, the seed of an Ed25519 key. Only ever in the
/// link; the log holds its public key ([`invite_key`]).
pub type InviteSecret = [u8; 32];

/// A fresh invite secret from the system's random numbers.
#[cfg(not(target_arch = "wasm32"))]
pub fn new_invite_secret() -> Result<InviteSecret, LogFileError> {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed)
        .map_err(|e| LogFileError(format!("no random numbers for an invite: {e}")))?;
    Ok(seed)
}

/// In a browser there are no system random numbers here: invites are made
/// by the desktop app.
#[cfg(target_arch = "wasm32")]
pub fn new_invite_secret() -> Result<InviteSecret, LogFileError> {
    Err(LogFileError("invites are made by the desktop app".into()))
}

/// A fresh random invite id (16 bytes from the same source).
#[cfg(not(target_arch = "wasm32"))]
pub fn new_invite_id() -> Result<method_core::log::Uid, LogFileError> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b)
        .map_err(|e| LogFileError(format!("no random numbers for an invite: {e}")))?;
    Ok(method_core::log::Uid(u128::from_le_bytes(b).max(1)))
}

/// In a browser: see [`new_invite_secret`].
#[cfg(target_arch = "wasm32")]
pub fn new_invite_id() -> Result<method_core::log::Uid, LogFileError> {
    Err(LogFileError("invites are made by the desktop app".into()))
}

/// The public key an invite with `secret` publishes.
pub fn invite_key(secret: &InviteSecret) -> PublicKey {
    KeyPair::from_seed(*secret).public()
}

/// The proof that `actor` holds invite `invite`'s secret: the invite key's
/// signature over [`method_core::log::invite::claim_message`].
pub fn claim_proof(
    secret: &InviteSecret,
    invite: method_core::log::Uid,
    actor: method_core::log::ActorId,
) -> Signature {
    use ed25519_dalek::Signer as _;
    let k = SigningKey::from_bytes(secret);
    let m = method_core::log::invite::claim_message(invite, actor);
    Signature(k.sign(&m).to_bytes())
}

/// A key pair: what this computer signs its changes with.
pub struct KeyPair(SigningKey);

impl KeyPair {
    /// The key pair whose secret seed is `seed`.
    pub fn from_seed(seed: [u8; 32]) -> KeyPair {
        KeyPair(SigningKey::from_bytes(&seed))
    }
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secret.
        write!(f, "KeyPair({:?})", self.public())
    }
}

impl Signer for KeyPair {
    fn public(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().to_bytes())
    }

    fn sign(&self, change: &Change) -> Signature {
        use ed25519_dalek::Signer as _;
        Signature(self.0.sign(&message(change)).to_bytes())
    }
}

/// The key file's name, beside the actor id.
pub const KEY_FILE: &str = "signing-key";

/// Where the key of the person whose actor id is kept in `actor_file` lives:
/// `signing-key` beside it.
pub fn key_path(actor_file: &Path) -> PathBuf {
    actor_file.with_file_name(KEY_FILE)
}

/// Reads the key pair kept in `path`.
pub fn read_key(path: &Path) -> Result<KeyPair, LogFileError> {
    let text = crate::fsio::read_to_string_capped(path, 1024)
        .map_err(|e| LogFileError(format!("{}: {e}", path.display())))?;
    let bytes = method_core::log::ids::unhex(text.trim())
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
        .ok_or_else(|| LogFileError(format!("{}: not a key", path.display())))?;
    Ok(KeyPair::from_seed(bytes))
}

/// The key pair kept in `path`, made the first time from the system's
/// random numbers and written readable by its owner only. Never over a key
/// already there: two programs making one at once keep the first.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_or_make_key(path: &Path) -> Result<KeyPair, LogFileError> {
    if path.is_file() {
        return read_key(path);
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed)
        .map_err(|e| LogFileError(format!("no random numbers for a key: {e}")))?;
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&dir).map_err(|e| LogFileError(format!("{}: {e}", dir.display())))?;
    // Written whole to a private temporary file, then linked into place,
    // which fails if a key appeared meanwhile.
    let tmp = dir.join(format!(".{KEY_FILE}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    {
        use std::io::Write;
        let mut f = crate::fsio::create_new(&tmp, crate::fsio::Access::Private)
            .map_err(|e| LogFileError(format!("{}: {e}", tmp.display())))?;
        let text = format!("{}\n", method_core::log::ids::hex(&seed));
        f.write_all(text.as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| LogFileError(format!("{}: {e}", tmp.display())))?;
    }
    let linked = std::fs::hard_link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(KeyPair::from_seed(seed)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => read_key(path),
        Err(e) => Err(LogFileError(format!("{}: {e}", path.display()))),
    }
}

/// In a browser there is no key file and no system randomness here: keys
/// are made by the desktop app and the command line only.
#[cfg(target_arch = "wasm32")]
pub fn load_or_make_key(path: &Path) -> Result<KeyPair, LogFileError> {
    read_key(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::log::{ActorId, Frontier, Op, Uid, Value};

    fn change() -> Change {
        Change {
            actor: ActorId([4; 32]),
            seq: 2,
            deps: Frontier::new(),
            lamport: 3,
            time: "2026-10-20T00:00:00Z".into(),
            label: "Edit".into(),
            ops: vec![Op::Set {
                entity: Uid::ROOT,
                field: method_core::log::Field::Title,
                value: Value::text("Signed"),
            }],
            sig: None,
        }
    }

    #[test]
    fn a_signature_verifies_and_a_changed_byte_does_not() {
        let k = KeyPair::from_seed([9; 32]);
        let mut c = change();
        c.sig = Some(k.sign(&c));
        assert!(Ed25519.verify(&k.public(), &c));
        // Read back from its line, it still verifies.
        let back = super::super::change_from_json(&super::super::change_to_json(&c)).unwrap();
        assert!(Ed25519.verify(&k.public(), &back));
        // Another key, another label, no signature: none verify.
        assert!(!Ed25519.verify(&KeyPair::from_seed([8; 32]).public(), &c));
        let mut changed = c.clone();
        changed.label = "Edit!".into();
        assert!(!Ed25519.verify(&k.public(), &changed));
        let mut bare = c.clone();
        bare.sig = None;
        assert!(!Ed25519.verify(&k.public(), &bare));
        // Signing twice gives the same bytes (Ed25519 is deterministic).
        assert_eq!(k.sign(&c), k.sign(&bare));
    }

    #[test]
    fn a_claim_proof_checks_for_its_actor_only() {
        use method_core::log::invite::claim_message;
        let secret = [5u8; 32];
        let key = invite_key(&secret);
        let invite = Uid(42);
        let p = claim_proof(&secret, invite, ActorId([1; 32]));
        assert!(Ed25519.verify_message(&key, &claim_message(invite, ActorId([1; 32])), &p));
        // Copied into someone else's name, or for another invite, it fails;
        // so does another secret's proof.
        assert!(!Ed25519.verify_message(&key, &claim_message(invite, ActorId([2; 32])), &p));
        assert!(!Ed25519.verify_message(&key, &claim_message(Uid(43), ActorId([1; 32])), &p));
        let other = claim_proof(&[6u8; 32], invite, ActorId([1; 32]));
        assert!(!Ed25519.verify_message(&key, &claim_message(invite, ActorId([1; 32])), &other));
        // Fresh secrets and ids differ.
        assert_ne!(new_invite_secret().unwrap(), new_invite_secret().unwrap());
        assert_ne!(new_invite_id().unwrap(), new_invite_id().unwrap());
    }

    #[test]
    fn the_signature_is_the_lines_last_field() {
        let k = KeyPair::from_seed([9; 32]);
        let mut c = change();
        c.sig = Some(k.sign(&c));
        let line = super::super::change_to_json(&c);
        let unsigned = super::super::unsigned_line(&c);
        assert!(line.starts_with(&unsigned[..unsigned.len() - 1]));
        assert!(line.ends_with(&format!(",\"sig\":\"{}\"}}", c.sig.unwrap().hex())));
    }

    #[test]
    fn the_key_file_is_private_and_kept() {
        let dir = std::env::temp_dir().join(format!("method-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = key_path(&dir.join("actor"));
        assert_eq!(path, dir.join("signing-key"));
        let k = load_or_make_key(&path).unwrap();
        let again = load_or_make_key(&path).unwrap();
        assert_eq!(k.public(), again.public());
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.trim().len(), 64);
        // The secret is never what Debug shows.
        assert!(!format!("{k:?}").contains(text.trim()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
        // No temporary file is left beside it.
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(left.len(), 1);
        std::fs::write(&path, "not a key").unwrap();
        assert!(load_or_make_key(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
