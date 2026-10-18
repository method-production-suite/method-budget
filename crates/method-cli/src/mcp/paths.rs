//! Where the server may read and write: inside its roots only.
//!
//! - A relative path is taken from the first root; an absolute one must be
//!   inside some root.
//! - An existing file is resolved with every symlink followed
//!   (`canonicalize`), and the real path must still be inside a root, so a
//!   link that leads out is refused.
//! - A file to write must be in an existing folder inside a root, must not
//!   be a symlink or a folder, and must have one of the extensions the tool
//!   writes (`.mthd` for budgets, `.xlsx`, `.csv`... for exports),
//!   so the server can't be pointed at a script, a config file or a git hook.
//! - Nothing below a root whose name starts with `.` (`.git`, `.ssh`, a
//!   hidden file) is read or written.

use std::path::{Component, Path, PathBuf};

use super::limits;
use super::tools::ToolError;

/// Budget file extensions.
pub const BUDGET_EXTS: &[&str] = method_format::EXTENSIONS;
/// Spreadsheets `import_sheet` reads.
pub const SHEET_EXTS: &[&str] = &["xlsx", "csv", "tsv", "txt"];
/// Budget exports `import_budget_export` reads.
pub const EXPORT_EXTS: &[&str] = &["json", "xml", "xlsx", "txt", "tsv", "tab"];
/// Schedule days files `schedule_days` reads.
pub const SCHEDULE_EXTS: &[&str] = &["json"];

/// The folders the server works in, each canonical (symlinks resolved).
#[derive(Clone, Debug)]
pub struct Roots {
    dirs: Vec<PathBuf>,
}

impl Roots {
    /// Canonicalizes `dirs`; each must be an existing folder.
    pub fn new(dirs: &[PathBuf]) -> Result<Roots, String> {
        if dirs.is_empty() {
            return Err("at least one root folder is needed".into());
        }
        let mut out: Vec<PathBuf> = Vec::new();
        for d in dirs {
            let real = std::fs::canonicalize(d).map_err(|e| format!("{}: {e}", d.display()))?;
            if !real.is_dir() {
                return Err(format!("{}: not a folder", d.display()));
            }
            if !out.contains(&real) {
                out.push(real);
            }
        }
        Ok(Roots { dirs: out })
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// `typed` joined to the first root when relative, after basic checks.
    fn join(&self, typed: &str) -> Result<PathBuf, ToolError> {
        if typed.trim().is_empty() {
            return Err(ToolError::path("the path is empty"));
        }
        if typed.len() > limits::MAX_PATH || typed.contains('\0') {
            return Err(ToolError::path("that is not a usable path"));
        }
        let p = Path::new(typed);
        Ok(if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.dirs[0].join(p)
        })
    }

    /// The root `real` (canonical) is inside, refusing hidden names below it.
    fn inside(&self, real: &Path, typed: &str) -> Result<&Path, ToolError> {
        let root = self
            .dirs
            .iter()
            .filter(|r| real.starts_with(r))
            .max_by_key(|r| r.as_os_str().len())
            .ok_or_else(|| {
                ToolError::path(format!(
                    "{typed}: outside the folders this server may use ({})",
                    self.list()
                ))
            })?;
        let rest = real.strip_prefix(root).unwrap_or(real);
        for c in rest.components() {
            match c {
                Component::Normal(n) if n.to_string_lossy().starts_with('.') => {
                    return Err(ToolError::path(format!(
                        "{typed}: hidden files and folders (names starting with `.`) are off limits"
                    )))
                }
                Component::Normal(_) => {}
                _ => return Err(ToolError::path(format!("{typed}: not a plain path"))),
            }
        }
        Ok(root)
    }

    fn list(&self) -> String {
        self.dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// An existing file with one of `exts`, as its real path.
    pub fn existing(&self, typed: &str, exts: &[&str]) -> Result<PathBuf, ToolError> {
        let joined = self.join(typed)?;
        let real = std::fs::canonicalize(&joined).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::not_found(format!("{typed}: no such file"))
            } else {
                ToolError::io(format!("{typed}: {e}"))
            }
        })?;
        self.inside(&real, typed)?;
        if !real.is_file() {
            return Err(ToolError::path(format!("{typed}: not a file")));
        }
        check_ext(&real, exts, typed)?;
        Ok(real)
    }

    /// A file to write, with one of `exts`: its folder must exist inside a
    /// root; the file itself may exist (a plain file, not a symlink).
    pub fn target(&self, typed: &str, exts: &[&str]) -> Result<PathBuf, ToolError> {
        let joined = self.join(typed)?;
        let name = joined
            .file_name()
            .ok_or_else(|| ToolError::path(format!("{typed}: needs a file name")))?
            .to_os_string();
        let parent = joined
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent_real = std::fs::canonicalize(parent).map_err(|e| {
            ToolError::not_found(format!("{typed}: the folder doesn't exist ({e})"))
        })?;
        if !parent_real.is_dir() {
            return Err(ToolError::path(format!("{typed}: not in a folder")));
        }
        let real = parent_real.join(&name);
        self.inside(&real, typed)?;
        check_ext(&real, exts, typed)?;
        match std::fs::symlink_metadata(&real) {
            Ok(m) if m.file_type().is_symlink() => Err(ToolError::path(format!(
                "{typed}: is a symbolic link; refusing to write through it"
            ))),
            Ok(m) if !m.is_file() => Err(ToolError::path(format!("{typed}: not a file"))),
            _ => Ok(real),
        }
    }

    /// A folder inside a root (the first root when `typed` is `None`).
    pub fn folder(&self, typed: Option<&str>) -> Result<PathBuf, ToolError> {
        let Some(typed) = typed else {
            return Ok(self.dirs[0].clone());
        };
        let joined = self.join(typed)?;
        let real = std::fs::canonicalize(&joined)
            .map_err(|e| ToolError::not_found(format!("{typed}: {e}")))?;
        self.inside(&real, typed)?;
        if !real.is_dir() {
            return Err(ToolError::path(format!("{typed}: not a folder")));
        }
        Ok(real)
    }

    /// How a real path is shown back: relative to the first root when inside
    /// it (so it can be passed back as is), else absolute.
    pub fn show(&self, real: &Path) -> String {
        match real.strip_prefix(&self.dirs[0]) {
            Ok(rest) if !rest.as_os_str().is_empty() => rest.display().to_string(),
            _ => real.display().to_string(),
        }
    }
}

