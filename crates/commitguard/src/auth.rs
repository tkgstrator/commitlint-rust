//! Credential-bound local identity cache. Cache hits never contact the API.
use crate::{
    Identity, Result,
    core::{self, Tools},
    util,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
thread_local! { static STRICT: Cell<bool> = const { Cell::new(false) }; }
pub struct StrictScope {
    previous: bool,
}
impl StrictScope {
    pub fn enter() -> Self {
        let previous = STRICT.with(|v| v.replace(true));
        Self { previous }
    }
}
impl Drop for StrictScope {
    fn drop(&mut self) {
        STRICT.with(|v| v.set(self.previous));
    }
}
pub fn strict_scope() -> StrictScope {
    StrictScope::enter()
}
pub fn is_strict() -> bool {
    STRICT.with(Cell::get) || std::env::var("COMMITGUARD_STRICT").as_deref() == Ok("1")
}
struct Context {
    fingerprint: String,
    token: String,
}
fn hash_fields(fields: &[&[u8]]) -> String {
    let mut h = Sha256::new();
    for field in fields {
        h.update((field.len() as u64).to_be_bytes());
        h.update(field);
    }
    format!("{:x}", h.finalize())
}
fn path_bytes(path: &Path) -> Result<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(path.as_os_str().as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        Ok(path
            .to_str()
            .ok_or("invalid authentication path")?
            .as_bytes()
            .to_vec())
    }
}
fn context(tools: &Tools) -> Result<Context> {
    let output = util::capture_env_bounded(
        &tools.gh,
        &["auth", "token", "--hostname", "github.com"].map(str::to_string),
        None,
        Duration::from_secs(15),
        &[],
        65536,
    )?;
    if output.code != 0 || output.stdout.len() > 65536 {
        return Err(
            "local gh credential unavailable; run --strict account after authenticating".into(),
        );
    }
    let token = std::str::from_utf8(&output.stdout)
        .map_err(|_| "invalid local credential")?
        .trim()
        .to_string();
    if token.is_empty() || token.contains(['\n', '\r', '\0']) {
        return Err("invalid local credential".into());
    }
    let source = if std::env::var_os("GH_TOKEN").is_some_and(|v| !v.is_empty()) {
        "GH_TOKEN"
    } else if std::env::var_os("GITHUB_TOKEN").is_some_and(|v| !v.is_empty()) {
        "GITHUB_TOKEN"
    } else {
        "stored"
    };
    let selected = util::capture_env_bounded(
        &tools.gh,
        &["config", "get", "user", "--host", "github.com"].map(str::to_string),
        None,
        Duration::from_secs(15),
        &[],
        256,
    )?;
    if selected.stdout.len() > 256 {
        return Err("invalid local account context".into());
    }
    if selected.code != 0 && source == "stored" {
        return Err("local gh selected account unavailable".into());
    }
    let account = if selected.code == 0 {
        std::str::from_utf8(&selected.stdout)
            .map_err(|_| "invalid local account context")?
            .trim()
            .to_string()
    } else {
        String::new()
    };
    if account.len() > 256 || account.contains(['\r', '\n', '\0']) {
        return Err("invalid local account context".into());
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is unavailable")?;
    let cfg = std::env::var_os("GH_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("gh")
        });
    let cfg = if cfg.is_absolute() {
        cfg
    } else {
        std::env::current_dir()
            .map_err(|_| "cannot resolve authentication context")?
            .join(cfg)
    };
    let cfg = cfg.canonicalize().unwrap_or(cfg);
    let gh = tools
        .gh
        .canonicalize()
        .map_err(|_| "cannot resolve authentication tool")?;
    let fingerprint = hash_fields(&[
        b"commitguard-auth-context-v1",
        b"github.com",
        source.as_bytes(),
        &path_bytes(&cfg)?,
        account.as_bytes(),
        &path_bytes(&gh)?,
        hash_fields(&[b"commitguard-token-v1", token.as_bytes()]).as_bytes(),
    ]);
    Ok(Context { fingerprint, token })
}
pub fn context_fingerprint(tools: &Tools) -> Result<String> {
    Ok(context(tools)?.fingerprint)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cache {
    schema: u32,
    hostname: String,
    context: String,
    login: String,
    id: u64,
    verified_at: u64,
}
fn cache_dir() -> Result<PathBuf> {
    cache_directory_for_home(&PathBuf::from(std::env::var_os("HOME").unwrap_or_default()))
}
pub(crate) fn cache_directory_for_home(home: &Path) -> Result<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"));
    if !base.is_absolute() {
        return Err("authentication state directory must be absolute".into());
    }
    // macOS system aliases are unavoidable for temporary homes; user-controlled
    // symlink ancestors are never accepted as private cache storage.
    let resolved = base;
    #[cfg(target_os = "macos")]
    let mut resolved = resolved;
    #[cfg(target_os = "macos")]
    for alias in ["/var", "/tmp", "/etc"] {
        if let Ok(rest) = resolved.strip_prefix(alias) {
            let root = Path::new(alias)
                .canonicalize()
                .map_err(|_| "cannot resolve system state path")?;
            resolved = root.join(rest);
            break;
        }
    }
    check_ancestors(&resolved)?;
    Ok(resolved.join("commitguard"))
}
fn check_ancestors(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(parent)
            && (!meta.is_dir() || meta.file_type().is_symlink())
        {
            return Err("unsafe authentication cache directory".into());
        }
    }
    Ok(())
}
fn private_dir(create: bool) -> Result<PathBuf> {
    let path = cache_dir()?;
    check_ancestors(&path)?;
    if create {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut b = fs::DirBuilder::new();
            b.recursive(true).mode(0o700);
            b.create(&path)
                .map_err(|_| "cannot create private authentication state")?;
        }
        #[cfg(not(unix))]
        {
            return Err("private authentication cache is unsupported on this platform".into());
        }
    }
    let meta = fs::symlink_metadata(&path)
        .map_err(|_| "identity cache missing; run commitguard --strict account")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if meta.permissions().mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
            return Err("authentication cache directory must be private".into());
        }
    }
    Ok(path)
}
fn read_cache(context: &Context) -> Result<Identity> {
    let path = private_dir(false)?.join(format!("{}.json", context.fingerprint));
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "identity cache missing or changed; run commitguard --strict account")?;
    let meta = file
        .metadata()
        .map_err(|_| "cannot inspect identity cache")?;
    if !meta.is_file() || meta.len() > 16384 {
        return Err("invalid identity cache; run --strict account".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if meta.permissions().mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
            return Err("identity cache must be private".into());
        }
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read identity cache")?;
    if bytes.len() > 16384 {
        return Err("oversized identity cache".into());
    }
    let record: Cache = serde_json::from_slice(&bytes)
        .map_err(|_| "invalid identity cache; run --strict account")?;
    if record.schema != 1
        || record.hostname != "github.com"
        || record.context != context.fingerprint
        || record.verified_at == 0
    {
        return Err("invalid identity cache binding".into());
    }
    core::decode_account(
        &serde_json::to_vec(
            &serde_json::json!({"login":record.login,"id":record.id,"type":"User"}),
        )
        .map_err(|_| "invalid identity cache")?,
    )
}
fn write_cache(ctx: &Context, identity: &Identity) -> Result<()> {
    let id = identity
        .email
        .split('+')
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("invalid validated identity")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock unavailable")?;
    let cache = Cache {
        schema: 1,
        hostname: "github.com".into(),
        context: ctx.fingerprint.clone(),
        login: identity.login.clone(),
        id,
        verified_at: now.as_secs(),
    };
    let dir = private_dir(true)?;
    let temp = dir.join(format!(".tmp-{}-{}", std::process::id(), now.as_nanos()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&temp)
            .map_err(|_| "cannot create private identity cache")?;
        file.write_all(&serde_json::to_vec(&cache).map_err(|_| "cannot serialize identity cache")?)
            .map_err(|_| "cannot write identity cache")?;
        file.sync_all()
            .map_err(|_| "cannot synchronize identity cache")?;
        check_ancestors(&dir)?;
        fs::rename(&temp, dir.join(format!("{}.json", ctx.fingerprint)))
            .map_err(|_| "cannot publish identity cache")?;
        File::open(&dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| "cannot synchronize identity directory")?;
        Ok(())
    })();
    let _ = fs::remove_file(&temp);
    result
}
pub fn account(tools: &Tools) -> Result<Identity> {
    Ok(account_with_context(tools)?.0)
}
/// Return the identity and the exact credential context verified together.
pub fn account_with_context(tools: &Tools) -> Result<(Identity, String)> {
    if is_strict() {
        return strict_account_with_context(tools);
    }
    let ctx = context(tools)?;
    let identity = read_cache(&ctx)?;
    if context(tools)?.fingerprint != ctx.fingerprint {
        return Err("local credential changed during verification".into());
    }
    Ok((identity, ctx.fingerprint))
}
pub fn strict_account(tools: &Tools) -> Result<Identity> {
    Ok(strict_account_with_context(tools)?.0)
}
fn strict_account_with_context(tools: &Tools) -> Result<(Identity, String)> {
    let ctx = context(tools)?;
    let output = util::capture_env_bounded(
        &tools.gh,
        &["api", "--hostname", "github.com", "user"].map(str::to_string),
        None,
        Duration::from_secs(15),
        &[
            ("GH_TOKEN", Some(ctx.token.as_str())),
            ("GITHUB_TOKEN", None),
            ("GH_HOST", None),
            ("GH_ENTERPRISE_TOKEN", None),
            ("GITHUB_ENTERPRISE_TOKEN", None),
        ],
        16384,
    )?;
    if output.code != 0 {
        return Err("strict online gh authentication failed; cache fallback is forbidden".into());
    }
    if output.stdout.len() > 16384 {
        return Err("oversized strict authentication response".into());
    }
    let identity = core::decode_account(&output.stdout)?;
    if context(tools)?.fingerprint != ctx.fingerprint {
        return Err("local credential changed during strict verification".into());
    }
    write_cache(&ctx, &identity)?;
    Ok((identity, ctx.fingerprint))
}
