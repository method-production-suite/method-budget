//! The budget files the server has open: what it last read or wrote of each
//! (to notice when someone else saved the file since), the budget as it was
//! when the server opened it (for "what did I change"), and the undo steps
//! of its own changes.
//!
//! Every change is made on a copy, checked (the budget must still add up if
//! it did before), written atomically next to the file and renamed into
//! place, keeping the file's permissions (`cli::write_atomic`). A change is
//! refused, with nothing written, when the file on disk is no longer what
//! the server last read or wrote: the caller re-opens it (`budget_open`),
//! looks at what changed and decides again.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fwb_core::{recalc, Budget, RecalcOptions, Report};

use super::limits;
use super::tools::ToolError;

/// One undo step: the file's bytes before a change, and what the change was.
#[derive(Clone, Debug)]
pub struct Step {
    pub label: String,
    pub bytes: Vec<u8>,
}

/// An open budget file.
#[derive(Clone, Debug)]
pub struct Doc {
    /// The bytes the server last read from or wrote to the file.
    pub base: Vec<u8>,
    /// `base`, parsed.
    pub budget: Budget,
    /// The bytes as the server first opened the file (or last re-opened it
    /// after someone else changed it).
    pub opened: Vec<u8>,
    /// Undo steps, oldest first.
    pub history: Vec<Step>,
    used: u64,
}

/// The budget a read sees: the server's copy, or what is on disk now when
/// someone else changed the file since.
pub struct View<'a> {
    pub budget: Cow<'a, Budget>,
    /// The file on disk is not what the server last read or wrote.
    pub changed_on_disk: bool,
    pub revision: String,
}

/// What a change did.
#[derive(Debug)]
pub struct Edited<T> {
    /// Whether anything changed (nothing is written when not).
    pub changed: bool,
    /// What the change function returned.
    pub out: T,
    pub before: Option<Report>,
    pub after: Result<Report, String>,
    pub revision: String,
}

/// What re-opening a file did.
pub struct Opened {
    /// The file had changed since the server last read it.
    pub reloaded: bool,
    /// Undo steps dropped because of that.
    pub dropped_undo: usize,
}

