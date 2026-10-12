//! Logical time: Lamport stamps order writes, version vectors say what a
//! change had seen (docs/research/collaboration.md, 3.4). Wall clocks never
//! decide anything.

use std::collections::BTreeMap;
use std::fmt;

use super::ids::ActorId;

/// When a write happened, in logical time: the change's Lamport clock,
/// then its actor, then the change's sequence number and the op's index in
/// it. Two writes to one field are settled by comparing stamps: the higher
/// one wins on every computer, whatever order they arrived in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Stamp {
    pub lamport: u64,
    pub actor: ActorId,
    pub seq: u64,
    pub index: u32,
}

impl Stamp {
    /// The stamp of nothing: what the root entity was created with.
    pub const ZERO: Stamp = Stamp {
        lamport: 0,
        actor: ActorId::NONE,
        seq: 0,
        index: 0,
    };

    /// Which change made this write.
    pub fn change(&self) -> ChangeId {
        ChangeId {
            actor: self.actor,
            seq: self.seq,
        }
    }

    /// A 64-bit digest of the stamp, for ids derived from it.
    pub fn digest(&self) -> u64 {
        let mut bytes = Vec::with_capacity(52);
        bytes.extend_from_slice(&self.lamport.to_le_bytes());
        bytes.extend_from_slice(&self.actor.0);
        bytes.extend_from_slice(&self.seq.to_le_bytes());
        bytes.extend_from_slice(&self.index.to_le_bytes());
        let h = super::hash::sha256(&bytes);
        u64::from_le_bytes(h[..8].try_into().expect("8 bytes"))
    }
}

impl fmt::Debug for Stamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}@{}/{}.{}",
            self.lamport,
            self.actor.short(),
            self.seq,
            self.index
        )
    }
}

/// Which change: an actor and its sequence number (from 1, no gaps).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChangeId {
    pub actor: ActorId,
    pub seq: u64,
}

impl fmt::Debug for ChangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.actor.short(), self.seq)
    }
}

impl fmt::Display for ChangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.actor.short(), self.seq)
    }
}

/// A version vector: the last sequence number seen from each actor. A
/// change's `deps` is the frontier its author had when making it; a log's
/// frontier is everything it has applied.
#[derive(Clone, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
pub struct Frontier(pub BTreeMap<ActorId, u64>);

impl Frontier {
    pub fn new() -> Frontier {
        Frontier::default()
    }

    /// The last seq seen from `actor` (0 for none).
    pub fn get(&self, actor: &ActorId) -> u64 {
        self.0.get(actor).copied().unwrap_or(0)
    }

    /// Whether change `(actor, seq)` is within this frontier.
    pub fn includes(&self, id: ChangeId) -> bool {
        self.get(&id.actor) >= id.seq
    }

    /// Whether everything in `other` is within this frontier.
    pub fn covers(&self, other: &Frontier) -> bool {
        other.0.iter().all(|(a, s)| self.get(a) >= *s)
    }

    /// Records `seq` from `actor` (keeps the higher of the two).
    pub fn advance(&mut self, actor: ActorId, seq: u64) {
        let e = self.0.entry(actor).or_insert(0);
        if seq > *e {
            *e = seq;
        }
    }

    /// The pointwise maximum of the two.
    pub fn merge(&mut self, other: &Frontier) {
        for (a, s) in &other.0 {
            self.advance(*a, *s);
        }
    }

    /// How many changes it spans in all.
    pub fn len(&self) -> u64 {
        self.0.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.values().all(|s| *s == 0)
    }

    /// A short, stable digest of the frontier, for presence and diagnostics.
    pub fn digest(&self) -> String {
        let mut bytes = Vec::new();
        for (a, s) in &self.0 {
            bytes.extend_from_slice(&a.0);
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        super::ids::hex(&super::hash::sha256(&bytes)[..8])
    }
}

impl fmt::Debug for Frontier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut l = f.debug_map();
        for (a, s) in &self.0 {
            l.entry(&a.short(), s);
        }
        l.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(n: u8) -> ActorId {
        let mut a = [0u8; 32];
        a[0] = n;
        ActorId(a)
    }

    #[test]
    fn stamps_order_by_lamport_then_actor() {
        let a = Stamp {
            lamport: 5,
            actor: actor(2),
            seq: 1,
            index: 0,
        };
        let b = Stamp {
            lamport: 5,
            actor: actor(1),
            seq: 9,
            index: 3,
        };
        let c = Stamp {
            lamport: 6,
            actor: actor(1),
            seq: 1,
            index: 0,
        };
        assert!(b < a && a < c);
        assert_eq!(a.digest(), a.digest());
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn frontiers_cover_and_merge() {
        let mut f = Frontier::new();
        f.advance(actor(1), 3);
        f.advance(actor(1), 2);
        assert_eq!(f.get(&actor(1)), 3);
        let mut g = Frontier::new();
        g.advance(actor(2), 1);
        assert!(!f.covers(&g));
        f.merge(&g);
        assert!(f.covers(&g));
        assert!(f.includes(ChangeId {
            actor: actor(1),
            seq: 3
        }));
        assert!(!f.includes(ChangeId {
            actor: actor(1),
            seq: 4
        }));
        assert_eq!(f.len(), 4);
    }
}
