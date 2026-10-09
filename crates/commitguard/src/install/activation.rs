//! Transactional writes: skills, instruction blocks, then (unless skills-only) the Git guard.
use super::{
    assets::{AGENT_YAML, HOOKS, NAME, REFERENCES, SIGNERS, SKILL},
    instructions::mandate,
    paths::{io, mode, quote, set_mode},
    preflight::Context,
    transaction::Transaction,
};
use crate::{Config, Result, util};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub(super) fn activate(ctx: &Context, tx: &mut Transaction) -> Result<()> {
    install_skills(ctx, tx)?;
    mandate(tx, &ctx.mandate_files[0], &ctx.codex_skill.join("SKILL.md"))?;
    mandate(
        tx,
        &ctx.mandate_files[1],
        &ctx.claude_skill.join("SKILL.md"),
    )?;
    if ctx.skills_only {
        ctx.setup.command(
            &ctx.codex_skill.join("bin").join(ctx.exe),
            &["--strict", "account"],
            false,
            false,
        )?;
        return Ok(());
    }
    install_guard(ctx, tx)
}

fn install_skills(ctx: &Context, tx: &mut Transaction) -> Result<()> {
    let skills: BTreeSet<PathBuf> = [ctx.codex_skill.clone(), ctx.claude_skill.clone()]
        .into_iter()
        .collect();
    for skill in skills {
        tx.write(&skill.join("SKILL.md"), SKILL.as_bytes(), 0o644)?;
        for (file, text) in REFERENCES {
            tx.write(&skill.join("references").join(file), text.as_bytes(), 0o644)?;
        }
        let launcher = format!(
            "#!/bin/sh\nexec {} \"$@\"\n",
            quote(&skill.join("bin").join(ctx.exe))?
        );
        tx.write(&skill.join("scripts/check"), launcher.as_bytes(), 0o755)?;
        tx.write(&skill.join("bin").join(ctx.exe), &ctx.binary, 0o755)?;
        tx.write(
            &skill.join("bin").join(ctx.canonical_exe),
            &ctx.binary,
            0o755,
        )?;
        tx.write(&skill.join("agents/openai.yaml"), AGENT_YAML, 0o644)?;
    }
    Ok(())
}

/// Reads one optional Git value; empty when unset.
fn git_value(ctx: &Context, args: &[&str]) -> Result<String> {
    Ok(
        String::from_utf8_lossy(&ctx.setup.command(&ctx.git, args, false, true)?)
            .trim()
            .to_owned(),
    )
}

fn guard_config(ctx: &Context, hooks: &Path) -> Result<Value> {
    let (root, previous) = (&ctx.root, &ctx.previous);
    let old_hooks = git_value(
        ctx,
        &["config", "--global", "--path", "--get", "core.hooksPath"],
    )?;
    let mut verifiers: BTreeMap<String, String> = previous
        .as_ref()
        .and_then(|v| v.get("verify_programs"))
        .map(|v| serde_json::from_value(v.clone()).map_err(|_| "invalid verifier records"))
        .transpose()?
        .unwrap_or_default();
    for (kind, key, fallback) in [
        ("openpgp", "gpg.program", "gpg"),
        ("ssh", "gpg.ssh.program", "ssh-keygen"),
        ("x509", "gpg.x509.program", "gpgsm"),
    ] {
        if !verifiers.contains_key(kind) {
            let configured = git_value(ctx, &["config", "--global", "--get", key])?;
            let value = if configured.is_empty()
                || configured == root.join(format!("sign-{kind}")).to_string_lossy()
            {
                util::find_tool(fallback)
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| fallback.into())
            } else {
                configured
            };
            verifiers.insert(kind.into(), value);
        }
    }
    let saved_previous = previous
        .as_ref()
        .and_then(|v| v.get("previous_hooks"))
        .cloned()
        .unwrap_or(Value::Null);
    let prev = if !old_hooks.is_empty()
        && Path::new(&old_hooks) != hooks
        && Path::new(&old_hooks) != ctx.home.join(".codex/git-identity-guard/hooks")
    {
        Value::String(old_hooks)
    } else {
        saved_previous
    };
    let (git, gh) = (&ctx.git, &ctx.gh);
    let updated_config = json!({"schema_version":1,"git":git,"gh":gh,"root":root,"previous_hooks":prev,"repo_hooks":previous.as_ref().and_then(|v|v.get("repo_hooks")).cloned().unwrap_or(json!({})),"verify_programs":verifiers});
    let mut config_value = previous.clone().unwrap_or_else(|| json!({}));
    let existing = config_value
        .as_object_mut()
        .ok_or("existing guard configuration is not an object")?;
    for obsolete in ["bun", "python", "commitlint_script", "hooks"] {
        existing.remove(obsolete);
    }
    for (key, value) in updated_config
        .as_object()
        .ok_or("invalid updated guard configuration")?
    {
        existing.insert(key.clone(), value.clone());
    }
    let _: Config = serde_json::from_value(config_value.clone())
        .map_err(|_| "invalid native guard configuration")?;
    Ok(config_value)
}