/// A short, stable fingerprint of a file's bytes (64-bit FNV-1a), shown to
/// clients as the file's revision. Conflicts are decided on the bytes
/// themselves, never on this.
pub fn revision(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Reads a file whole, refusing one over [`limits::MAX_FILE_BYTES`].
pub fn read_file(path: &Path, shown: &str) -> Result<Vec<u8>, ToolError> {
    let len = std::fs::metadata(path)
        .map_err(|e| ToolError::io(format!("{shown}: {e}")))?
        .len();
    if len > limits::MAX_FILE_BYTES {
        return Err(ToolError::refused(format!(
            "{shown}: {len} bytes is more than this server reads ({} MB)",
            limits::MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    std::fs::read(path).map_err(|e| ToolError::io(format!("{shown}: {e}")))
}

/// Parses a budget file's bytes.
pub fn parse(bytes: &[u8], shown: &str) -> Result<Budget, ToolError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ToolError::invalid(format!("{shown}: not a text file (not UTF-8)")))?;
    fwb_format::from_str(text).map_err(|e| ToolError::invalid(format!("{shown}: {e}")))
}

/// Whether the application would open this budget read-only.
pub fn is_shared_copy(b: &Budget) -> bool {
    b.provenance.as_ref().is_some_and(|p| p.is_shared_copy())
}

/// The open files, by real path.
#[derive(Default)]
pub struct Docs {
    open: BTreeMap<PathBuf, Doc>,
    clock: u64,
}

impl Docs {
    pub fn new() -> Docs {
        Docs::default()
    }

    /// The open file at `path`, if any.
    pub fn get(&self, path: &Path) -> Option<&Doc> {
        self.open.get(path)
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Opens `path` with `bytes` as read now, closing the least recently
    /// used file past [`limits::MAX_OPEN_FILES`].
    fn insert(&mut self, path: &Path, bytes: Vec<u8>, shown: &str) -> Result<&mut Doc, ToolError> {
        let budget = parse(&bytes, shown)?;
        if self.open.len() >= limits::MAX_OPEN_FILES && !self.open.contains_key(path) {
            if let Some(oldest) = self
                .open
                .iter()
                .min_by_key(|(_, d)| d.used)
                .map(|(p, _)| p.clone())
            {
                self.open.remove(&oldest);
            }
        }
        let used = self.tick();
        self.open.insert(
            path.to_path_buf(),
            Doc {
                opened: bytes.clone(),
                base: bytes,
                budget,
                history: Vec::new(),
                used,
            },
        );
        Ok(self.open.get_mut(path).expect("just inserted"))
    }

    /// Opens (or re-opens) `path`: the server's copy becomes what is on disk.
    /// When the file changed since the server last read it, its undo steps
    /// are dropped (they would undo onto someone else's changes).
    pub fn open(&mut self, path: &Path, shown: &str) -> Result<(&Doc, Opened), ToolError> {
        let disk = read_file(path, shown)?;
        let used = self.tick();
        let state = match self.open.get_mut(path) {
            Some(doc) if doc.base == disk => {
                doc.used = used;
                Opened {
                    reloaded: false,
                    dropped_undo: 0,
                }
            }
            Some(doc) => {
                let budget = parse(&disk, shown)?;
                let dropped_undo = doc.history.len();
                *doc = Doc {
                    opened: disk.clone(),
                    base: disk,
                    budget,
                    history: Vec::new(),
                    used,
                };
                Opened {
                    reloaded: true,
                    dropped_undo,
                }
            }
            None => {
                self.insert(path, disk, shown)?;
                Opened {
                    reloaded: false,
                    dropped_undo: 0,
                }
            }
        };
        Ok((self.open.get(path).expect("open"), state))
    }

    /// The budget at `path` as it is now, for reading. A file not open yet is
    /// opened. When someone else changed the file since the server last read
    /// it, the read sees the new content (flagged) but the server's copy is
    /// left alone, so the next change is still refused until the file is
    /// re-opened.
    pub fn view(&mut self, path: &Path, shown: &str) -> Result<View<'_>, ToolError> {
        let disk = read_file(path, shown)?;
        let used = self.tick();
        if !self.open.contains_key(path) {
            self.insert(path, disk.clone(), shown)?;
        }
        let doc = self.open.get_mut(path).expect("open");
        doc.used = used;
        if doc.base == disk {
            return Ok(View {
                revision: revision(&doc.base),
                budget: Cow::Borrowed(&doc.budget),
                changed_on_disk: false,
            });
        }
        let budget = parse(&disk, shown)?;
        Ok(View {
            revision: revision(&disk),
            budget: Cow::Owned(budget),
            changed_on_disk: true,
        })
    }

    /// Changes the budget at `path` with `f` and writes it. Refused, with
    /// nothing written, when the file changed on disk since the server last
    /// read or wrote it, when it is a shared copy (the application opens
    /// those read-only), when `f` fails, or when the budget added up before
    /// and doesn't after.
    pub fn edit<T>(
        &mut self,
        path: &Path,
        shown: &str,
        label: &str,
        f: impl FnOnce(&mut Budget) -> Result<T, ToolError>,
    ) -> Result<Edited<T>, ToolError> {
        let disk = read_file(path, shown)?;
        let used = self.tick();
        if !self.open.contains_key(path) {
            self.insert(path, disk.clone(), shown)?;
        }
        let doc = self.open.get_mut(path).expect("open");
        doc.used = used;
        if doc.base != disk {
            return Err(conflict(shown));
        }
        if is_shared_copy(&doc.budget) {
            return Err(ToolError::refused(format!(
                "{shown} is a copy saved for sharing, which Method Budget opens read-only. \
                 Nothing was written. Use save_as with editable: true to make an editable copy."
            )));
        }
        let mut after = doc.budget.clone();
        let out = f(&mut after)?;
        if after == doc.budget {
            return Ok(Edited {
                changed: false,
                out,
                before: None,
                after: Err("unchanged".into()),
                revision: revision(&doc.base),
            });
        }
        let opts = RecalcOptions::default();
        let before = recalc(&doc.budget, &opts).ok();
        let r_after = recalc(&after, &opts).map_err(|e| e.to_string());
        if let (Some(_), Err(e)) = (&before, &r_after) {
            return Err(ToolError::refused(format!(
                "after this change the budget wouldn't add up ({e}). Nothing was written."
            )));
        }
        let bytes = fwb_format::to_string(&after).into_bytes();
        write(doc, path, shown, label, bytes, after)?;
        Ok(Edited {
            changed: true,
            out,
            before,
            after: r_after,
            revision: revision(&doc.base),
        })
    }

    /// Writes `budget` to `path` as a whole file (a new budget, a copy, an
    /// import or a merge result). An existing file is replaced only with
    /// `overwrite`, and then as an undoable change of that file, refused like
    /// [`Docs::edit`] when it changed on disk since the server read it.
    pub fn write_new(
        &mut self,
        path: &Path,
        shown: &str,
        label: &str,
        budget: Budget,
        overwrite: bool,
    ) -> Result<String, ToolError> {
        let bytes = fwb_format::to_string(&budget).into_bytes();
        if path.exists() {
            if !overwrite {
                return Err(ToolError::refused(format!(
                    "{shown} already exists; pass overwrite: true to replace it"
                )));
            }
            let disk = read_file(path, shown)?;
            let used = self.tick();
            if !self.open.contains_key(path) {
                // Not a budget, or not one we can read: refuse rather than
                // replace something we can't put back.
                self.insert(path, disk.clone(), shown).map_err(|e| {
                    ToolError::refused(format!(
                        "{shown} exists and isn't a budget this server can read, so it won't \
                         replace it ({})",
                        e.message
                    ))
                })?;
            }
            let doc = self.open.get_mut(path).expect("open");
            doc.used = used;
            if doc.base != disk {
                return Err(conflict(shown));
            }
            write(doc, path, shown, label, bytes, budget)?;
            return Ok(revision(&doc.base));
        }
        check_size(&bytes, shown)?;
        crate::cli::write_atomic(path, &bytes).map_err(|e| ToolError::io(e.message))?;
        let rev = revision(&bytes);
        self.open.remove(path);
        self.insert(path, bytes, shown)?;
        Ok(rev)
    }

    /// Takes back the last change this server made to `path`: writes the
    /// file as it was before it. Refused when the file changed on disk since.
    pub fn undo(
        &mut self,
        path: &Path,
        shown: &str,
    ) -> Result<(String, Budget, Budget), ToolError> {
        let disk = read_file(path, shown)?;
        let doc = self
            .open
            .get_mut(path)
            .ok_or_else(|| nothing_to_undo(shown))?;
        if doc.base != disk {
            return Err(conflict(shown));
        }
        let step = doc
            .history
            .last()
            .cloned()
            .ok_or_else(|| nothing_to_undo(shown))?;
        let restored = parse(&step.bytes, shown)?;
        check_size(&step.bytes, shown)?;
        crate::cli::write_atomic(path, &step.bytes).map_err(|e| ToolError::io(e.message))?;
        doc.history.pop();
        let was = std::mem::replace(&mut doc.budget, restored.clone());
        doc.base = step.bytes;
        Ok((step.label, was, restored))
    }
}

fn check_size(bytes: &[u8], shown: &str) -> Result<(), ToolError> {
    if bytes.len() as u64 > limits::MAX_FILE_BYTES {
        return Err(ToolError::refused(format!(
            "{shown}: the result would be more than {} MB; nothing was written",
            limits::MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// Writes `bytes` (the budget `after`) over the open file and records the
/// undo step.
fn write(
    doc: &mut Doc,
    path: &Path,
    shown: &str,
    label: &str,
    bytes: Vec<u8>,
    after: Budget,
) -> Result<(), ToolError> {
    check_size(&bytes, shown)?;
    crate::cli::write_atomic(path, &bytes).map_err(|e| ToolError::io(e.message))?;
    let old = std::mem::replace(&mut doc.base, bytes);
    doc.budget = after;
    doc.history.push(Step {
        label: label.to_string(),
        bytes: old,
    });
    let mut total: usize = doc.history.iter().map(|s| s.bytes.len()).sum();
    while doc.history.len() > limits::MAX_UNDO_STEPS
        || (total > limits::MAX_UNDO_BYTES && doc.history.len() > 1)
    {
        total -= doc.history.remove(0).bytes.len();
    }
    Ok(())
}

fn conflict(shown: &str) -> ToolError {
    ToolError::conflict(format!(
        "{shown} changed on disk since this server last read it (another program or person \
         saved it). Nothing was written. Call budget_open to load the file as it is now (diff \
         with since: \"disk\" first shows what changed), then make the change again."
    ))
}

fn nothing_to_undo(shown: &str) -> ToolError {
    ToolError::refused(format!(
        "nothing to undo in {shown}: only changes this server made since it opened the file \
         can be undone"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("method-mcp-docs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn edits_write_atomically_detect_conflicts_and_undo() {
        let dir = temp("edit");
        let p = dir.join("Film.mthd");
        crate::cli::save(p.to_str().unwrap(), &fwb_core::starter::budget()).unwrap();
        let mut docs = Docs::new();
        let original = std::fs::read(&p).unwrap();

        let e = docs
            .edit(&p, "Film.mthd", "Set title", |b| {
                b.title = "New".into();
                Ok(())
            })
            .unwrap();
        assert!(e.changed);
        assert_eq!(crate::cli::load(p.to_str().unwrap()).unwrap().title, "New");
        // Only the budget is left in the folder: the temporary file was renamed.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

        // No change: nothing written, no undo step.
        let e = docs.edit(&p, "Film.mthd", "Nothing", |_| Ok(())).unwrap();
        assert!(!e.changed);
        assert_eq!(docs.get(&p).unwrap().history.len(), 1);

        // A change that breaks the totals is refused.
        let e = docs
            .edit(&p, "Film.mthd", "Break", |b| {
                b.globals.insert("SHOOT_DAYS".into(), "NOPE * 2".into());
                Ok(())
            })
            .unwrap_err();
        assert!(e.message.contains("wouldn't add up"), "{}", e.message);
        assert_eq!(crate::cli::load(p.to_str().unwrap()).unwrap().title, "New");

        // Someone else saves the file: reads see it, changes are refused.
        let mut theirs = crate::cli::load(p.to_str().unwrap()).unwrap();
        theirs.title = "Theirs".into();
        crate::cli::save(p.to_str().unwrap(), &theirs).unwrap();
        let v = docs.view(&p, "Film.mthd").unwrap();
        assert!(v.changed_on_disk);
        assert_eq!(v.budget.title, "Theirs");
        let e = docs
            .edit(&p, "Film.mthd", "Mine", |b| {
                b.title = "Mine".into();
                Ok(())
            })
            .unwrap_err();
        assert_eq!(e.kind, "conflict");
        assert!(docs.undo(&p, "Film.mthd").unwrap_err().kind == "conflict");
        assert_eq!(
            crate::cli::load(p.to_str().unwrap()).unwrap().title,
            "Theirs"
        );

        // Re-opening takes the file as it is and drops the undo steps.
        let (_, o) = docs.open(&p, "Film.mthd").unwrap();
        assert!(o.reloaded);
        assert_eq!(o.dropped_undo, 1);
        docs.edit(&p, "Film.mthd", "Mine", |b| {
            b.title = "Mine".into();
            Ok(())
        })
        .unwrap();
        let (label, was, now) = docs.undo(&p, "Film.mthd").unwrap();
        assert_eq!(label, "Mine");
        assert_eq!((was.title.as_str(), now.title.as_str()), ("Mine", "Theirs"));
        assert!(docs
            .undo(&p, "Film.mthd")
            .unwrap_err()
            .message
            .contains("nothing to undo"));
        assert_ne!(std::fs::read(&p).unwrap(), original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shared_copies_are_read_only_and_new_files_are_not_replaced() {
        let dir = temp("shared");
        let p = dir.join("Shared.mthd");
        let b = fwb_core::share::share_copy(
            &fwb_core::starter::budget(),
            None,
            &fwb_core::share::ShareOptions::default(),
            "2026-10-10T12:00:00Z",
            "0",
            None,
        )
        .unwrap();
        let mut docs = Docs::new();
        docs.write_new(&p, "Shared.mthd", "New", b.clone(), false)
            .unwrap();
        let e = docs
            .edit(&p, "Shared.mthd", "x", |b| {
                b.title = "x".into();
                Ok(())
            })
            .unwrap_err();
        assert!(e.message.contains("read-only"), "{}", e.message);
        let e = docs
            .write_new(&p, "Shared.mthd", "New", b, false)
            .unwrap_err();
        assert!(e.message.contains("already exists"));
        // A file that isn't a budget is never replaced.
        let other = dir.join("notes.mthd");
        std::fs::write(&other, "not a budget").unwrap();
        let e = docs
            .write_new(&other, "notes.mthd", "x", fwb_core::starter::budget(), true)
            .unwrap_err();
        assert!(e.message.contains("isn't a budget"), "{}", e.message);
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "not a budget");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revisions_are_stable() {
        assert_eq!(revision(b""), "cbf29ce484222325");
        assert_ne!(revision(b"a"), revision(b"b"));
    }
}
