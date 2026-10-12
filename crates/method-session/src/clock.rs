//! The time and the one source of uniqueness a session needs, handed in by
//! the host rather than read here, so the crate runs where `std`'s clock
//! panics (a browser's WebAssembly).
//!
//! Three things use it: the time each undo step was made (the History list
//! shows it), quick repeats joining one undo step and autosave's quiet
//! period (both measured on a clock that never goes back), and recovery
//! keys (unique to each open document in every running copy of the app).
//!
//! A host installs its [`Clock`] once, before the first session, with
//! [`install`]. The desktop app turns on the `std-clock` feature instead,
//! which makes the system clock the default; without it (a browser that
//! never installs one) the clock reads 0, so steps are undated and every
//! quick repeat joins its step until the host installs a real clock.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// Where a session gets the time.
pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch (0 when the clock is before it).
    fn wall_ms(&self) -> u64;
    /// Milliseconds on a clock that never goes back, from any start.
    fn mono_ms(&self) -> u64;
    /// A short name no other call, in this or any other running copy of
    /// the app, gives out: lowercase letters, digits and `-` only.
    fn unique(&self) -> String;
}

static CLOCK: OnceLock<Box<dyn Clock>> = OnceLock::new();

/// Makes `clock` the one every session uses. Only the first call counts,
/// and only before anything has read the time: `false` when it came too
/// late and the clock already in use stays.
pub fn install(clock: Box<dyn Clock>) -> bool {
    CLOCK.set(clock).is_ok()
}

fn clock() -> &'static dyn Clock {
    CLOCK.get_or_init(default_clock).as_ref()
}

/// Now, in milliseconds since the Unix epoch.
pub fn wall_ms() -> u64 {
    clock().wall_ms()
}

/// Now, in milliseconds on the clock that never goes back.
pub fn mono_ms() -> u64 {
    clock().mono_ms()
}

/// A name nobody else gets (see [`Clock::unique`]).
pub fn unique() -> String {
    clock().unique()
}

#[cfg(any(test, feature = "std-clock"))]
fn default_clock() -> Box<dyn Clock> {
    Box::new(SystemClock)
}

#[cfg(not(any(test, feature = "std-clock")))]
fn default_clock() -> Box<dyn Clock> {
    Box::new(Stopped)
}

/// No clock: always 0, and a counter for uniqueness (enough within one
/// page; a browser host installs its own).
#[cfg_attr(any(test, feature = "std-clock"), allow(dead_code))]
struct Stopped;

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl Clock for Stopped {
    fn wall_ms(&self) -> u64 {
        0
    }

    fn mono_ms(&self) -> u64 {
        0
    }

    fn unique(&self) -> String {
        format!("{:x}", COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// The operating system's clocks and the process id (the desktop).
#[cfg(any(test, feature = "std-clock"))]
pub struct SystemClock;

#[cfg(any(test, feature = "std-clock"))]
impl Clock for SystemClock {
    fn wall_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }

    fn mono_ms(&self) -> u64 {
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        let start = *START.get_or_init(std::time::Instant::now);
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn unique(&self) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        format!(
            "{:x}-{nanos:x}-{:x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_clock_is_the_system_clock() {
        assert!(wall_ms() > 1_600_000_000_000);
        let a = mono_ms();
        assert!(mono_ms() >= a);
        assert_ne!(unique(), unique());
        assert!(unique()
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
    }

    #[test]
    fn the_stopped_clock_reads_zero_and_still_gives_unique_names() {
        assert_eq!((Stopped.wall_ms(), Stopped.mono_ms()), (0, 0));
        assert_ne!(Stopped.unique(), Stopped.unique());
    }
}
