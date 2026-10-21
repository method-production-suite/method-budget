//! Invites (decisions.md, entry 97; docs/research/collaboration.md, 9.1).
//!
//! Someone who may give a role records an [`Op::Invite`]: a random id, the
//! role (a department's categories and accounts by uid, as a grant names
//! them), an expiry, and the public half of a fresh Ed25519 key whose
//! 32-byte seed is the invite's secret. The secret goes only into the link
//! the inviter copies and sends; no computer keeps it, and the log never
//! holds it.
//!
//! The newcomer's app joins the folder with its own actor and key and
//! records an [`Op::Claim`]: the invite's id and the invite key's signature
//! over the invite and the claimant's actor ([`claim_message`]). A claim in
//! the folder proves its claimant had the link, and can't be copied into
//! someone else's name, which a claim carrying the secret itself could.
//!
//! Every computer judges a claim ([`judge`]): the invite is within the
//! claim's `deps`; the proof checks against the invite's key; the claim's
//! own time is no later than the expiry; the inviter could still give that
//! role where the claim was made; and, judged against changes the claim
//! may never have seen ([`cut`]):
//!
//! - **Cancel** is a cutoff, like a removal (entry 89): a claim the
//!   canceller hadn't seen (outside the cancel's `deps`) is refused,
//!   whatever it claims to have seen. A claim the canceller had seen stands:
//!   the invite was used before it was cancelled. Removing the inviter
//!   cancels their invites the same way.
//! - **Used once**: of two claims of one invite, the one that saw the other
//!   loses; of two made apart, the lower (Lamport clock, actor, number)
//!   wins. Every computer settles them alike, whatever order they arrive
//!   in; a claim that loses is refused and listed.
//!
//! A valid claim gives the claimant the invite's role exactly as a grant by
//! the inviter would ([`super::State::apply`]). Expiry is the claim's own
//! wall-clock time, a claim like every change's time: someone holding the
//! link whose clock is wrong (or set back on purpose) can join after the
//! expiry. Cancel is the dependable way to take an invite back.

use super::access::{self, Bounds};
use super::clock::{ChangeId, Frontier};
use super::ids::{ActorId, Uid};
use super::op::Change;
use super::state::State;

/// What an invite key signs for a claim, before the invite and the actor.
pub const CLAIM_DOMAIN: &[u8] = b"method-budget-log claim v1\n";

/// The bytes an invite key signs to let `actor` take up invite `invite`.
pub fn claim_message(invite: Uid, actor: ActorId) -> Vec<u8> {
    let mut m = CLAIM_DOMAIN.to_vec();
    m.extend_from_slice(invite.hex().as_bytes());
    m.push(b'\n');
    m.extend_from_slice(actor.hex().as_bytes());
    m
}

/// Why a claim is refused, in words for the list of changes kept aside.
/// Each starts the reason after [`super::refused::INVITE`].
pub mod why {
    pub const NOT_FOUND: &str = "the invite isn't in this budget where the claim was made";
    pub const WRONG_SECRET: &str = "it doesn't hold the invite's secret";
    pub const CANT_CHECK: &str = "the invite's secret can't be checked here";
    pub const EXPIRED: &str = "it was made after the invite expired";
    pub const BAD_TIME: &str = "its time can't be read, so the expiry can't be checked";
    pub const CANCELLED: &str = "the invite was cancelled before it reached the canceller";
    pub const USED: &str = "the invite was already used by someone else";
    pub const OWNER: &str = "the owner doesn't take up an invite";
    pub const NOT_ALLOWED: &str = "the person who made the invite could no longer give that role";
}

/// A claim applied: what judges claims of the same invite made apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimBound {
    pub id: ChangeId,
    pub invite: Uid,
    pub lamport: u64,
    pub deps: Frontier,
}

/// A cancel applied: claims of its invite outside `deps` are refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelBound {
    pub id: ChangeId,
    pub invite: Uid,
    pub deps: Frontier,
}

/// Whether claim `a` wins over claim `b` of the same invite: `b` saw `a`,
/// or they were made apart and `a` is the lower (Lamport, actor, number).
fn wins(a: &ClaimBound, b: &ClaimBound) -> bool {
    if a.id == b.id {
        return false;
    }
    if b.deps.includes(a.id) {
        return true;
    }
    !a.deps.includes(b.id) && (a.lamport, a.id.actor, a.id.seq) < (b.lamport, b.id.actor, b.id.seq)
}

