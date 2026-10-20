//! Signed changes (docs/research/collaboration.md, section 7; decisions.md,
//! entry 89).
//!
//! Each person's computer has a key pair. Its public key goes into the log
//! with a [`super::Op::Key`] in a change of its own, signed by that key, and
//! from then on every change it writes is signed. Every app checks every
//! signature against the key its author published before the change is
//! applied; a change whose signature is missing or doesn't match is kept
//! aside and listed, as a change beyond its author's role is.
//!
//! The engine holds no cryptography and no randomness of its own (it
//! builds for a browser and stays dependency-free): what a signature is
//! made over (the change's line in the log file) and the Ed25519 maths
//! live in `method-format`, which hands the log a [`Signer`] and a
//! [`Verifier`]. A log without a verifier can't check a signature, so it
//! keeps aside every change that needs one.
//!
//! Signing starts for the whole budget with the owner's key (the owner's
//! first change holding a `key`, normally the same change as the first
//! role): from then on a change must be signed when its author has a key,
//! when it was made after the owner's key, or when its author is a
//! stranger to the owner's key change (a computer it had never seen and no
//! role named), whatever the change claims to have seen.

use std::fmt;

use super::ids::hex;

/// A person's Ed25519 public key, as the log publishes it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    /// The 64 hex characters of the key.
    pub fn hex(&self) -> String {
        hex(&self.0)
    }

    /// Reads 64 hex characters.
    pub fn parse(s: &str) -> Option<PublicKey> {
        let bytes = super::ids::unhex(s.trim())?;
        bytes.try_into().ok().map(PublicKey)
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "key:{}", &self.hex()[..8])
    }
}

/// An Ed25519 signature over a change.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    /// The 128 hex characters of the signature.
    pub fn hex(&self) -> String {
        hex(&self.0)
    }

    /// Reads 128 hex characters.
    pub fn parse(s: &str) -> Option<Signature> {
        let bytes = super::ids::unhex(s.trim())?;
        bytes.try_into().ok().map(Signature)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sig:{}", &self.hex()[..8])
    }
}

/// This computer's key: signs the changes it records.
pub trait Signer: Send + Sync {
    /// The public key, as published.
    fn public(&self) -> PublicKey;
    /// The signature of `change` (its `sig` left out of what is signed).
    fn sign(&self, change: &super::Change) -> Signature;
}

/// Checks a change's signature against a published key.
pub trait Verifier: Send + Sync {
    /// Whether `change.sig` is `key`'s signature of `change`.
    fn verify(&self, key: &PublicKey, change: &super::Change) -> bool;
}

/// Why a change's signature doesn't pass, in words for the list of changes
/// kept aside.
pub mod why {
    pub const MISSING: &str = "it isn't signed, and changes in this budget must be";
    pub const MISMATCH: &str = "its signature doesn't match the person's key";
    pub const NO_KEY: &str = "it is signed, but its author has no key in this budget";
    pub const CANT_CHECK: &str = "signatures can't be checked here";
    pub const KEY_TAKEN: &str =
        "a key in this computer's name is in the budget and it isn't this computer's";
    pub const TWO_KEYS: &str = "a person's key is published once and doesn't change";
    pub const NOT_PERSONS_KEY: &str =
        "a command line or agent signs with its person's key, and this one doesn't";
    pub const PERSON_HAS_NO_KEY: &str =
        "a command line or agent writes for a person only once that person's key is in the budget";
}
