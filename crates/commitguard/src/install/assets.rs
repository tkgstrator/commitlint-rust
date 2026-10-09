//! Embedded skill resources and managed names.
pub(super) const SKILL: &str = include_str!("../../../../resources/SKILL.md");
pub(super) const NAME: &str = "gh-commit-identity";
pub(super) const REFERENCES: &[(&str, &str)] = &[
    (
        "host-setup.md",
        include_str!("../../../../resources/references/host-setup.md"),
    ),
    (
        "portable-setup.md",
        include_str!("../../../../resources/references/portable-setup.md"),
    ),
    (
        "mac-hooks.md",
        include_str!("../../../../resources/references/mac-hooks.md"),
    ),
];
pub(super) const AGENT_YAML: &[u8] = b"interface:\n  display_name: Git Commit Identity\n  short_description: Check opted-in human gh commit policy\n";
pub(super) const SIGNERS: [&str; 3] = ["openpgp", "ssh", "x509"];
pub(super) const HOOKS: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];