/// The part of judging `change`'s claim that depends on changes it may
/// never have seen: a cancel or a removal of the inviter it falls after, or
/// another claim that wins the invite. `None` when nothing in `bounds`
/// stands against it (or it holds no claim).
pub fn cut(state: &State, bounds: &Bounds, change: &Change) -> Option<&'static str> {
    let (invite, _) = change.claim()?;
    let me = ClaimBound {
        id: change.id(),
        invite,
        lamport: change.lamport,
        deps: change.deps.clone(),
    };
    if bounds
        .cancels
        .iter()
        .any(|c| c.invite == invite && !c.deps.includes(me.id))
    {
        return Some(why::CANCELLED);
    }
    if let Some(rec) = state.invites().get(&invite) {
        let inviter = access::person_at(state, rec.by, &rec.deps);
        // A removal the invite was made after doesn't count: the inviter
        // was given a role again before inviting.
        if bounds
            .removals
            .iter()
            .any(|r| r.person == inviter && !rec.deps.includes(r.id) && !r.deps.includes(me.id))
        {
            return Some(why::CANCELLED);
        }
    }
    if bounds
        .claims
        .iter()
        .any(|c| c.invite == invite && wins(c, &me))
    {
        return Some(why::USED);
    }
    None
}

/// Judges `change`'s claim, if it holds one: everything in the module's
/// description, checking the proof with `verifier` (`None`: there is none
/// to check with, so the claim is refused).
pub fn judge(
    state: &State,
    bounds: &Bounds,
    change: &Change,
    verifier: Option<&dyn super::Verifier>,
) -> Result<(), &'static str> {
    let Some((invite, proof)) = change.claim() else {
        return Ok(());
    };
    let Some(rec) = state.invites().get(&invite) else {
        return Err(why::NOT_FOUND);
    };
    if !change.deps.includes(rec.change) {
        return Err(why::NOT_FOUND);
    }
    let Some(verifier) = verifier else {
        return Err(why::CANT_CHECK);
    };
    if !verifier.verify_message(&rec.key, &claim_message(invite, change.actor), &proof) {
        return Err(why::WRONG_SECRET);
    }
    if state.owner() == Some(access::person_of_change(state, change)) {
        return Err(why::OWNER);
    }
    match (rfc3339_secs(&change.time), rfc3339_secs(&rec.expires)) {
        (Some(at), Some(until)) => {
            if at > until {
                return Err(why::EXPIRED);
            }
        }
        _ => return Err(why::BAD_TIME),
    }
    let granter = access::role_at(state, rec.by, &change.deps);
    let target = access::role_at(state, change.actor, &change.deps);
    if !access::may_give(&granter, &target, &rec.role) {
        return Err(why::NOT_ALLOWED);
    }
    if let Some(w) = cut(state, bounds, change) {
        return Err(w);
    }
    Ok(())
}

/// Whether the invite can still be taken up as `state` stands, at time
/// `now` (seconds since 1970, UTC): not used, not cancelled, not expired.
pub fn open(rec: &super::state::InviteRecord, now: i64) -> bool {
    rec.claimed.is_none()
        && rec.cancelled.is_none()
        && rfc3339_secs(&rec.expires).is_some_and(|until| now <= until)
}

/// Seconds since 1970-01-01T00:00:00Z of an RFC 3339 time
/// (`2026-10-21T14:03:22Z`, with fractions of a second or an offset such as
/// `+02:00`). `None` for anything else.
pub fn rfc3339_secs(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> {
        let t = s.get(a..z)?;
        if !t.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        t.parse().ok()
    };
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hh, mm, ss) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &frac[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let r = rest.as_bytes();
            if r.len() != 6 || !matches!(r[0], b'+' | b'-') || r[3] != b':' {
                return None;
            }
            let oh: i64 = rest.get(1..3)?.parse().ok()?;
            let om: i64 = rest.get(4..6)?.parse().ok()?;
            if oh > 23 || om > 59 {
                return None;
            }
            let o = oh * 3600 + om * 60;
            if r[0] == b'-' {
                -o
            } else {
                o
            }
        }
    };
    Some(days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss - offset)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// public-domain `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `secs` since 1970 as RFC 3339 UTC to the second (what an invite's
/// expiry is written as).
pub fn rfc3339_at(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_times_read_and_write() {
        assert_eq!(rfc3339_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_secs("2025-10-12T00:00:00Z"), Some(1_760_227_200));
        assert_eq!(
            rfc3339_secs("2025-10-12T02:00:00+02:00"),
            Some(1_760_227_200)
        );
        assert_eq!(
            rfc3339_secs("2025-10-11T19:00:00.250-05:00"),
            Some(1_760_227_200)
        );
        for bad in [
            "",
            "yesterday",
            "2025-10-12",
            "2025-13-01T00:00:00Z",
            "2025-10-12T00:00:00",
        ] {
            assert_eq!(rfc3339_secs(bad), None, "{bad}");
        }
        for secs in [0i64, 86_399, 951_782_400, 1_760_227_200, 4_102_444_800] {
            assert_eq!(rfc3339_secs(&rfc3339_at(secs)), Some(secs));
        }
        assert_eq!(rfc3339_at(1_760_227_200), "2025-10-12T00:00:00Z");
    }

    #[test]
    fn a_claim_message_names_the_invite_and_the_actor() {
        let m = claim_message(Uid(7), ActorId([1; 32]));
        assert!(m.starts_with(CLAIM_DOMAIN));
        assert_ne!(m, claim_message(Uid(7), ActorId([2; 32])));
        assert_ne!(m, claim_message(Uid(8), ActorId([1; 32])));
    }
}
