//! The release workflow never runs cargo beside a key (SUITE-STANDARDS.md section 3, "Made
//! strict 2026-10-19"; Method Launcher's test is the model): in each job, no step after the
//! first one that reads a secret may run `cargo`, since a stale fingerprint would rebuild and
//! run build scripts beside the key. Those steps run binaries built earlier instead (the
//! Tauri CLI's own `cargo-tauri bundle`, `target/debug/xtask`).
//!
//! A unit test, not one in `tests/`: an integration test makes `cargo test` rebuild the
//! xtask binary, which Windows can't replace while it runs.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The jobs of a workflow, each as its steps' text, from the YAML's indentation (jobs
    /// at two spaces, steps at six).
    fn jobs(workflow: &str) -> Vec<(String, Vec<String>)> {
        let mut jobs: Vec<(String, Vec<String>)> = Vec::new();
        let mut in_jobs = false;
        for line in workflow.lines() {
            if line == "jobs:" {
                in_jobs = true;
                continue;
            }
            if !in_jobs {
                continue;
            }
            let job = line
                .strip_prefix("  ")
                .and_then(|l| l.strip_suffix(':'))
                .filter(|n| !n.starts_with(' ') && !n.is_empty());
            if let Some(name) = job {
                jobs.push((name.to_string(), Vec::new()));
            } else if line.starts_with("      - ") {
                if let Some((_, steps)) = jobs.last_mut() {
                    steps.push(format!("{line}\n"));
                }
            } else if line.starts_with("        ") {
                if let Some(step) = jobs.last_mut().and_then(|(_, steps)| steps.last_mut()) {
                    step.push_str(line);
                    step.push('\n');
                }
            }
        }
        jobs
    }

    /// Whether a step runs cargo, ignoring comments (`cargo-tauri` is another program).
    fn runs_cargo(step: &str) -> bool {
        step.lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .any(|l| {
                l.split(|c: char| !c.is_alphanumeric() && c != '-')
                    .any(|w| w == "cargo")
            })
    }

    fn violations(workflow: &str) -> Vec<String> {
        let mut found = Vec::new();
        for (job, steps) in jobs(workflow) {
            let Some(first) = steps.iter().position(|s| s.contains("secrets.")) else {
                continue;
            };
            for step in steps.iter().skip(first + 1).filter(|s| runs_cargo(s)) {
                found.push(format!(
                    "{job}: {}",
                    step.lines().next().unwrap_or_default().trim()
                ));
            }
        }
        found
    }

    #[test]
    fn no_cargo_after_a_secret() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/release.yml");
        let workflow = std::fs::read_to_string(&path).expect("release.yml");
        let jobs = jobs(&workflow);
        assert!(
            jobs.iter()
                .any(|(name, steps)| name == "build" && steps.len() > 10),
            "the build job's steps were read"
        );
        assert_eq!(violations(&workflow), Vec::<String>::new());
    }

    #[test]
    fn the_check_catches_cargo_after_a_secret() {
        let bad = "jobs:\n  build:\n    steps:\n      - name: Build\n        run: cargo build\n      - name: Key\n        env:\n          K: ${{ secrets.KEY }}\n        run: echo\n      - name: Package\n        run: |\n          # cargo is fine in a comment\n          cargo tauri bundle\n";
        assert_eq!(violations(bad), vec!["build: - name: Package".to_string()]);
        let good = bad.replace("cargo tauri bundle", "cargo-tauri bundle");
        assert!(violations(&good).is_empty());
    }
}
