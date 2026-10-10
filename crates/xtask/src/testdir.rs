//! Throwaway repositories for the tests: a fresh folder under the system's
//! temporary directory, optionally a git repository, removed when dropped.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// An empty folder, made a git repository with an author set.
    pub fn new(name: &str) -> TempRepo {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "method-xtask-{name}-{}-{n}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let r = TempRepo { root };
        r.git(&["init", "--quiet", "--initial-branch=main"]);
        r.git(&["config", "user.name", "Test"]);
        r.git(&["config", "user.email", "test@example.com"]);
        r.git(&["config", "commit.gpgsign", "false"]);
        r.git(&["config", "tag.gpgsign", "false"]);
        r
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write(&self, rel: &str, content: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap()
    }

    pub fn read_dir(&self, rel: &str) -> Vec<String> {
        std::fs::read_dir(self.root.join(rel))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    pub fn git(&self, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Commits everything there is, with `message` (or "work").
    pub fn commit_with(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
    }

    pub fn commit(&self) {
        self.commit_with("work");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
