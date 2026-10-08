//! Raw commit, attribution, graph and gitlink-only tree validation.
use super::{Entry, MAX_BYTES, Manifest, SourceProvenance, git::Git};
use crate::{Identity, Result, core, policy};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Commit<'a> {
    origin: Option<&'a str>,
    tree: &'a str,
    parents: Vec<&'a str>,
    author: &'a str,
    committer: &'a str,
    removed: BTreeSet<&'a str>,
    message: &'a str,
}
fn oid(value: &str, width: usize) -> Result<()> {
    if value.len() != width || !core::oid_valid(value) {
        return Err("malformed bulk commit OID".into());
    }
    Ok(())
}
pub(super) fn parse_profile(
    bytes: &[u8],
    candidate: bool,
    width: usize,
    origin_profile: bool,
) -> Result<Commit<'_>> {
    let raw = core::text(bytes)?;
    let (headers, message) = raw.split_once("\n\n").ok_or("malformed bulk commit")?;
    if headers.contains(['\r', '\0']) {
        return Err("malformed bulk commit headers".into());
    }
    let origin = if origin_profile {
        crate::provenance::header_origin(headers)?
    } else {
        None
    };
    let mut tree = None;
    let mut parents = Vec::new();
    let mut author = None;
    let mut committer = None;
    let mut removed = BTreeSet::new();
    let mut previous = "";
    let mut stage = 0;
    for line in headers.split('\n') {
        if line.starts_with(' ') {
            if candidate || !["gpgsig", "gpgsig-sha256", "mergetag"].contains(&previous) {
                return Err("unsupported bulk header continuation".into());
            }
            continue;
        }
        let (key, value) = line.split_once(' ').ok_or("malformed bulk header")?;
        previous = key;
        match key {
            "tree" if tree.is_none() && (!candidate || stage == 0) => {
                oid(value, width)?;
                tree = Some(value);
                stage = 1;
            }
            "parent" if !candidate || stage == 1 => {
                oid(value, width)?;
                parents.push(value);
            }
            "author" if author.is_none() && (!candidate || stage == 1) => {
                author = Some(value);
                stage = 2;
            }
            "committer" if committer.is_none() && (!candidate || stage == 2) => {
                committer = Some(value);
                stage = 3;
            }
            crate::provenance::KEY if origin.is_some() && stage == 3 => {}
            "gpgsig" | "gpgsig-sha256" | "encoding" | "mergetag" if !candidate => {
                if !removed.insert(key) {
                    return Err("duplicate removable bulk header".into());
                }
            }
            _ => return Err("unsupported, duplicate or out-of-order bulk header".into()),
        }
    }
    Ok(Commit {
        origin,
        tree: tree.ok_or("missing bulk tree")?,
        parents,
        author: author.ok_or("missing bulk author")?,
        committer: committer.ok_or("missing bulk committer")?,
        removed,
        message,
    })
}
pub(super) fn date(value: &str) -> Result<&str> {
    let (_, zone) = value.rsplit_once(' ').ok_or("invalid bulk identity date")?;
    if zone.len() != 5
        || !matches!(zone.as_bytes()[0], b'+' | b'-')
        || !zone.as_bytes()[1..].iter().all(u8::is_ascii_digit)
    {
        return Err("invalid bulk identity timezone".into());
    }
    let prefix = &value[..value.len() - zone.len() - 1];
    let (ident, timestamp) = prefix
        .rsplit_once(' ')
        .ok_or("invalid bulk identity timestamp")?;
    if !ident.ends_with('>')
        || timestamp.is_empty()
        || !timestamp.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("invalid bulk identity timestamp".into());
    }
    Ok(&value[ident.len() + 1..])
}

#[path = "credits.rs"]
mod credits;

