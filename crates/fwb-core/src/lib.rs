//! Film Wizard Budgeting engine.
//!
//! Clean-room: written from how film budgets work and from observed behaviour of
//! budgeting tools, never from proprietary code or binaries. See CLEAN-ROOM.md.

/// Money in integer cents, so totals never drift from float rounding.
pub type Cents = i64;

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_builds() {}
}
