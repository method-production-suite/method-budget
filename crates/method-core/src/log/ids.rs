//! Identities that never change and need no coordination: actors, entity
//! ids and the random row ids of a shared budget (docs/research/
//! collaboration.md, 3.3).

use std::fmt;

use crate::model::RowId;

/// One person on one computer. 32 bytes: in phase 3 of the collaboration
/// design this is the computer's ed25519 public key; until then it is 32
/// random bytes made the first time the computer joins a shared budget.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ActorId(pub [u8; 32]);

impl ActorId {
    /// The actor whose bytes are all zero: a placeholder for tests and for
    /// the stamp of the root entity. Never a real actor.
    pub const NONE: ActorId = ActorId([0; 32]);

    /// The 64 hex characters of the id.
    pub fn hex(&self) -> String {
        hex(&self.0)
    }

    /// The first 8 hex characters: enough to tell actors apart in text.
    pub fn short(&self) -> String {
        self.hex()[..8].to_string()
    }

    /// Reads 64 hex characters (either case).
    pub fn parse(s: &str) -> Option<ActorId> {
        let bytes = unhex(s.trim())?;
        bytes.try_into().ok().map(ActorId)
    }

    /// A fresh random actor id from `ids`.
    pub fn random(ids: &mut Ids) -> ActorId {
        let mut out = [0u8; 32];
        for chunk in out.chunks_mut(8) {
            chunk.copy_from_slice(&ids.next_u64().to_le_bytes());
        }
        ActorId(out)
    }
}

impl fmt::Debug for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "actor:{}", self.short())
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

/// The stable identity of an entity in the log: a category, an account,
/// a row, a setup item, a note, a charge... 128 random bits, drawn by the
/// actor that creates the entity. Never written into the `.mthd`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Uid(pub u128);

impl Uid {
    /// The budget itself: its settings are fields of this entity, and the
    /// topsheet, charges, credits and versions are its lists.
    pub const ROOT: Uid = Uid(0);

    /// The 32 hex characters of the id.
    pub fn hex(&self) -> String {
        format!("{:032x}", self.0)
    }

    /// Reads 32 hex characters (either case).
    pub fn parse(s: &str) -> Option<Uid> {
        let s = s.trim();
        if s.len() != 32 {
            return None;
        }
        u128::from_str_radix(s, 16).ok().map(Uid)
    }

    /// A fresh random id from `ids`, never [`Uid::ROOT`].
    pub fn random(ids: &mut Ids) -> Uid {
        loop {
            let v = ((ids.next_u64() as u128) << 64) | ids.next_u64() as u128;
            if v != 0 {
                return Uid(v);
            }
        }
    }
}

impl fmt::Debug for Uid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Uid::ROOT {
            f.write_str("uid:root")
        } else {
            write!(f, "uid:{}", &self.hex()[..8])
        }
    }
}

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

/// Lowest row id a shared budget draws: 2^32, above anything a counter
/// would have reached.
pub const SHARED_ROW_ID_MIN: RowId = 1 << 32;
/// One past the highest row id a shared budget draws: 2^53, so every id
/// is exact in JavaScript readers (docs/FORMAT.md, "Big integers").
pub const SHARED_ROW_ID_END: RowId = 1 << 53;

/// A row id for a row made in a shared budget: random in
/// `[2^32, 2^53)` and not one `taken` says is in use (every id the budget
/// knows, versions included).
pub fn random_row_id(ids: &mut Ids, taken: impl Fn(RowId) -> bool) -> RowId {
    loop {
        let id = SHARED_ROW_ID_MIN + ids.next_u64() % (SHARED_ROW_ID_END - SHARED_ROW_ID_MIN);
        if !taken(id) {
            return id;
        }
    }
}

/// A row id derived from a 64-bit value (a hash of the op that created a
/// colliding row): the same input always gives the same id, in the shared
/// range.
pub fn derived_row_id(from: u64) -> RowId {
    SHARED_ROW_ID_MIN + from % (SHARED_ROW_ID_END - SHARED_ROW_ID_MIN)
}

/// A source of random bits for ids: xoshiro256**, seeded from the operating
/// system ([`Ids::from_os`]) or from a fixed seed in tests ([`Ids::seeded`]).
/// Not for secrets: ids only have to be unique.
#[derive(Clone, Debug)]
pub struct Ids {
    s: [u64; 4],
}

impl Ids {
    /// Seeded from the operating system's entropy, through the standard
    /// library's randomly keyed hasher (the one thing in `std` that reads
    /// OS randomness), mixed with the process id and this call's stack
    /// address.
    pub fn from_os() -> Ids {
        use std::hash::{BuildHasher, Hasher};
        let state = std::collections::hash_map::RandomState::new();
        let marker = 0u8;
        let salt = (std::process::id() as u64) ^ (&marker as *const u8 as u64).rotate_left(32);
        let mut s = [0u64; 4];
        for (i, word) in s.iter_mut().enumerate() {
            let mut h = state.build_hasher();
            h.write_u64(i as u64);
            h.write_u64(salt);
            *word = h.finish();
        }
        let mut ids = Ids::seeded(s[0] ^ s[1].rotate_left(17) ^ s[2].rotate_left(34) ^ s[3]);
        // Mix every word in, not just the fold.
        for (i, word) in s.iter().enumerate() {
            ids.s[i] ^= *word;
        }
        if ids.s == [0; 4] {
            ids.s[0] = 1;
        }
        ids
    }

    /// A deterministic source: the same seed gives the same ids.
    pub fn seeded(seed: u64) -> Ids {
        // splitmix64 fills the state from one word.
        let mut x = seed;
        let mut s = [0u64; 4];
        for word in &mut s {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            *word = z ^ (z >> 31);
        }
        if s == [0; 4] {
            s[0] = 1;
        }
        Ids { s }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }
}

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The bytes of a hex string (either case), or `None` if it isn't one.
pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_hex() {
        let mut ids = Ids::seeded(7);
        let a = ActorId::random(&mut ids);
        assert_eq!(ActorId::parse(&a.hex()), Some(a));
        let u = Uid::random(&mut ids);
        assert_eq!(Uid::parse(&u.hex()), Some(u));
        assert_eq!(Uid::parse("zz"), None);
        assert_eq!(ActorId::parse("00"), None);
    }

    #[test]
    fn seeded_ids_repeat_and_os_ids_differ() {
        let mut a = Ids::seeded(1);
        let mut b = Ids::seeded(1);
        assert_eq!(a.next_u64(), b.next_u64());
        let mut x = Ids::from_os();
        let mut y = Ids::from_os();
        assert_ne!(x.next_u64(), y.next_u64());
    }

    #[test]
    fn row_ids_stay_in_the_shared_range() {
        let mut ids = Ids::seeded(3);
        for _ in 0..1000 {
            let id = random_row_id(&mut ids, |_| false);
            assert!((SHARED_ROW_ID_MIN..SHARED_ROW_ID_END).contains(&id));
        }
        let taken = random_row_id(&mut Ids::seeded(3), |_| false);
        assert_ne!(random_row_id(&mut Ids::seeded(3), |id| id == taken), taken);
        assert!((SHARED_ROW_ID_MIN..SHARED_ROW_ID_END).contains(&derived_row_id(u64::MAX)));
    }
}