fn consume(blocks: &mut Vec<credits::Block>, value: &str) -> Result<()> {
    let index = blocks
        .iter()
        .position(|b| b.raw == value)
        .ok_or("bulk credit mapping must consume an actual exact credit block")?;
    blocks.remove(index);
    Ok(())
}
fn main_value(header: &str) -> Result<String> {
    let (ident, _) = header
        .rsplit_once("> ")
        .ok_or("malformed source main identity")?;
    Ok(format!("{ident}>"))
}
fn attribution(
    entry: &Entry,
    source: &Commit<'_>,
    candidate: &str,
    identity: &Identity,
    enforce_human_policy: bool,
) -> Result<()> {
    let mut old = credits::scan(source.message)?;
    let mut new = credits::scan(candidate)?;
    let mut expected = credits::Counter::new();
    let mut actual = credits::Counter::new();
    for block in &old {
        if let Some(ai) = &block.ai {
            credits::add(&mut expected, ai);
        }
    }
    for block in &new {
        if let Some(ai) = &block.ai {
            credits::add(&mut actual, ai);
        }
        if enforce_human_policy && (block.key == "co-authored-by" || block.key == "signed-off-by") {
            policy::validate_attribution(block.raw.as_bytes(), identity)?;
        }
    }
    let mut mains = BTreeSet::new();
    for header in [source.author, source.committer] {
        if let Some(key) = credits::checked_standard(&main_value(header)?)? {
            mains.insert(key);
        }
    }
    let mut roles = BTreeSet::new();
    for declaration in &entry.main_credit_changes {
        let header = match declaration.role.as_str() {
            "author" => source.author,
            "committer" => source.committer,
            _ => {
                return Err("bulk main credit declaration role must be author or committer".into());
            }
        };
        if !roles.insert(&declaration.role) || declaration.old_identity != header {
            return Err(
                "bulk main credit declaration must freeze one exact actual role/header".into(),
            );
        }
        if core::check_ident(header, identity, "main identity").is_ok() {
            return Err("canonical human main identity cannot be relabeled as AI".into());
        }
        let block = credits::parse(&declaration.new)?;
        if block.key == "signed-off-by" {
            return Err("main AI attribution cannot become a sign-off certification".into());
        }
        let ai = block
            .ai
            .ok_or("main AI declaration requires recognized AI attribution")?;
        if ai.values().sum::<usize>() != 1 {
            return Err("one main identity must declare exactly one AI actor".into());
        }
        if !new.iter().any(|candidate| candidate.raw == declaration.new) {
            return Err("declared main AI credit is missing from the candidate message".into());
        }
        let key = ai.keys().next().unwrap().clone();
        if let Some(known) = credits::checked_standard(&main_value(header)?)?
            && key != known
        {
            return Err("recognized main AI provider/model/version/context cannot change".into());
        }
        mains.insert(key);
    }
    // Author and Committer may name the same actor, or that actor may already
    // have a body credit. Neither creates an additional contribution count.
    for key in mains {
        if !expected.contains_key(&key)
            && !new.iter().any(|block| {
                block.key != "signed-off-by"
                    && block
                        .ai
                        .as_ref()
                        .is_some_and(|actors| actors.contains_key(&key))
            })
        {
            return Err("main AI contribution requires a co-author or compact credit, never a new sign-off certification".into());
        }
        expected.entry(key).or_insert(1);
    }
    for change in &entry.credit_changes {
        let old_blocks = match (!change.old.is_empty(), !change.source_blocks.is_empty()) {
            (true, false) => {
                let blocks = credits::scan(&change.old)?;
                if blocks.is_empty()
                    || blocks
                        .iter()
                        .map(|block| block.raw.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                        != change.old
                    || !source.message.contains(&change.old)
                {
                    return Err(
                        "bulk credit mapping old must be exact contiguous source credit blocks"
                            .into(),
                    );
                }
                blocks
            }
            (false, true) => {
                let mut blocks = Vec::with_capacity(change.source_blocks.len());
                for raw in &change.source_blocks {
                    let mut parsed = credits::scan(raw)?;
                    if parsed.len() != 1 || parsed[0].raw != *raw || parsed[0].ai.is_none() {
                        return Err("bulk source_blocks must each name one exact physical recognized AI credit block".into());
                    }
                    blocks.push(parsed.remove(0));
                }
                blocks
            }
            _ => {
                return Err(
                    "bulk credit change requires exactly one nonempty old or source_blocks".into(),
                );
            }
        };
        let new_block = credits::parse(&change.new)?;
        let mut old_ai = credits::Counter::new();
        let all_ai = old_blocks.iter().all(|block| block.ai.is_some());
        if all_ai {
            for block in &old_blocks {
                credits::add(&mut old_ai, block.ai.as_ref().unwrap());
            }
            let new_ai = new_block
                .ai
                .as_ref()
                .ok_or("recognized AI credit cannot be removed or relabeled human")?;
            for block in &old_blocks {
                if (block.key == "signed-off-by" || new_block.key == "signed-off-by")
                    && block.key != new_block.key
                {
                    return Err("AI sign-off certification key cannot change".into());
                }
                if block.key != new_block.key
                    && block.key != "co-authored-by"
                    && !(block.key == "generated-with" && new_block.key == "ai-credit")
                {
                    return Err("only AI co-author or exact generated-tool credit may change to compact credit".into());
                }
            }
            if old_ai != *new_ai {
                return Err(
                    "bulk recognized AI provider/model/version/context/multiplicity cannot change"
                        .into(),
                );
            }
        } else {
            if old_blocks.len() != 1 || new_block.ai.is_some() {
                return Err("human or unknown credit cannot be grouped or relabeled AI".into());
            }
            let old_block = &old_blocks[0];
            if old_block.key != new_block.key {
                return Err("human attribution must preserve its standard credit key".into());
            }
            let canonical = format!("{} <{}>", identity.login, identity.email);
            if new_block.value != canonical {
                return Err("human credit mapping requires canonical gh identity".into());
            }
            if old_block.value != canonical && !change.owned {
                return Err(
                    "noncanonical human credit mapping requires explicit owned=true provenance"
                        .into(),
                );
            }
        }
        consume(&mut new, &change.new)?;
        for block in old_blocks {
            consume(&mut old, &block.raw)?;
        }
    }
    // Unmapped credits retain their exact physical bytes; semantic equality
    // alone does not authorize a format change or genuine human normalization.
    for block in old {
        consume(&mut new, &block.raw)?;
    }
    if new.iter().any(|block| block.ai.is_none()) {
        return Err("bulk candidate cannot invent human attribution or certification".into());
    }
    if actual != expected {
        return Err("bulk AI provider/model/version/context or multiplicity changed".into());
    }
    Ok(())
}

pub(super) fn all(
    git: &Git,
    manifest: &Manifest,
    sources: &[Vec<u8>],
    candidates: &[Vec<u8>],
    identity: &Identity,
) -> Result<()> {
    let mapping: BTreeMap<&str, &str> = manifest
        .entries
        .iter()
        .map(|e| (e.source_oid.as_str(), e.expected_oid.as_str()))
        .collect();
    let origin_profile = super::profile(manifest)?;
    let boundaries: BTreeSet<&str> = manifest.boundaries.iter().map(String::as_str).collect();
    let mut used_boundaries = BTreeSet::new();
    let mut tree_pairs = Vec::new();
    for ((entry, source), candidate) in manifest.entries.iter().zip(sources).zip(candidates) {
        let old = parse_profile(source, false, git.width, origin_profile)?;
        let new = parse_profile(candidate, true, git.width, origin_profile)?;
        core::validate_commit_bytes(&entry.expected_oid, candidate, identity)?;
        match entry.source_provenance {
            None if old.origin.is_some() || new.origin.is_some() => {
                return Err("bulk origin metadata requires an explicit provenance mode".into());
            }
            None => {}
            Some(SourceProvenance::Add) => {
                let digest = format!("{:x}", Sha256::digest(source));
                if old.origin.is_some()
                    || digest != entry.source_sha256
                    || new.origin != Some(digest.as_str())
                {
                    return Err("bulk origin add must bind the complete actual source bytes".into());
                }
            }
            Some(SourceProvenance::Preserve)
                if old.origin.is_none() || new.origin != old.origin =>
            {
                return Err("bulk origin preserve must copy the exact source origin".into());
            }
            Some(SourceProvenance::Preserve) => {}
        }
        if entry.source_provenance.is_some() && entry.ownership.is_none() {
            return Err("bulk origin profile requires positive exact source ownership".into());
        }
        if date(old.author)? != date(new.author)? || date(old.committer)? != date(new.committer)? {
            return Err("bulk candidate must preserve both raw date/timezone fields".into());
        }
        let own = core::check_ident(old.author, identity, "source author").is_ok();
        if let Some(ownership) = &entry.ownership {
            if !ownership.owned || ownership.old_author != old.author {
                return Err(
                    "bulk ownership must freeze the exact actual old author and owned=true".into(),
                );
            }
        } else if !own {
            return Err("bulk noncanonical source author requires explicit ownership".into());
        }
        let own_committer = core::check_ident(old.committer, identity, "source committer").is_ok();
        if let Some(ownership) = &entry.committer_ownership {
            if !ownership.owned || ownership.old_committer != old.committer {
                return Err("bulk committer ownership must freeze exact actual old committer and owned=true".into());
            }
        } else if !own_committer {
            return Err(
                "bulk noncanonical source committer requires explicit committer ownership".into(),
            );
        }
        let removed: BTreeSet<&str> = entry.remove_headers.iter().map(String::as_str).collect();
        if removed.len() != entry.remove_headers.len() || removed != old.removed {
            return Err(
                "bulk remove_headers must exactly declare actual removable source headers".into(),
            );
        }
        let mut parents = Vec::with_capacity(old.parents.len());
        for parent in &old.parents {
            if let Some(mapped) = mapping.get(parent) {
                parents.push(*mapped);
            } else if boundaries.contains(parent) {
                parents.push(*parent);
                used_boundaries.insert(*parent);
            } else {
                return Err("bulk outside-scope parent requires a declared boundary".into());
            }
        }
        if parents != new.parents {
            return Err("bulk candidate ordered parent mapping mismatch".into());
        }
        attribution(entry, &old, new.message, identity, true)?;
        tree_pairs.push((old.tree.to_string(), new.tree.to_string(), entry));
    }
    if used_boundaries != boundaries {
        return Err("bulk boundaries must exactly match actual outside-scope parents".into());
    }
    trees(git, &tree_pairs)
}

#[derive(Clone, PartialEq, Eq)]
struct TreeEntry {
    mode: Vec<u8>,
    oid: String,
}
fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if value.is_empty() || !value.len().is_multiple_of(2) {
        return Err("invalid bulk gitlink path_hex".into());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().as_chunks::<2>().0 {
        fn nibble(b: u8) -> Result<u8> {
            match b {
                b'0'..=b'9' => Ok(b - b'0'),
                b'a'..=b'f' => Ok(b - b'a' + 10),
                _ => Err("invalid bulk lowercase hex path".into()),
            }
        }
        bytes.push(nibble(pair[0])? * 16 + nibble(pair[1])?);
    }
    if bytes.contains(&0)
        || bytes
            .split(|b| *b == b'/')
            .any(|part| part.is_empty() || part == b"." || part == b"..")
    {
        return Err("invalid bulk gitlink path components".into());
    }
    Ok(bytes)
}
fn tree(bytes: &[u8], width: usize) -> Result<BTreeMap<Vec<u8>, TreeEntry>> {
    let mut entries = BTreeMap::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let space = bytes[cursor..]
            .iter()
            .position(|b| *b == b' ')
            .map(|n| cursor + n)
            .ok_or("malformed bulk binary tree mode")?;
        let mode = &bytes[cursor..space];
        if ![
            b"40000".as_slice(),
            b"100644",
            b"100755",
            b"120000",
            b"160000",
        ]
        .contains(&mode)
        {
            return Err("unsupported bulk tree mode".into());
        }
        let start = space + 1;
        let nul = bytes[start..]
            .iter()
            .position(|b| *b == 0)
            .map(|n| start + n)
            .ok_or("malformed bulk tree name")?;
        let name = &bytes[start..nul];
        if name.is_empty() || name.contains(&b'/') || name == b"." || name == b".." {
            return Err("invalid bulk tree name".into());
        }
        let end = (nul + 1)
            .checked_add(width / 2)
            .ok_or("bulk tree overflow")?;
        let raw_oid = bytes
            .get(nul + 1..end)
            .ok_or("truncated bulk binary tree OID")?;
        let oid = raw_oid
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if entries
            .insert(
                name.to_vec(),
                TreeEntry {
                    mode: mode.to_vec(),
                    oid,
                },
            )
            .is_some()
        {
            return Err("duplicate bulk tree name".into());
        }
        cursor = end;
    }
    Ok(entries)
}
struct Work {
    entry: usize,
    old: String,
    new: String,
    prefix: Vec<u8>,
    depth: usize,
}
fn trees(git: &Git, pairs: &[(String, String, &Entry)]) -> Result<()> {
    let mut declarations = Vec::with_capacity(pairs.len());
    let mut consumed: Vec<BTreeSet<Vec<u8>>> = vec![BTreeSet::new(); pairs.len()];
    let mut pending = Vec::new();
    for (index, (old, new, entry)) in pairs.iter().enumerate() {
        let mut paths = BTreeMap::new();
        for link in &entry.gitlinks {
            git.oid(&link.old_oid)?;
            git.oid(&link.new_oid)?;
            if link.old_oid == link.new_oid {
                return Err("bulk gitlink declaration must change an OID".into());
            }
            if paths
                .insert(
                    decode_hex(&link.path_hex)?,
                    (link.old_oid.as_str(), link.new_oid.as_str()),
                )
                .is_some()
            {
                return Err("duplicate bulk gitlink path".into());
            }
        }
        if old == new && !paths.is_empty() {
            return Err("bulk gitlink declaration is unused in unchanged tree".into());
        }
        // Also prove root tree objects exist for unchanged commits. Descendants
        // with identical OIDs need no traversal: their byte identity is fixed.
        pending.push(Work {
            entry: index,
            old: old.clone(),
            new: new.clone(),
            prefix: Vec::new(),
            depth: 0,
        });
        declarations.push(paths);
    }
    let mut cache: BTreeMap<String, BTreeMap<Vec<u8>, TreeEntry>> = BTreeMap::new();
    let mut total = 0usize;
    let mut work_count = 0usize;
    while !pending.is_empty() {
        let missing: BTreeSet<String> = pending
            .iter()
            .flat_map(|work| [&work.old, &work.new])
            .filter(|oid| !cache.contains_key(*oid))
            .cloned()
            .collect();
        let queries: Vec<String> = missing.into_iter().collect();
        let raw = git.batch(&queries, "tree")?;
        for (oid, bytes) in queries.into_iter().zip(raw) {
            total = total
                .checked_add(bytes.len())
                .ok_or("bulk tree aggregate overflow")?;
            if total > MAX_BYTES {
                return Err("bulk tree aggregate exceeds safe limit".into());
            }
            cache.insert(oid, tree(&bytes, git.width)?);
        }
        let mut next = Vec::new();
        for work in pending {
            work_count += 1;
            if work.depth > 256 || work_count > 1_000_000 {
                return Err("bulk tree traversal exceeds safe bounds".into());
            }
            if work.old == work.new {
                continue;
            }
            let old = cache
                .get(&work.old)
                .ok_or("bulk source tree cache missing")?;
            let new = cache
                .get(&work.new)
                .ok_or("bulk candidate tree cache missing")?;
            if old.len() != new.len() || old.keys().ne(new.keys()) {
                return Err("bulk tree names changed".into());
            }
            for (name, before) in old {
                let after = new.get(name).ok_or("bulk tree entry missing")?;
                if before.mode != after.mode {
                    return Err("bulk tree entry mode changed".into());
                }
                let mut path = work.prefix.clone();
                if !path.is_empty() {
                    path.push(b'/');
                }
                path.extend(name);
                if before.oid == after.oid {
                    continue;
                }
                if before.mode == b"40000" {
                    next.push(Work {
                        entry: work.entry,
                        old: before.oid.clone(),
                        new: after.oid.clone(),
                        prefix: path,
                        depth: work.depth + 1,
                    });
                } else if before.mode == b"160000" {
                    if declarations[work.entry].get(&path)
                        != Some(&(before.oid.as_str(), after.oid.as_str()))
                    {
                        return Err(
                            "bulk gitlink change lacks exact path/old/new declaration".into()
                        );
                    }
                    consumed[work.entry].insert(path);
                } else {
                    return Err("bulk tree blob or symlink OID changed".into());
                }
            }
        }
        pending = next;
    }
    for (declared, used) in declarations.iter().zip(consumed) {
        if declared.keys().cloned().collect::<BTreeSet<_>>() != used {
            return Err("bulk gitlink declaration did not match an actual change".into());
        }
    }
    Ok(())
}

/// Reuse the same provenance rules for an annotated tag's single main role.
pub(super) fn tag_attribution(
    entry: &Entry,
    tagger: &str,
    old_message: &str,
    new_message: &str,
    identity: &Identity,
) -> Result<()> {
    let source = Commit {
        tree: "",
        parents: Vec::new(),
        author: tagger,
        committer: tagger,
        removed: BTreeSet::new(),
        origin: None,
        message: old_message,
    };
    attribution(entry, &source, new_message, identity, false)
}
