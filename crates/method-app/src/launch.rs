//! The window binary's command line (SUITE-STANDARDS.md, section 2;
//! decisions.md, entry 85), as Method Launcher and the systems call it:
//!
//! - a file to open (Windows and Linux pass a double-clicked file this way);
//! - `--check-for-updates`: check now, as Help > Check for Updates…
//!   (src/updater.rs, started when the page first asks where an update is,
//!   so it hears the answer);
//! - `--open-sample`: open the Field Station sample, as Help > Open Sample
//!   (src/sample.rs, in the place of the start window).
//!
//! Both flags are read at the start only: the app has no single-instance
//! hand-off, so a second launch is a second copy of the app with its own
//! command line. Any other option (`-psn_…` from older macOS, a `--` option
//! this build doesn't know) is skipped, never taken for a file.

use std::ffi::OsString;
use std::path::PathBuf;

pub const CHECK_FOR_UPDATES: &str = "--check-for-updates";
pub const OPEN_SAMPLE: &str = "--open-sample";

/// What the command line asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchArgs {
    pub check_for_updates: bool,
    pub open_sample: bool,
    /// The arguments that aren't options, in order: candidate files.
    pub files: Vec<PathBuf>,
}

impl LaunchArgs {
    /// Reads `args` (without the program's own name). Flags count anywhere;
    /// anything starting with `-` is an option, known or not.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> LaunchArgs {
        let mut out = LaunchArgs::default();
        for a in args {
            match a.to_str() {
                Some(CHECK_FOR_UPDATES) => out.check_for_updates = true,
                Some(OPEN_SAMPLE) => out.open_sample = true,
                _ if a.to_string_lossy().starts_with('-') => {}
                _ => out.files.push(PathBuf::from(a)),
            }
        }
        out
    }

    /// This process's command line.
    pub fn from_env() -> LaunchArgs {
        LaunchArgs::parse(std::env::args_os().skip(1))
    }

    /// The first candidate that names an existing file.
    pub fn file(&self) -> Option<PathBuf> {
        self.files.iter().find(|p| p.is_file()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: &[&str]) -> LaunchArgs {
        LaunchArgs::parse(v.iter().map(OsString::from))
    }

    #[test]
    fn flags_count_anywhere_on_the_line() {
        assert_eq!(parse(&[]), LaunchArgs::default());
        let a = parse(&["--open-sample"]);
        assert!(a.open_sample && !a.check_for_updates && a.files.is_empty());
        let a = parse(&["--check-for-updates"]);
        assert!(a.check_for_updates && !a.open_sample && a.files.is_empty());
        let a = parse(&["/films/a.mthd", "--check-for-updates", "--open-sample"]);
        assert!(a.check_for_updates && a.open_sample);
        assert_eq!(a.files, [PathBuf::from("/films/a.mthd")]);
        let a = parse(&["--open-sample", "/films/a.mthd", "--open-sample"]);
        assert!(a.open_sample);
        assert_eq!(a.files, [PathBuf::from("/films/a.mthd")]);
    }

    #[test]
    fn other_options_are_skipped_not_opened() {
        let a = parse(&[
            "-psn_0_123",
            "--verbose",
            "--open-sample=yes",
            "--OPEN-SAMPLE",
            "-",
            "/films/a.mthd",
        ]);
        assert!(
            !a.open_sample && !a.check_for_updates,
            "only the exact flags"
        );
        assert_eq!(a.files, [PathBuf::from("/films/a.mthd")]);
    }

    #[test]
    fn a_file_still_opens_beside_a_flag() {
        let dir = crate::safety::tests::temp_dir("launch-args");
        let f = dir.join("film.mthd");
        std::fs::write(&f, "{}").unwrap();
        let line = |v: &[&std::ffi::OsStr]| LaunchArgs::parse(v.iter().map(|s| s.to_os_string()));
        let a = line(&["--open-sample".as_ref(), f.as_os_str()]);
        assert!(a.open_sample);
        assert_eq!(a.file(), Some(f.clone()));
        let a = line(&[f.as_os_str(), "--check-for-updates".as_ref()]);
        assert!(a.check_for_updates);
        assert_eq!(a.file(), Some(f.clone()));
        // Missing files and folders are passed over for the next candidate.
        let a = line(&[
            dir.join("missing.mthd").as_os_str(),
            dir.as_os_str(),
            "--open-sample".as_ref(),
            f.as_os_str(),
        ]);
        assert_eq!(a.file(), Some(f.clone()));
        assert_eq!(line(&["--open-sample".as_ref()]).file(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
