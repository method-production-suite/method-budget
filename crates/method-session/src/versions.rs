//! Named versions saved inside a budget: the Versions dialog's commands
//! (decisions.md, entries 17 and 27). The engine (`method_core::versions`) does
//! the work; this checks what the page sends and makes each change one undo
//! step, so Undo takes back a save, a rename, a delete or a whole restore.
//!
//! The page lists the versions from the view (`budget.versions`: id, name,
//! created, note); their content never crosses the bridge.
//!
//! Names are one line of at most [`NAME_MAX_CHARS`] characters, trimmed and
//! not blank; notes are at most [`MAX_NOTE_CHARS`] characters, blank meaning
//! none. `created` is the page's local time as an ISO 8601 date-time (the
//! engine never reads the clock), checked for shape only.

use method_core::{Budget, VersionId, MAX_NOTE_CHARS};

use crate::edit::EditResult;
use crate::session::Session;

/// Longest version name, in characters.
pub const NAME_MAX_CHARS: usize = 200;

/// A version name: trimmed, one line, not blank, not too long.
pub fn check_name(name: &str) -> EditResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a version needs a name".into());
    }
    if name.chars().any(char::is_control) {
        return Err("a version name is one line, with no tabs".into());
    }
    let n = name.chars().count();
    if n > NAME_MAX_CHARS {
        return Err(format!(
            "that name is {n} characters; version names can be at most {NAME_MAX_CHARS}"
        ));
    }
    Ok(name.to_string())
}

/// A version note: `None` when blank; kept as typed otherwise.
pub fn check_note(note: Option<&str>) -> EditResult<Option<String>> {
    let Some(note) = note.filter(|n| !n.trim().is_empty()) else {
        return Ok(None);
    };
    let n = note.chars().count();
    if n > MAX_NOTE_CHARS {
        return Err(format!(
            "that note is {n} characters; notes can be at most {MAX_NOTE_CHARS}"
        ));
    }
    Ok(Some(note.to_string()))
}

/// Checks that `s` looks like an ISO 8601 date-time as the page writes it:
/// `YYYY-MM-DDTHH:MM`, optional `:SS` and fraction, then `Z` or `+HH:MM` /
/// `-HH:MM` (or nothing, for a local time without an offset).
pub fn check_created(s: &str) -> EditResult<String> {
    fn digits(b: &[u8]) -> bool {
        !b.is_empty() && b.iter().all(u8::is_ascii_digit)
    }
    fn number(b: &[u8]) -> u32 {
        b.iter().fold(0, |n, d| n * 10 + u32::from(d - b'0'))
    }
    let bad = || format!("`{s}` isn't a date and time like 2026-10-09T14:30:00-07:00");
    let b = s.as_bytes();
    if b.len() < 16 || b.len() > 40 {
        return Err(bad());
    }
    let shape = digits(&b[0..4])
        && b[4] == b'-'
        && digits(&b[5..7])
        && b[7] == b'-'
        && digits(&b[8..10])
        && b[10] == b'T'
        && digits(&b[11..13])
        && b[13] == b':'
        && digits(&b[14..16]);
    if !shape {
        return Err(bad());
    }
    let (month, day, hour, minute) = (
        number(&b[5..7]),
        number(&b[8..10]),
        number(&b[11..13]),
        number(&b[14..16]),
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return Err(bad());
    }
    let mut rest = &b[16..];
    if rest.first() == Some(&b':') {
        if rest.len() < 3 || !digits(&rest[1..3]) || number(&rest[1..3]) > 60 {
            return Err(bad());
        }
        rest = &rest[3..];
        if rest.first() == Some(&b'.') {
            let n = rest[1..].iter().take_while(|c| c.is_ascii_digit()).count();
            if n == 0 || n > 9 {
                return Err(bad());
            }
            rest = &rest[1 + n..];
        }
    }
    let zone_ok = match rest {
        [] | [b'Z'] => true,
        [sign, h1, h2, b':', m1, m2] if *sign == b'+' || *sign == b'-' => {
            digits(&[*h1, *h2, *m1, *m2]) && number(&[*h1, *h2]) <= 23 && number(&[*m1, *m2]) <= 59
        }
        _ => false,
    };
    if !zone_ok {
        return Err(bad());
    }
    Ok(s.to_string())
}