fn configure_git(ctx: &Context, tx: &mut Transaction, hooks: &Path) -> Result<()> {
    let (root, setup, git) = (&ctx.root, &ctx.setup, &ctx.git);
    let global_mode = mode(&setup.global);
    tx.save(&setup.global)?;
    for (key, value) in [
        ("core.hooksPath", hooks.to_string_lossy().into_owned()),
        ("commit.gpgsign", "false".into()),
        ("tag.gpgsign", "false".into()),
        (
            "gpg.program",
            root.join("sign-openpgp").to_string_lossy().into_owned(),
        ),
        (
            "gpg.openpgp.program",
            root.join("sign-openpgp").to_string_lossy().into_owned(),
        ),
        (
            "gpg.ssh.program",
            root.join("sign-ssh").to_string_lossy().into_owned(),
        ),
        (
            "gpg.x509.program",
            root.join("sign-x509").to_string_lossy().into_owned(),
        ),
        ("devflow.commit-policy", "strict".into()),
    ] {
        setup.command(git, &["config", "--global", key, &value], false, false)?;
    }
    set_mode(&setup.global, global_mode)?;
    if let Some(local) = &ctx.local_config {
        let local_mode = mode(local);
        tx.save(local)?;
        setup.command(
            git,
            &["config", "--local", "devflow.commit-policy", "strict"],
            true,
            false,
        )?;
        setup.command(
            git,
            &[
                "config",
                "--local",
                "devflow.commit-policy-root",
                ctx.codex_skill.to_str().ok_or("skill path is not UTF-8")?,
            ],
            true,
            false,
        )?;
        set_mode(local, local_mode)?;
    }
    Ok(())
}

/// Shell profile PATH block; kept separate from the instruction block's marker semantics.
fn shell_block(original: &str, root: &Path) -> Result<String> {
    let begin = format!("# {NAME} PATH");
    let end = format!("# /{NAME} PATH");
    let block = format!(
        "{begin}\nexport PATH={}:\"$PATH\"\n{end}",
        quote(&root.join("bin"))?
    );
    match (original.find(&begin), original.find(&end)) {
        (Some(a), Some(b)) if b >= a => Ok(format!(
            "{}{}{}",
            &original[..a],
            block,
            &original[b + end.len()..]
        )),
        (None, None) => Ok(format!(
            "{}{}\n{}\n",
            original,
            if original.is_empty() || original.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            block
        )),
        _ => Err("malformed managed shell block".into()),
    }
}

fn install_guard(ctx: &Context, tx: &mut Transaction) -> Result<()> {
    let (root, config_path, binary) = (&ctx.root, &ctx.config_path, &ctx.binary);
    let hooks = root.join("hooks");
    let cli = root.join("bin").join(ctx.exe);
    let config_value = guard_config(ctx, &hooks)?;
    tx.write(&cli, binary, 0o755)?;
    tx.write(&root.join("bin").join(ctx.canonical_exe), binary, 0o755)?;
    tx.write(&root.join("bin").join(ctx.git_exe), binary, 0o755)?;
    tx.write(
        config_path,
        &serde_json::to_vec_pretty(&config_value)
            .map_err(|_| "cannot serialize native guard configuration")?,
        0o600,
    )?;
    for hook in HOOKS {
        let script = format!(
            "#!/bin/sh\nexec {} --config {} hook {} \"$@\"\n",
            quote(&cli)?,
            quote(config_path)?,
            hook
        );
        tx.write(&hooks.join(hook), script.as_bytes(), 0o755)?;
    }
    for kind in SIGNERS {
        let script = format!(
            "#!/bin/sh\nexec {} --config {} sign {} \"$@\"\n",
            quote(&cli)?,
            quote(config_path)?,
            kind
        );
        tx.write(&root.join(format!("sign-{kind}")), script.as_bytes(), 0o755)?;
    }
    configure_git(ctx, tx, &hooks)?;
    for file in &ctx.shells {
        let path = ctx.home.join(file);
        let original = if path.exists() {
            io(fs::read_to_string(&path), "read shell configuration")?
        } else {
            String::new()
        };
        let text = shell_block(&original, root)?;
        tx.write(&path, text.as_bytes(), mode(&path))?;
    }
    let config_arg = config_path
        .to_str()
        .ok_or("configuration path is not UTF-8")?;
    ctx.setup
        .command(&cli, &["--config", config_arg, "version"], false, false)?;
    ctx.setup.command(
        &cli,
        &["--config", config_arg, "--strict", "account"],
        false,
        false,
    )?;
    Ok(())
}
