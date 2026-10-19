//! The sample budget (SUITE-STANDARDS.md, section 8; decisions.md, entry
//! 84; docs/APP.md, "The sample budget"): Field Station, the suite's
//! invented production (fixtures/field-station/), opened as an untitled
//! copy marked "Sample", never as the bundled file itself, so nothing a
//! person types can change what the next copy starts from.
//!
//! The host bundles the file's bytes and remembers whether the sample has
//! been shown; this side decides, with no window, whether the app's start
//! opens it, and makes the copy. There is no setting for it (decisions.md,
//! entry 86): it opens once at the first start, and Help > Open Sample
//! brings a copy back any time.

use crate::session::Session;

/// The sample's line about itself, shown as its tab's banner until the
/// first Save or until closed (the converted budget's banner, decisions.md,
/// entry 67).
pub const NOTICE: &str = "Field Station is an invented sample budget, open as an untitled copy: \
change anything, nothing is kept unless you save it. To start a budget of your own, choose New… \
in the File menu.";

/// What the app's start knows when it decides whether to open the sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// The sample has been shown before (at a start or from Help > Open
    /// Sample).
    pub seen: bool,
    /// Nothing was opened, reopened or recovered: only the untouched
    /// Untitled budget is in the window, and no question is waiting.
    pub nothing_open: bool,
    /// The person has budgets of their own: anything on the recent or
    /// pinned lists.
    pub own_budgets: bool,
}

/// Whether the app's start opens the sample: only the first time, with
/// nothing else to show, for someone who hasn't opened a budget of their
/// own.
pub fn at_launch(l: Launch) -> bool {
    !l.seen && l.nothing_open && !l.own_budgets
}

/// The start found someone already at work (budgets of their own, or
/// something opened or recovered), so the sample is not for them: marked
/// seen without showing it, so it doesn't open unasked at a later start
/// after they clear their recent list.
pub fn retire_at_launch(l: Launch) -> bool {
    !l.seen && (l.own_budgets || !l.nothing_open)
}

/// A fresh untitled copy of the sample `bytes` (a `.mthd` file): no file,
/// nothing to undo and nothing unsaved (closing it asks nothing, and a
/// budget opened next takes its tab as it would an untouched Untitled
/// one), its tab marked "Sample" and [`NOTICE`] as its banner until it is
/// first saved.
pub fn session(bytes: &[u8]) -> Result<Session, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "The sample budget isn't text.")?;
    let budget = method_format::from_str(text)
        .map_err(|e| format!("The sample budget can't be read: {e}"))?;
    let mut s = Session::with_budget(budget, None);
    s.set_sample(NOTICE.to_string());
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::DiskStamp;

    const FIELD_STATION: &[u8] =
        include_bytes!("../../../fixtures/field-station/field-station.mthd");

    fn launch() -> Launch {
        Launch {
            seen: false,
            nothing_open: true,
            own_budgets: false,
        }
    }

    #[test]
    fn the_start_opens_it_once_with_nothing_else_to_show() {
        assert!(at_launch(launch()));
        for l in [
            Launch {
                seen: true,
                ..launch()
            },
            Launch {
                nothing_open: false,
                ..launch()
            },
            Launch {
                own_budgets: true,
                ..launch()
            },
        ] {
            assert!(!at_launch(l), "{l:?}");
        }
    }

    #[test]
    fn someone_already_at_work_never_sees_it_later() {
        assert!(!retire_at_launch(launch()));
        for l in [
            Launch {
                own_budgets: true,
                ..launch()
            },
            Launch {
                nothing_open: false,
                ..launch()
            },
            Launch {
                own_budgets: true,
                nothing_open: false,
                ..launch()
            },
        ] {
            assert!(retire_at_launch(l), "{l:?}");
            assert!(!at_launch(l), "{l:?}");
        }
        // Already seen: nothing to mark.
        for l in [
            Launch {
                seen: true,
                ..launch()
            },
            Launch {
                own_budgets: true,
                seen: true,
                ..launch()
            },
        ] {
            assert!(!retire_at_launch(l), "{l:?}");
            assert!(!at_launch(l), "{l:?}");
        }
    }

    #[test]
    fn every_start_either_shows_it_marks_it_or_finds_it_seen() {
        // Once a start has run, the sample never opens unasked again: it was
        // shown (and marked seen), or retired, or seen already.
        for seen in [false, true] {
            for nothing_open in [false, true] {
                for own_budgets in [false, true] {
                    let l = Launch {
                        seen,
                        nothing_open,
                        own_budgets,
                    };
                    let outcomes = [at_launch(l), retire_at_launch(l), l.seen];
                    assert_eq!(outcomes.iter().filter(|o| **o).count(), 1, "{l:?}");
                }
            }
        }
    }

    #[test]
    fn a_copy_marked_sample_until_saved() {
        let mut s = session(FIELD_STATION).unwrap();
        assert!(s.is_sample());
        assert_eq!(s.path(), None);
        assert!(s.is_untouched() && !s.is_dirty());
        assert_eq!(s.tab_label(), "Sample: Field Station");
        assert_eq!(s.converted(), Some(NOTICE));
        assert_eq!(s.view().converted.as_deref(), Some(NOTICE));
        // Every copy starts from the same budget.
        let again = session(FIELD_STATION).unwrap();
        assert_eq!(again.budget(), s.budget());
        // Saved as a file, it is the person's own.
        let path = std::path::Path::new("/budgets/Field Station.mthd");
        s.mark_saved(
            path,
            DiskStamp {
                len: 0,
                modified: None,
                hash: 0,
            },
        );
        assert!(!s.is_sample());
        assert_eq!(s.converted(), None);
        assert_eq!(s.tab_label(), "Field Station");
    }

    #[test]
    fn the_notice_says_how_to_start_your_own_in_plain_words() {
        assert!(NOTICE.contains("sample") && NOTICE.contains("New…"));
        assert!(!NOTICE.contains('\u{2014}') && !NOTICE.contains('!'));
    }

    #[test]
    fn what_isnt_a_budget_is_refused() {
        assert!(session(b"not a budget").is_err());
        assert!(session(&[0xff, 0xfe]).is_err());
    }
}
