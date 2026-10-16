//! Directory checkouts bound to a session (spec §4.9, v1: explicit checkout and sync).
//!
//! A checkout is a directory plus `.trellis/checkout.json`, which records the server,
//! repository, session, scopes, and the content hash of every file as last synced. `sync`
//! sends local edits to the session; `update` rebases the session and refreshes the files.
//! Untracked files matching the root `.gitignore` (build output such as `__pycache__/`) are
//! never sent.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use trellis_coord::Scope;
use trellis_core::{Hash, RepoPath};
use trellis_land::{Intent, RebaseOutcome};

use crate::client::Client;
use crate::error::{ClientError, Result};

const META_DIR: &str = ".trellis";
const MANIFEST: &str = "checkout.json";

/// A directory bound to a session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkout {
    pub server: String,
    pub repo: String,
    pub session: String,
    pub principal: String,
    pub scopes: Vec<String>,
    /// Path → content hash as last synced with the session.
    pub files: BTreeMap<String, Hash>,
}

/// Local edits not yet sent to the session.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changes {
    pub written: Vec<String>,
    pub deleted: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.written.is_empty() && self.deleted.is_empty()
    }
}

fn parse_scopes(scopes: &[String]) -> Result<Vec<Scope>> {
    scopes
        .iter()
        .map(|s| Scope::parse(s).map_err(|e| ClientError::Invalid(e.to_string())))
        .collect()
}

impl Checkout {
    pub fn load(dir: &Path) -> Result<Self> {
        let bytes = fs::read(dir.join(META_DIR).join(MANIFEST)).map_err(|e| {
            ClientError::Invalid(format!("{} is not a checkout: {e}", dir.display()))
        })?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn save(&self, dir: &Path) -> Result<()> {
        let meta = dir.join(META_DIR);
        fs::create_dir_all(&meta)?;
        fs::write(meta.join(MANIFEST), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn client(&self) -> Result<Client> {
        Client::new(&self.server, &self.repo)
    }

    fn in_scope(&self, path: &RepoPath) -> Result<bool> {
        let scopes = parse_scopes(&self.scopes)?;
        Ok(scopes.is_empty() || scopes.iter().any(|s| s.covers(path)))
    }
}

/// Watches the checkout's scopes from `session`, so landings there notify it.
fn watch(client: &Client, session: &str, scopes: &[String], dir: &Path) -> Result<()> {
    let intent = Intent {
        description: format!("checkout at {}", dir.display()),
        reads: parse_scopes(scopes)?,
        ..Default::default()
    };
    client.declare_intent(session, &intent)?;
    Ok(())
}

/// Makes the directory match the session's view within the checkout's scopes.
fn materialize(client: &Client, checkout: &mut Checkout, dir: &Path) -> Result<()> {
    let view = client.files(&checkout.session, &checkout.scopes)?;
    let mut next = BTreeMap::new();
    for entry in view {
        let rel = entry.path.as_str().to_owned();
        let target = dir.join(&rel);
        if checkout.files.get(&rel) != Some(&entry.blob.hash) || !target.exists() {
            let content = client.blob(&entry.blob.hash)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, content)?;
        }
        next.insert(rel, entry.blob.hash);
    }
    for old in checkout.files.keys() {
        if !next.contains_key(old) {
            let _ = fs::remove_file(dir.join(old));
        }
    }
    checkout.files = next;
    Ok(())
}

/// Materializes `session`'s view of `scopes` (empty means everything) into `dir`.
pub fn checkout(
    client: &Client,
    session: &str,
    principal: &str,
    scopes: &[String],
    dir: &Path,
) -> Result<Checkout> {
    let scopes = if scopes.is_empty() {
        vec!["**".to_owned()]
    } else {
        scopes.to_vec()
    };
    watch(client, session, &scopes, dir)?;
    fs::create_dir_all(dir)?;
    let mut co = Checkout {
        server: client.server().to_owned(),
        repo: client.repo().to_owned(),
        session: session.to_owned(),
        principal: principal.to_owned(),
        scopes,
        files: BTreeMap::new(),
    };
    materialize(client, &mut co, dir)?;
    co.save(dir)?;
    Ok(co)
}

/// Binds an existing checkout to another session (typically a new one after landing) and
/// refreshes the files to its view. Refuses if there are unsynced local edits.
pub fn rebind(dir: &Path, session: &str) -> Result<Checkout> {
    let (mut co, changes) = status(dir)?;
    if !changes.is_empty() {
        return Err(ClientError::Invalid(
            "the checkout has unsynced edits; sync or discard them first".into(),
        ));
    }
    let client = co.client()?;
    watch(&client, session, &co.scopes, dir)?;
    co.session = session.to_owned();
    materialize(&client, &mut co, dir)?;
    co.save(dir)?;
    Ok(co)
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || (dir == root && name == META_DIR) {
            continue;
        }
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            walk(root, &path, out)?;
        } else if kind.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| ClientError::Invalid(e.to_string()))?;
            let rel: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.insert(rel.join("/"), path);
        }
    }
    Ok(())
}

/// The subset of `.gitignore` a checkout honors: blank lines and `#` comments are skipped, a
/// trailing `/` matches directories only, a pattern with another `/` matches the whole path from
/// the root, and any other pattern matches a file or directory name at any depth; `*` and `?`
/// are wildcards that do not cross `/`. Negation (`!`) is not supported.
struct Ignore(Vec<(String, bool)>);

