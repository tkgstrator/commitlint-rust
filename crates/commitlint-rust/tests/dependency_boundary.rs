//! The message crate must not depend on guard, identity or configuration crates.
use std::{fs, path::Path};

#[test]
fn manifest_declares_no_guard_runtime_dependency() {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let runtime = manifest.split("[dev-dependencies]").next().unwrap();
    for forbidden in ["commitguard", "gh-commit-guard"] {
        assert!(!runtime.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn sources_have_no_guard_config_identity_gh_or_hook_references() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    check_sources(&src);
}

fn check_sources(directory: &Path) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            check_sources(&path);
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        // Strip the explanatory comments; production code is what matters.
        let code: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "commitguard",
            "gh_commit_guard",
            "Identity",
            "\"gh\"",
            "core.hooksPath",
            "recognized_ai",
        ] {
            assert!(
                !code.contains(forbidden),
                "{} mentions {forbidden}",
                path.display()
            );
        }
    }
}