/// Saves the budget's current content as a new version. Returns its id.
pub fn save(
    b: &mut Budget,
    name: &str,
    note: Option<&str>,
    created: &str,
) -> EditResult<VersionId> {
    let name = check_name(name)?;
    let note = check_note(note)?;
    let created = check_created(created)?;
    Ok(method_core::save_version(b, name, created, note))
}

/// Puts version `id`'s content in place of the budget's, after saving what
/// it replaces as an automatic version (`Before restoring "<name>"`).
/// Returns the automatic version's id.
pub fn restore(b: &mut Budget, id: VersionId, created: &str) -> EditResult<VersionId> {
    let created = check_created(created)?;
    method_core::restore_version(b, id, created).map_err(|e| e.to_string())
}

pub fn rename(b: &mut Budget, id: VersionId, name: &str) -> EditResult {
    let name = check_name(name)?;
    method_core::rename_version(b, id, name).map_err(|e| e.to_string())
}

pub fn delete(b: &mut Budget, id: VersionId) -> EditResult {
    method_core::delete_version(b, id)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
// ---------------------------------------------------------------------------
// Commands (the desktop's src/versions.rs and the host's src/host/versions.rs
// call these)

/// Saves the budget as a new version, `created` being the page's local time.
pub fn save_version(
    s: &mut Session,
    name: &str,
    note: Option<&str>,
    created: &str,
) -> EditResult<VersionId> {
    let label = crate::messages::msg("engine.undo.save_version", &[("name", &name.trim())]);
    s.edit(label, |b| save(b, name, note, created))
}

/// Restores version `id` (one undo step; the replaced content is kept as an
/// automatic version too).
pub fn restore_version(s: &mut Session, id: VersionId, created: &str) -> EditResult<VersionId> {
    let label = match s.budget().version(id) {
        Some(v) => crate::messages::msg("engine.undo.restore_version", &[("name", &v.name)]),
        None => crate::messages::word("engine.undo.restore_a_version"),
    };
    s.edit(label, |b| restore(b, id, created))
}

pub fn rename_version(s: &mut Session, id: VersionId, name: &str) -> EditResult {
    let label = crate::messages::msg("engine.undo.rename_version", &[("name", &name.trim())]);
    s.edit(label, |b| rename(b, id, name))
}

pub fn delete_version(s: &mut Session, id: VersionId) -> EditResult {
    let label = match s.budget().version(id) {
        Some(v) => crate::messages::msg("engine.undo.delete_version", &[("name", &v.name)]),
        None => crate::messages::word("engine.undo.delete_a_version"),
    };
    s.edit(label, |b| delete(b, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    const AT: &str = "2026-10-09T14:30:00-07:00";

    fn first_rate(s: &Session) -> String {
        s.budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| !d.rate.is_blank())
            .map(|d| d.rate.0.clone())
            .unwrap()
    }

    fn first_line(s: &Session) -> method_core::RowId {
        s.budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| !d.rate.is_blank())
            .unwrap()
            .id
    }

    #[test]
    fn names_notes_and_times_are_checked() {
        assert_eq!(check_name("  Locked draft ").unwrap(), "Locked draft");
        assert!(check_name("   ").is_err());
        assert!(check_name("two\nlines").is_err());
        assert!(check_name(&"x".repeat(NAME_MAX_CHARS)).is_ok());
        assert!(check_name(&"x".repeat(NAME_MAX_CHARS + 1)).is_err());
        assert_eq!(check_note(Some("  ")).unwrap(), None);
        assert_eq!(check_note(None).unwrap(), None);
        assert_eq!(
            check_note(Some(" Studio notes\nround 2 "))
                .unwrap()
                .as_deref(),
            Some(" Studio notes\nround 2 ")
        );
        assert!(check_note(Some(&"é".repeat(MAX_NOTE_CHARS))).is_ok());
        assert!(check_note(Some(&"é".repeat(MAX_NOTE_CHARS + 1))).is_err());
        for ok in [
            AT,
            "2026-10-09T14:30",
            "2026-10-09T14:30:59Z",
            "2026-10-09T14:30:59.123+05:30",
            "2026-01-31T00:00:00",
        ] {
            assert!(check_created(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "yesterday",
            "2026-10-09",
            "2026-13-09T14:30",
            "2026-10-09 14:30",
            "2026-10-09T24:00",
            "2026-10-09T14:30:00+5",
            "2026-10-09T14:30:00.Z",
            "2026-10-09T14:30:00<script>",
        ] {
            assert!(check_created(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn save_rename_delete_are_each_one_undo_step() {
        let mut s = Session::new();
        let id = s
            .edit("Test edit", |b| {
                save(b, " First pass ", Some("for the studio"), AT)
            })
            .unwrap();
        let v = s.budget().version(id).unwrap();
        assert_eq!(
            (v.name.as_str(), v.created.as_str(), v.note.as_deref()),
            ("First pass", AT, Some("for the studio"))
        );
        assert!(s.edit("Test edit", |b| save(b, "", None, AT)).is_err());
        assert!(s.edit("Test edit", |b| save(b, "x", None, "now")).is_err());

        s.edit("Test edit", |b| rename(b, id, "Studio draft"))
            .unwrap();
        assert_eq!(s.budget().version(id).unwrap().name, "Studio draft");
        assert!(s.edit("Test edit", |b| rename(b, id + 7, "x")).is_err());
        s.undo();
        assert_eq!(s.budget().version(id).unwrap().name, "First pass");

        s.edit("Test edit", |b| delete(b, id)).unwrap();
        assert!(s.budget().versions.is_empty());
        assert!(s.edit("Test edit", |b| delete(b, id)).is_err());
        s.undo();
        assert_eq!(s.budget().versions.len(), 1);
        s.undo();
        assert!(s.budget().versions.is_empty(), "undo takes back the save");
        assert!(!s.is_dirty());
    }

    #[test]
    fn restore_is_reversible_and_one_undo_step() {
        let mut s = Session::new();
        let row = first_line(&s);
        let before = first_rate(&s);
        let v1 = s
            .edit("Test edit", |b| save(b, "Approved", None, AT))
            .unwrap();
        s.set_detail_field(row, "rate", "123").unwrap();
        let edited = s.budget().clone();

        let auto = s.edit("Test edit", |b| restore(b, v1, AT)).unwrap();
        assert_eq!(first_rate(&s), before, "the version's content is back");
        let names: Vec<&str> = s
            .budget()
            .versions
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert_eq!(names, ["Approved", "Before restoring \"Approved\""]);
        // The catalog has the same words, so the app can translate it.
        assert_eq!(
            names[1],
            crate::messages::msg("engine.name.before_restoring", &[("name", &"Approved")])
        );
        // The automatic version holds exactly what was replaced.
        assert_eq!(
            s.budget().version(auto).unwrap().budget.to_budget(),
            edited.snapshot().into_budget(Vec::new())
        );

        // One undo step puts the edited content and the version list back.
        s.undo();
        assert_eq!(*s.budget(), edited);
        s.redo();
        assert_eq!(first_rate(&s), before);

        // Restoring the automatic version also brings the edit back.
        s.edit("Test edit", |b| restore(b, auto, AT)).unwrap();
        assert_eq!(first_rate(&s), "123");
        assert_eq!(s.budget().versions.len(), 3);

        assert!(s.edit("Test edit", |b| restore(b, 999, AT)).is_err());
        assert!(s.edit("Test edit", |b| restore(b, v1, "soon")).is_err());
    }

    #[test]
    fn copying_a_budget_shares_its_versions() {
        // Every edit works on a copy of the budget; versions are whole copies
        // of it, so they must not be copied again with every keystroke.
        let mut s = Session::new();
        for i in 0..3 {
            s.edit("Test edit", |b| save(b, &format!("v{i}"), None, AT))
                .unwrap();
        }
        let copy = s.budget().clone();
        for (a, b) in s.budget().versions.iter().zip(&copy.versions) {
            assert!(std::sync::Arc::ptr_eq(&a.budget, &b.budget));
        }
        // An edit elsewhere keeps the very same version content.
        let row = first_line(&s);
        let held = std::sync::Arc::clone(&s.budget().versions[0].budget);
        s.set_detail_field(row, "rate", "5").unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &held,
            &s.budget().versions[0].budget
        ));
    }
}