impl Ignore {
    fn load(dir: &Path) -> Self {
        let text = fs::read_to_string(dir.join(".gitignore")).unwrap_or_default();
        Ignore(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
                .map(|l| match l.strip_suffix('/') {
                    Some(d) => (d.trim_start_matches('/').to_owned(), true),
                    None => (l.trim_start_matches('/').to_owned(), false),
                })
                .collect(),
        )
    }

    fn matches(&self, rel: &str) -> bool {
        let parts: Vec<&str> = rel.split('/').collect();
        self.0.iter().any(|(pattern, dir_only)| {
            if pattern.contains('/') {
                // Anchored: the whole path, or (for a directory pattern) a leading part of it.
                (1..=parts.len())
                    .any(|n| (!dir_only || n < parts.len()) && glob(pattern, &parts[..n].join("/")))
            } else {
                let names = if *dir_only {
                    &parts[..parts.len() - 1]
                } else {
                    &parts[..]
                };
                names.iter().any(|name| glob(pattern, name))
            }
        })
    }
}

/// `*` matches any run of characters except `/`, `?` one character except `/`.
fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some((b'*', rest)) => (0..=t.len())
                .take_while(|&i| i == 0 || t[i - 1] != b'/')
                .any(|i| go(rest, &t[i..])),
            Some((b'?', rest)) => t.first().is_some_and(|&c| c != b'/') && go(rest, &t[1..]),
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

/// Local edits relative to the last sync, within the checkout's scopes.
pub fn status(dir: &Path) -> Result<(Checkout, Changes)> {
    let co = Checkout::load(dir)?;
    let mut local = BTreeMap::new();
    walk(dir, dir, &mut local)?;
    let ignore = Ignore::load(dir);
    let mut changes = Changes::default();
    for (rel, file) in &local {
        if !co.files.contains_key(rel) && ignore.matches(rel) {
            continue;
        }
        let Ok(path) = RepoPath::parse(rel) else {
            continue;
        };
        if !co.in_scope(&path)? {
            continue;
        }
        if co.files.get(rel) != Some(&Hash::of(&fs::read(file)?)) {
            changes.written.push(rel.clone());
        }
    }
    for rel in co.files.keys() {
        if !local.contains_key(rel) {
            changes.deleted.push(rel.clone());
        }
    }
    Ok((co, changes))
}

/// Sends local edits to the session.
pub fn sync(dir: &Path) -> Result<Changes> {
    let (mut co, changes) = status(dir)?;
    let client = co.client()?;
    for rel in &changes.written {
        let content = fs::read(dir.join(rel))?;
        client.write(&co.session, rel, &content)?;
        co.files.insert(rel.clone(), Hash::of(&content));
    }
    for rel in &changes.deleted {
        client.delete(&co.session, rel)?;
        co.files.remove(rel);
    }
    co.save(dir)?;
    Ok(changes)
}

/// Syncs, rebases the session onto the latest `main`, and refreshes the directory.
pub fn update(dir: &Path) -> Result<RebaseOutcome> {
    sync(dir)?;
    let mut co = Checkout::load(dir)?;
    let client = co.client()?;
    let outcome = client.rebase(&co.session)?;
    if matches!(outcome, RebaseOutcome::Rebased { .. }) {
        materialize(&client, &mut co, dir)?;
        co.save(dir)?;
    }
    Ok(outcome)
}

/// Writes each text conflict's merge, with conflict markers, into its file so it can be
/// edited into the resolution. Returns the paths written.
pub fn write_markers(dir: &Path, conflict: &str) -> Result<Vec<String>> {
    let co = Checkout::load(dir)?;
    let record = co.client()?.conflict(conflict)?;
    let mut written = Vec::new();
    for path in &record.paths {
        if path.reason == "TextOverlap" {
            fs::write(dir.join(path.path.as_str()), &path.detail)?;
            written.push(path.path.as_str().to_owned());
        }
    }
    Ok(written)
}

/// Resolves a conflict with the directory's current contents of every conflicting path (a
/// missing file resolves to a delete), then refreshes the directory.
pub fn resolve(dir: &Path, conflict: &str) -> Result<RebaseOutcome> {
    sync(dir)?;
    let mut co = Checkout::load(dir)?;
    let client = co.client()?;
    let record = client.conflict(conflict)?;
    let mut resolutions = BTreeMap::new();
    for path in &record.paths {
        let file = dir.join(path.path.as_str());
        let content = if file.exists() {
            Some(fs::read_to_string(&file)?)
        } else {
            None
        };
        resolutions.insert(path.path.as_str().to_owned(), content);
    }
    let outcome = client.resolve_conflict(conflict, &resolutions)?;
    if matches!(outcome, RebaseOutcome::Rebased { .. }) {
        materialize(&client, &mut co, dir)?;
        co.save(dir)?;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_patterns() {
        let ignore = Ignore(
            ["__pycache__/", "*.pyc", "build/out", "/dist/", "a?c"]
                .iter()
                .map(|l| match l.strip_suffix('/') {
                    Some(d) => (d.trim_start_matches('/').to_owned(), true),
                    None => (l.to_string(), false),
                })
                .collect(),
        );
        assert!(ignore.matches("pkg/__pycache__/m.pyc"));
        assert!(ignore.matches("junk.pyc"));
        assert!(ignore.matches("x/y/z.pyc"));
        assert!(!ignore.matches("pyc.txt"));
        assert!(
            !ignore.matches("__pycache__"),
            "a file named like an ignored directory"
        );
        assert!(ignore.matches("build/out"));
        assert!(!ignore.matches("src/build/out"));
        assert!(ignore.matches("dist/app.js"));
        assert!(ignore.matches("abc"));
        assert!(!ignore.matches("a/c"));
    }
}