fn check_ext(real: &Path, exts: &[&str], typed: &str) -> Result<(), ToolError> {
    let ok = real
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)));
    if ok {
        Ok(())
    } else {
        let list: Vec<String> = exts.iter().map(|e| format!(".{e}")).collect();
        Err(ToolError::path(format!(
            "{typed}: this tool only works with {} files",
            list.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn temp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("method-mcp-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn paths_stay_inside_the_roots() {
        let base = temp("inside");
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("a.mthd"), "x").unwrap();
        std::fs::write(root.join("sub/b.mthd"), "x").unwrap();
        std::fs::write(root.join(".git/c.mthd"), "x").unwrap();
        std::fs::write(root.join("notes.txt"), "x").unwrap();
        std::fs::write(outside.join("o.mthd"), "x").unwrap();
        let roots = Roots::new(std::slice::from_ref(&root)).unwrap();
        let b = BUDGET_EXTS;

        assert!(roots.existing("a.mthd", b).unwrap().ends_with("a.mthd"));
        assert!(roots.existing("sub/b.mthd", b).is_ok());
        let abs = root.join("a.mthd");
        assert!(roots.existing(abs.to_str().unwrap(), b).is_ok());
        assert_eq!(
            roots.show(&roots.existing("sub/b.mthd", b).unwrap()),
            Path::new("sub").join("b.mthd").display().to_string()
        );

        let refused = |r: Result<PathBuf, ToolError>, want: &str| {
            let e = r.unwrap_err();
            assert!(e.message.contains(want), "{}", e.message);
        };
        refused(
            roots.existing("../outside/o.mthd", b),
            "outside the folders",
        );
        refused(
            roots.existing(outside.join("o.mthd").to_str().unwrap(), b),
            "outside",
        );
        refused(roots.existing(".git/c.mthd", b), "hidden");
        refused(
            roots.existing("notes.txt", b),
            "only works with .mthd files",
        );
        refused(roots.existing("missing.mthd", b), "no such file");
        refused(roots.existing("", b), "empty");
        refused(roots.existing("a\0.mthd", b), "not a usable path");
        refused(roots.target("../outside/new.mthd", b), "outside");
        refused(roots.target("nofolder/new.mthd", b), "folder doesn't exist");
        refused(roots.target("evil.sh", b), "only works with");
        refused(roots.target(".hidden.mthd", b), "hidden");
        refused(roots.target("sub", b), "only works with");
        assert!(roots
            .target("sub/new.mthd", b)
            .unwrap()
            .ends_with("sub/new.mthd"));
        assert!(roots.folder(Some("sub")).is_ok());
        refused(roots.folder(Some("../outside")), "outside");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_that_leave_are_refused() {
        use std::os::unix::fs::symlink;
        let base = temp("links");
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("o.mthd"), "x").unwrap();
        std::fs::write(root.join("in.mthd"), "x").unwrap();
        symlink(outside.join("o.mthd"), root.join("link.mthd")).unwrap();
        symlink(&outside, root.join("dirlink")).unwrap();
        symlink(root.join("in.mthd"), root.join("inner.mthd")).unwrap();
        symlink(base.join("nowhere.mthd"), root.join("dangling.mthd")).unwrap();
        let roots = Roots::new(std::slice::from_ref(&root)).unwrap();
        let b = BUDGET_EXTS;
        for p in ["link.mthd", "dirlink/o.mthd"] {
            let e = roots.existing(p, b).unwrap_err();
            assert!(e.message.contains("outside"), "{p}: {}", e.message);
        }
        assert!(roots
            .target("dirlink/new.mthd", b)
            .unwrap_err()
            .message
            .contains("outside"));
        // A link that stays inside resolves to the real file, which is fine to
        // read, but nothing is ever written through a link.
        assert!(roots
            .existing("inner.mthd", b)
            .unwrap()
            .ends_with("in.mthd"));
        for p in ["inner.mthd", "dangling.mthd", "link.mthd"] {
            let e = roots.target(p, b).unwrap_err();
            assert!(e.message.contains("symbolic link"), "{p}: {}", e.message);
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
