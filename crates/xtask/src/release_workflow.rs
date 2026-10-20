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

    /// The full crate-age check, security exceptions only, over both locks, in a job every
    /// build job needs (SUITE-STANDARDS.md section 6): no release builds, signs or ships a
    /// crate younger than the cooldown unless it fixes an advisory.
    fn release_age_check(workflow: &str) -> Result<(), String> {
        let jobs = jobs(workflow);
        let check = jobs
            .iter()
            .find(|(name, _)| name == "check")
            .ok_or("no check job")?;
        let step = check
            .1
            .iter()
            .find(|s| s.contains("check-crate-ages.py"))
            .ok_or("the check job doesn't run check-crate-ages.py")?;
        let runs = step
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .find(|l| l.contains("check-crate-ages.py"))
            .unwrap_or_default();
        if !runs.contains("--release") {
            return Err("check-crate-ages.py runs without --release".into());
        }
        for lock in ["Cargo.lock", "fuzz/Cargo.lock"] {
            if !step.contains(lock) {
                return Err(format!("the age check leaves out {lock}"));
            }
        }
        // The build job's own lines: from "  build:" to the next job (two spaces, a name).
        let build_needs_check = workflow
            .lines()
            .skip_while(|l| *l != "  build:")
            .skip(1)
            .take_while(|l| !(l.starts_with("  ") && !l.starts_with("   ") && l.ends_with(':')))
            .any(|l| l.trim() == "needs: check" || l.trim() == "needs: [check]");
        if !build_needs_check {
            return Err("the build job doesn't need the check job".into());
        }
        Ok(())
    }

    #[test]
    fn the_release_checks_every_crates_age_before_building() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/release.yml");
        let workflow = std::fs::read_to_string(&path).expect("release.yml");
        assert_eq!(release_age_check(&workflow), Ok(()));
    }

    #[test]
    fn the_age_check_test_catches_a_weaker_release() {
        let good = "jobs:\n  check:\n    steps:\n      - name: Ages\n        run: |\n          for lock in Cargo.lock fuzz/Cargo.lock; do\n            python3 scripts/check-crate-ages.py --lock \"$lock\" --release\n          done\n\n  build:\n    needs: check\n    steps:\n      - name: Build\n        run: cargo build\n";
        assert_eq!(release_age_check(good), Ok(()));
        let no_release = good.replace(" --release", "");
        assert!(release_age_check(&no_release)
            .unwrap_err()
            .contains("--release"));
        let one_lock = good.replace(" fuzz/Cargo.lock", "");
        assert!(release_age_check(&one_lock)
            .unwrap_err()
            .contains("fuzz/Cargo.lock"));
        let no_needs = good.replace("    needs: check\n", "");
        assert!(release_age_check(&no_needs).unwrap_err().contains("need"));
        let no_step = good.replace("check-crate-ages.py", "something-else.py");
        assert!(release_age_check(&no_step).is_err());
    }

    #[test]
    fn the_check_catches_cargo_after_a_secret() {
        let bad = "jobs:\n  build:\n    steps:\n      - name: Build\n        run: cargo build\n      - name: Key\n        env:\n          K: ${{ secrets.KEY }}\n        run: echo\n      - name: Package\n        run: |\n          # cargo is fine in a comment\n          cargo tauri bundle\n";
        assert_eq!(violations(bad), vec!["build: - name: Package".to_string()]);
        let good = bad.replace("cargo tauri bundle", "cargo-tauri bundle");
        assert!(violations(&good).is_empty());
    }
}
