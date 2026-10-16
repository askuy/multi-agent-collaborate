# M0: Core, Store, Merge, Derive — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Trellis foundation: content-addressed changesets with a canonical encoding, blob and metadata storage with single-row compare-and-swap (CAS) refs, three-way merge drivers by file kind, and a derived-data framework with path sets, skip lists, and a Merkle tree index.

**Architecture:** Four crates in one Cargo workspace. `trellis-core` holds pure types (`Hash`, `RepoPath`, `FileKind`, `Changeset`) and a hand-written canonical codec, so a changeset id is the BLAKE3 hash of its encoding (spec §4.1, from Scion and Mononoke). `trellis-store` defines the `BlobStore` and `MetaStore` traits, with filesystem, in-memory, and SQLite implementations; ref updates are a conditional single-row `UPDATE` (spec §4.1, from Scion's single Spanner row). `trellis-merge` implements `merge3` by `FileKind` (spec §4.2 compose route). `trellis-derive` computes derived data recursively from parents and caches it under `(repo, changeset, name, version)` (spec §4.8, from Mononoke), and provides `PathSet`, `SkipList`, `TreeIndex`, and `paths_changed_since`.

**Tech Stack:** Rust 2024 edition (toolchain 1.94), `blake3`, `hex`, `thiserror` 2, `zstd`, `rusqlite` (bundled SQLite), `tempfile`, `diffy`, `serde_json`, `serde_yaml`, `proptest`.

**Spec:** `docs/superpowers/specs/2026-10-15-trellis-design.md` (§4.1, §4.2 compose, §4.8, §6, §7 invariants 7 and 8, §10).

---

## File Structure

```text
Cargo.toml                                 workspace: members, shared dependency versions, lints
.gitignore                                 + /target, .idea/
crates/trellis-core/
  Cargo.toml
  src/lib.rs                               module wiring and re-exports
  src/error.rs                             CoreError, Result
  src/hash.rs                              Hash (BLAKE3-256): of, from_slice, hex, Display, FromStr
  src/path.rs                              RepoPath: normalized repo-relative path
  src/kind.rs                              FileKind: Text, Markdown, Json, Yaml, Binary; infer
  src/codec.rs                             Writer/Reader: canonical big-endian + u32 length-prefixed
  src/changeset.rs                         BlobRef, FileChange, Provenance, Changeset (encode/decode/id)
crates/trellis-store/
  Cargo.toml
  src/lib.rs
  src/error.rs                             StoreError, Result
  src/blob.rs                              BlobStore trait, FsBlobStore (zstd files), MemBlobStore
  src/meta.rs                              MetaStore trait, RefState
  src/sqlite.rs                            SqliteMetaStore: changesets, refs (CAS), derived
crates/trellis-merge/
  Cargo.toml
  src/lib.rs                               merge3, MergeResult, Conflict, ConflictReason
  src/text.rs                              line-based three-way merge (diffy)
  src/structured.rs                        field-level JSON/YAML merge; arrays keyed by "id"
crates/trellis-derive/
  Cargo.toml
  src/lib.rs
  src/error.rs                             DeriveError, Result
  src/framework.rs                         Derivable trait, DeriveCtx (iterative, cached derivation)
  src/pathset.rs                           PathSet: paths changed by one changeset
  src/skiplist.rs                          SkipList + ancestor_at, is_first_parent_ancestor, first_parent_merge_base
  src/tree.rs                              Tree, TreeEntry, TreeIndex (Merkle), lookup, flatten, read_file
  src/paths.rs                             paths_changed_since
  src/testutil.rs                          (cfg(test)) Fixture for building changeset chains
```

---

### Task 1: Workspace and `Hash`

**Files:**
- Create: `Cargo.toml`
- Modify: `.gitignore`
- Create: `crates/trellis-core/Cargo.toml`, `crates/trellis-core/src/lib.rs`, `crates/trellis-core/src/error.rs`, `crates/trellis-core/src/hash.rs`

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
publish = false

[workspace.dependencies]
trellis-core = { path = "crates/trellis-core" }
trellis-store = { path = "crates/trellis-store" }
trellis-merge = { path = "crates/trellis-merge" }
trellis-derive = { path = "crates/trellis-derive" }
blake3 = "1"
hex = "0.4"
thiserror = "2"
zstd = "0.13"
rusqlite = { version = "0.37", features = ["bundled"] }
tempfile = "3"
diffy = "0.4"
serde_json = "1"
serde_yaml = "0.9"
proptest = "1"

[workspace.lints.rust]
unsafe_code = "forbid"
```

Append to `.gitignore`:

```text
/target
.idea/
```

- [ ] **Step 2: Create the `trellis-core` crate with `Hash` tests**

`crates/trellis-core/Cargo.toml`:

```toml
[package]
name = "trellis-core"
description = "Core Trellis types: content hashes, repository paths, file kinds, and changesets with a canonical encoding."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
blake3 = { workspace = true }
hex = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
proptest = { workspace = true }
```

`crates/trellis-core/src/lib.rs`:

```rust
//! Core Trellis types: content hashes, repository paths, file kinds, and changesets.

mod error;
mod hash;

pub use error::{CoreError, Result};
pub use hash::Hash;
```

`crates/trellis-core/src/error.rs`:

```rust
/// Errors from core types and the canonical codec.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid path {path:?}: {reason}")]
    InvalidPath { path: String, reason: &'static str },
    #[error("invalid hash: {0}")]
    InvalidHash(String),
    #[error("decode error: {0}")]
    Decode(&'static str),
}

pub type Result<T> = std::result::Result<T, CoreError>;
```

`crates/trellis-core/src/hash.rs` (the type and its tests; the methods come in Step 4):

```rust
use std::fmt;
use std::str::FromStr;

use crate::error::{CoreError, Result};

/// A BLAKE3-256 content hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_of_is_blake3_and_round_trips_through_hex() {
        let h = Hash::of(b"hello");
        assert_eq!(h.to_hex(), blake3::hash(b"hello").to_hex().to_string());
        assert_eq!(h.to_string(), h.to_hex());
        assert_eq!(h.to_hex().parse::<Hash>().unwrap(), h);
        assert_eq!(Hash::from_slice(h.as_bytes()).unwrap(), h);
    }

    #[test]
    fn rejects_wrong_length_and_bad_hex() {
        assert!(matches!("abcd".parse::<Hash>(), Err(CoreError::InvalidHash(_))));
        assert!(matches!("zz".parse::<Hash>(), Err(CoreError::InvalidHash(_))));
        assert!(Hash::from_slice(&[0u8; 31]).is_err());
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p trellis-core`
Expected: compile error, `no function or associated item named 'of' found for struct 'Hash'`.

- [ ] **Step 4: Implement `Hash`**

Insert between the struct and the tests in `crates/trellis-core/src/hash.rs`:

```rust
impl Hash {
    /// Wraps raw hash bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Hashes `data` with BLAKE3.
    pub fn of(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Reads a hash from a slice that must be exactly 32 bytes long.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let array: [u8; 32] = bytes.try_into().map_err(|_| {
            CoreError::InvalidHash(format!("expected 32 bytes, got {}", bytes.len()))
        })?;
        Ok(Self(array))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", &self.to_hex()[..12])
    }
}

impl FromStr for Hash {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(|e| CoreError::InvalidHash(e.to_string()))?;
        Self::from_slice(&bytes)
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p trellis-core`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml .gitignore crates/trellis-core
git commit -m "feat(core): add workspace and BLAKE3 Hash type"
```

---

### Task 2: `RepoPath`

**Files:**
- Create: `crates/trellis-core/src/path.rs`
- Modify: `crates/trellis-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

`crates/trellis-core/src/path.rs`:

```rust
use std::fmt;

use crate::error::{CoreError, Result};

/// A normalized, repository-relative path: `/`-separated, with no empty, `.`, or `..`
/// segments, no leading `/`, no backslash, and no NUL byte.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct RepoPath(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_paths() {
        for p in ["a", "a/b.md", "dir/sub/file.json", "名字/文件.md", ".hidden/x"] {
            assert_eq!(RepoPath::parse(p).unwrap().as_str(), p);
        }
    }

    #[test]
    fn rejects_malformed_paths() {
        for p in ["", "/a", "a//b", "a/", "./a", "a/../b", "..", "a\\b", "a\0b"] {
            assert!(RepoPath::parse(p).is_err(), "{p:?} should be rejected");
        }
    }

    #[test]
    fn exposes_segments_and_file_name() {
        let p = RepoPath::parse("a/b/c.md").unwrap();
        assert_eq!(p.file_name(), "c.md");
        assert_eq!(p.segments().collect::<Vec<_>>(), ["a", "b", "c.md"]);
        assert_eq!(RepoPath::parse("top").unwrap().file_name(), "top");
        assert_eq!(p.to_string(), "a/b/c.md");
    }
}
```

Modify `crates/trellis-core/src/lib.rs`:

```rust
//! Core Trellis types: content hashes, repository paths, file kinds, and changesets.

mod error;
mod hash;
mod path;

pub use error::{CoreError, Result};
pub use hash::Hash;
pub use path::RepoPath;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-core path`
Expected: compile error, `no function or associated item named 'parse'`.

- [ ] **Step 3: Implement `RepoPath`**

Insert between the struct and the tests:

```rust
impl RepoPath {
    pub fn parse(s: &str) -> Result<Self> {
        let invalid = |reason: &'static str| -> Result<Self> {
            Err(CoreError::InvalidPath { path: s.to_owned(), reason })
        };
        if s.is_empty() {
            return invalid("empty");
        }
        if s.starts_with('/') {
            return invalid("absolute");
        }
        if s.contains('\\') {
            return invalid("backslash");
        }
        if s.contains('\0') {
            return invalid("nul byte");
        }
        for segment in s.split('/') {
            match segment {
                "" => return invalid("empty segment"),
                "." | ".." => return invalid("dot segment"),
                _ => {}
            }
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn segments(&self) -> std::str::Split<'_, char> {
        self.0.split('/')
    }

    pub fn file_name(&self) -> &str {
        match self.0.rfind('/') {
            Some(i) => &self.0[i + 1..],
            None => &self.0,
        }
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-core`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-core
git commit -m "feat(core): add normalized RepoPath"
```

---

### Task 3: `FileKind`

**Files:**
- Create: `crates/trellis-core/src/kind.rs`
- Modify: `crates/trellis-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

`crates/trellis-core/src/kind.rs`:

```rust
use crate::error::{CoreError, Result};
use crate::path::RepoPath;

/// How a file is merged and validated.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FileKind {
    Text,
    Markdown,
    Json,
    Yaml,
    Binary,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> RepoPath {
        RepoPath::parse(p).unwrap()
    }

    #[test]
    fn round_trips_through_u8() {
        for kind in [FileKind::Text, FileKind::Markdown, FileKind::Json, FileKind::Yaml, FileKind::Binary] {
            assert_eq!(FileKind::from_u8(kind.to_u8()).unwrap(), kind);
        }
        assert!(FileKind::from_u8(0).is_err());
        assert!(FileKind::from_u8(6).is_err());
    }

    #[test]
    fn infers_from_content_then_extension() {
        assert_eq!(FileKind::infer(&path("a/README.md"), b"# hi"), FileKind::Markdown);
        assert_eq!(FileKind::infer(&path("x.MARKDOWN"), b"hi"), FileKind::Markdown);
        assert_eq!(FileKind::infer(&path("c.json"), b"{}"), FileKind::Json);
        assert_eq!(FileKind::infer(&path("c.yml"), b"a: 1"), FileKind::Yaml);
        assert_eq!(FileKind::infer(&path("c.yaml"), b"a: 1"), FileKind::Yaml);
        assert_eq!(FileKind::infer(&path("main.rs"), b"fn main() {}"), FileKind::Text);
        assert_eq!(FileKind::infer(&path("Makefile"), b"all:"), FileKind::Text);
        assert_eq!(FileKind::infer(&path("img.md"), b"\x00\x01"), FileKind::Binary);
        assert_eq!(FileKind::infer(&path("bad.txt"), &[0xff, 0xfe]), FileKind::Binary);
    }
}
```

Modify `crates/trellis-core/src/lib.rs`:

```rust
//! Core Trellis types: content hashes, repository paths, file kinds, and changesets.

mod error;
mod hash;
mod kind;
mod path;

pub use error::{CoreError, Result};
pub use hash::Hash;
pub use kind::FileKind;
pub use path::RepoPath;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-core kind`
Expected: compile error, `no function or associated item named 'from_u8'`.

- [ ] **Step 3: Implement `FileKind`**

Insert between the enum and the tests:

```rust
impl FileKind {
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Text => 1,
            Self::Markdown => 2,
            Self::Json => 3,
            Self::Yaml => 4,
            Self::Binary => 5,
        }
    }

    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Text),
            2 => Ok(Self::Markdown),
            3 => Ok(Self::Json),
            4 => Ok(Self::Yaml),
            5 => Ok(Self::Binary),
            _ => Err(CoreError::Decode("unknown file kind")),
        }
    }

    /// Binary if the content is not UTF-8 or has a NUL byte in its first 8 KiB;
    /// otherwise chosen by extension, defaulting to `Text`.
    pub fn infer(path: &RepoPath, content: &[u8]) -> Self {
        let head = &content[..content.len().min(8192)];
        if head.contains(&0) || std::str::from_utf8(content).is_err() {
            return Self::Binary;
        }
        let extension = path
            .file_name()
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        match extension.as_deref() {
            Some("md" | "markdown") => Self::Markdown,
            Some("json") => Self::Json,
            Some("yaml" | "yml") => Self::Yaml,
            _ => Self::Text,
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-core`
Expected: `test result: ok. 7 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-core
git commit -m "feat(core): add FileKind with content and extension inference"
```

---

### Task 4: Canonical codec

**Files:**
- Create: `crates/trellis-core/src/codec.rs`
- Modify: `crates/trellis-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

`crates/trellis-core/src/codec.rs`:

```rust
//! A canonical binary codec: a magic prefix, big-endian fixed-width integers, and
//! u32-length-prefixed byte strings. Every value has exactly one encoding.

use crate::error::{CoreError, Result};
use crate::hash::Hash;

pub struct Writer {
    buf: Vec<u8>,
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGIC: &[u8] = b"TEST\x01";

    #[test]
    fn round_trips_every_value_type() {
        let h = Hash::of(b"x");
        let mut w = Writer::new(MAGIC);
        w.u8(7);
        w.u32(70_000);
        w.u64(u64::MAX);
        w.i64(-5);
        w.count(3);
        w.bytes(b"raw");
        w.str("文字");
        w.hash(&h);
        w.opt_str(Some("s"));
        w.opt_str(None);
        w.opt_hash(Some(&h));
        w.opt_hash(None);
        let bytes = w.finish();

        let mut r = Reader::new(&bytes, MAGIC).unwrap();
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u32().unwrap(), 70_000);
        assert_eq!(r.u64().unwrap(), u64::MAX);
        assert_eq!(r.i64().unwrap(), -5);
        assert_eq!(r.count().unwrap(), 3);
        assert_eq!(r.bytes().unwrap(), b"raw");
        assert_eq!(r.string().unwrap(), "文字");
        assert_eq!(r.hash().unwrap(), h);
        assert_eq!(r.opt_string().unwrap().as_deref(), Some("s"));
        assert_eq!(r.opt_string().unwrap(), None);
        assert_eq!(r.opt_hash().unwrap(), Some(h));
        assert_eq!(r.opt_hash().unwrap(), None);
        r.finish().unwrap();
    }

    #[test]
    fn rejects_bad_magic_truncation_and_trailing_bytes() {
        assert_eq!(Reader::new(b"NOPE", MAGIC).err(), Some(CoreError::Decode("bad magic")));

        let mut w = Writer::new(MAGIC);
        w.str("hello");
        let bytes = w.finish();
        let truncated = &bytes[..bytes.len() - 1];
        let mut r = Reader::new(truncated, MAGIC).unwrap();
        assert_eq!(r.string(), Err(CoreError::Decode("unexpected end of input")));

        let mut extra = bytes.clone();
        extra.push(0);
        let mut r = Reader::new(&extra, MAGIC).unwrap();
        r.string().unwrap();
        assert_eq!(r.finish(), Err(CoreError::Decode("trailing bytes")));
    }

    #[test]
    fn rejects_invalid_option_tag_and_utf8() {
        let mut w = Writer::new(MAGIC);
        w.u8(9);
        let bytes = w.finish();
        assert_eq!(Reader::new(&bytes, MAGIC).unwrap().opt_string(), Err(CoreError::Decode("bad option tag")));

        let mut w = Writer::new(MAGIC);
        w.bytes(&[0xff]);
        let bytes = w.finish();
        assert_eq!(Reader::new(&bytes, MAGIC).unwrap().string(), Err(CoreError::Decode("invalid utf-8")));
    }
}
```

Modify `crates/trellis-core/src/lib.rs`:

```rust
//! Core Trellis types: content hashes, repository paths, file kinds, and changesets.

pub mod codec;
mod error;
mod hash;
mod kind;
mod path;

pub use error::{CoreError, Result};
pub use hash::Hash;
pub use kind::FileKind;
pub use path::RepoPath;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-core codec`
Expected: compile error, `no function or associated item named 'new' found for struct 'Writer'`.

- [ ] **Step 3: Implement `Writer` and `Reader`**

Insert between the structs and the tests:

```rust
impl Writer {
    pub fn new(magic: &[u8]) -> Self {
        Self { buf: magic.to_vec() }
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// Writes a collection length or byte length as a u32.
    ///
    /// # Panics
    /// If `n` exceeds `u32::MAX`; callers bound sizes long before that.
    pub fn count(&mut self, n: usize) {
        self.u32(u32::try_from(n).expect("length exceeds u32::MAX"));
    }

    pub fn bytes(&mut self, v: &[u8]) {
        self.count(v.len());
        self.buf.extend_from_slice(v);
    }

    pub fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }

    pub fn hash(&mut self, v: &Hash) {
        self.buf.extend_from_slice(v.as_bytes());
    }

    pub fn opt_str(&mut self, v: Option<&str>) {
        match v {
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
            None => self.u8(0),
        }
    }

    pub fn opt_hash(&mut self, v: Option<&Hash>) {
        match v {
            Some(h) => {
                self.u8(1);
                self.hash(h);
            }
            None => self.u8(0),
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8], magic: &[u8]) -> Result<Self> {
        if !buf.starts_with(magic) {
            return Err(CoreError::Decode("bad magic"));
        }
        Ok(Self { buf, pos: magic.len() })
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(CoreError::Decode("length overflow"))?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(CoreError::Decode("unexpected end of input"))?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| CoreError::Decode("unexpected end of input"))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    pub fn count(&mut self) -> Result<usize> {
        Ok(self.u32()? as usize)
    }

    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let n = self.count()?;
        self.take(n)
    }

    pub fn string(&mut self) -> Result<String> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| CoreError::Decode("invalid utf-8"))
    }

    pub fn hash(&mut self) -> Result<Hash> {
        Ok(Hash::from_bytes(self.array()?))
    }

    pub fn opt_string(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(CoreError::Decode("bad option tag")),
        }
    }

    pub fn opt_hash(&mut self) -> Result<Option<Hash>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.hash()?)),
            _ => Err(CoreError::Decode("bad option tag")),
        }
    }

    /// Fails unless every byte was consumed, so each value has one encoding.
    pub fn finish(self) -> Result<()> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(CoreError::Decode("trailing bytes"))
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-core`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-core
git commit -m "feat(core): add canonical binary codec"
```

---

### Task 5: `Changeset`

**Files:**
- Create: `crates/trellis-core/src/changeset.rs`
- Modify: `crates/trellis-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

`crates/trellis-core/src/changeset.rs`:

```rust
use std::collections::BTreeMap;

use crate::codec::{Reader, Writer};
use crate::error::{CoreError, Result};
use crate::hash::Hash;
use crate::kind::FileKind;
use crate::path::RepoPath;

const MAGIC: &[u8] = b"TRLS-CS\x01";

/// A content-addressed blob and its length.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlobRef {
    pub hash: Hash,
    pub size: u64,
}

/// One file's change relative to the first parent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FileChange {
    Upsert { blob: BlobRef, kind: FileKind },
    Delete,
}

/// Who made a change and in which context (spec §2, from Libra traces).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Provenance {
    pub principal: String,
    pub session: Option<String>,
    pub task: Option<String>,
    pub model: Option<String>,
    pub trace: Option<Hash>,
}

/// A write-ahead-log entry: the files changed relative to the first parent, with
/// content-addressed pointers (spec §4.1, from Scion and Mononoke).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Changeset {
    pub parents: Vec<Hash>,
    pub changes: BTreeMap<RepoPath, FileChange>,
    pub provenance: Provenance,
    pub message: String,
    pub created_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn sample() -> Changeset {
        let mut changes = BTreeMap::new();
        changes.insert(
            RepoPath::parse("docs/a.md").unwrap(),
            FileChange::Upsert { blob: BlobRef::of(b"hello"), kind: FileKind::Markdown },
        );
        changes.insert(RepoPath::parse("old.txt").unwrap(), FileChange::Delete);
        Changeset {
            parents: vec![Hash::of(b"parent")],
            changes,
            provenance: Provenance {
                principal: "agent-7".into(),
                session: Some("s1".into()),
                task: Some("T-12".into()),
                model: Some("claude-opus-5".into()),
                trace: Some(Hash::of(b"trace")),
            },
            message: "edit docs".into(),
            created_at_ms: 1_760_000_000_000,
        }
    }

    #[test]
    fn round_trips_and_id_is_hash_of_encoding() {
        let cs = sample();
        let bytes = cs.encode();
        assert_eq!(Changeset::decode(&bytes).unwrap(), cs);
        assert_eq!(cs.id(), Hash::of(&bytes));
        assert_eq!(cs.first_parent(), Some(Hash::of(b"parent")));
    }

    #[test]
    fn id_depends_on_every_field() {
        let base = sample();
        let mut other = base.clone();
        other.message.push('!');
        assert_ne!(base.id(), other.id());
        let mut other = base.clone();
        other.provenance.session = None;
        assert_ne!(base.id(), other.id());
        assert_eq!(base.id(), sample().id());
    }

    #[test]
    fn rejects_unsorted_changes_and_trailing_bytes() {
        let mut w = Writer::new(MAGIC);
        w.count(0);
        w.count(2);
        w.str("b");
        w.u8(2);
        w.str("a");
        w.u8(2);
        w.str("p");
        w.opt_str(None);
        w.opt_str(None);
        w.opt_str(None);
        w.opt_hash(None);
        w.str("");
        w.i64(0);
        assert_eq!(
            Changeset::decode(&w.finish()),
            Err(CoreError::Decode("changes not strictly sorted"))
        );

        let mut bytes = sample().encode();
        bytes.push(0);
        assert_eq!(Changeset::decode(&bytes), Err(CoreError::Decode("trailing bytes")));
    }

    fn arb_change() -> impl Strategy<Value = FileChange> {
        prop_oneof![
            (any::<[u8; 32]>(), any::<u64>(), 1u8..=5).prop_map(|(h, size, k)| FileChange::Upsert {
                blob: BlobRef { hash: Hash::from_bytes(h), size },
                kind: FileKind::from_u8(k).unwrap(),
            }),
            Just(FileChange::Delete),
        ]
    }

    fn arb_changeset() -> impl Strategy<Value = Changeset> {
        (
            prop::collection::vec(any::<[u8; 32]>().prop_map(Hash::from_bytes), 0..3),
            prop::collection::btree_map(
                "[a-z]{1,3}(/[a-z]{1,3}){0,2}".prop_map(|s| RepoPath::parse(&s).unwrap()),
                arb_change(),
                0..8,
            ),
            ".{0,12}",
            proptest::option::of("[a-z]{0,6}"),
            any::<i64>(),
        )
            .prop_map(|(parents, changes, message, session, created_at_ms)| Changeset {
                parents,
                changes,
                provenance: Provenance { principal: "agent".into(), session, ..Default::default() },
                message,
                created_at_ms,
            })
    }

    proptest! {
        #[test]
        fn encode_decode_round_trips(cs in arb_changeset()) {
            let bytes = cs.encode();
            prop_assert_eq!(Changeset::decode(&bytes).unwrap(), cs.clone());
            prop_assert_eq!(Hash::of(&bytes), cs.id());
        }
    }
}
```

Modify `crates/trellis-core/src/lib.rs`:

```rust
//! Core Trellis types: content hashes, repository paths, file kinds, and changesets.

mod changeset;
pub mod codec;
mod error;
mod hash;
mod kind;
mod path;

pub use changeset::{BlobRef, Changeset, FileChange, Provenance};
pub use error::{CoreError, Result};
pub use hash::Hash;
pub use kind::FileKind;
pub use path::RepoPath;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-core changeset`
Expected: compile error, `no function or associated item named 'of' found for struct 'BlobRef'`.

- [ ] **Step 3: Implement encoding, decoding, and ids**

Insert between `Changeset` and the tests:

```rust
impl BlobRef {
    pub fn of(data: &[u8]) -> Self {
        Self { hash: Hash::of(data), size: data.len() as u64 }
    }
}

impl Changeset {
    pub fn first_parent(&self) -> Option<Hash> {
        self.parents.first().copied()
    }

    /// The changeset id: BLAKE3 of the canonical encoding.
    pub fn id(&self) -> Hash {
        Hash::of(&self.encode())
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(MAGIC);
        w.count(self.parents.len());
        for parent in &self.parents {
            w.hash(parent);
        }
        w.count(self.changes.len());
        for (path, change) in &self.changes {
            w.str(path.as_str());
            match change {
                FileChange::Upsert { blob, kind } => {
                    w.u8(1);
                    w.hash(&blob.hash);
                    w.u64(blob.size);
                    w.u8(kind.to_u8());
                }
                FileChange::Delete => w.u8(2),
            }
        }
        let p = &self.provenance;
        w.str(&p.principal);
        w.opt_str(p.session.as_deref());
        w.opt_str(p.task.as_deref());
        w.opt_str(p.model.as_deref());
        w.opt_hash(p.trace.as_ref());
        w.str(&self.message);
        w.i64(self.created_at_ms);
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, MAGIC)?;
        let parent_count = r.count()?;
        let mut parents = Vec::new();
        for _ in 0..parent_count {
            parents.push(r.hash()?);
        }
        let change_count = r.count()?;
        let mut changes = BTreeMap::new();
        let mut previous: Option<RepoPath> = None;
        for _ in 0..change_count {
            let path = RepoPath::parse(&r.string()?)?;
            if previous.as_ref().is_some_and(|prev| prev >= &path) {
                return Err(CoreError::Decode("changes not strictly sorted"));
            }
            let change = match r.u8()? {
                1 => {
                    let hash = r.hash()?;
                    let size = r.u64()?;
                    let kind = FileKind::from_u8(r.u8()?)?;
                    FileChange::Upsert { blob: BlobRef { hash, size }, kind }
                }
                2 => FileChange::Delete,
                _ => return Err(CoreError::Decode("unknown change tag")),
            };
            previous = Some(path.clone());
            changes.insert(path, change);
        }
        let provenance = Provenance {
            principal: r.string()?,
            session: r.opt_string()?,
            task: r.opt_string()?,
            model: r.opt_string()?,
            trace: r.opt_hash()?,
        };
        let message = r.string()?;
        let created_at_ms = r.i64()?;
        r.finish()?;
        Ok(Self { parents, changes, provenance, message, created_at_ms })
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-core`
Expected: `test result: ok. 14 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-core
git commit -m "feat(core): add Changeset with canonical encoding and content id"
```

---

### Task 6: `BlobStore` (filesystem and memory)

**Files:**
- Create: `crates/trellis-store/Cargo.toml`, `crates/trellis-store/src/lib.rs`, `crates/trellis-store/src/error.rs`, `crates/trellis-store/src/blob.rs`

- [ ] **Step 1: Create the crate and write the failing tests**

`crates/trellis-store/Cargo.toml`:

```toml
[package]
name = "trellis-store"
description = "Trellis storage: content-addressed blob stores and the metadata store (changesets, CAS refs, derived data)."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
thiserror = { workspace = true }
zstd = { workspace = true }
rusqlite = { workspace = true }
tempfile = { workspace = true }
```

`crates/trellis-store/src/lib.rs`:

```rust
//! Trellis storage: content-addressed blobs and the metadata store.

mod blob;
mod error;

pub use blob::{BlobStore, FsBlobStore, MemBlobStore};
pub use error::{Result, StoreError};
```

`crates/trellis-store/src/error.rs`:

```rust
use trellis_core::{CoreError, Hash};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("object {0} is corrupt")]
    Corrupt(Hash),
    #[error("store lock poisoned")]
    Poisoned,
}

pub type Result<T> = std::result::Result<T, StoreError>;
```

`crates/trellis-store/src/blob.rs`:

```rust
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use trellis_core::{BlobRef, Hash};

use crate::error::{Result, StoreError};

/// Immutable, content-addressed byte storage.
pub trait BlobStore: Send + Sync {
    /// Stores `data` (idempotent) and returns its reference.
    fn put(&self, data: &[u8]) -> Result<BlobRef>;
    /// Returns the bytes for `hash`, verified against it, or `None` if absent.
    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>>;
    fn contains(&self, hash: &Hash) -> Result<bool>;
}

/// Blobs as zstd-compressed files at `root/<first 2 hex>/<remaining 62 hex>`.
pub struct FsBlobStore {
    root: PathBuf,
}

/// Blobs in a hash map; for tests and simulation.
#[derive(Default)]
pub struct MemBlobStore {
    blobs: Mutex<HashMap<Hash, Vec<u8>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(store: &dyn BlobStore) {
        let a = store.put(b"hello").unwrap();
        assert_eq!(a, BlobRef::of(b"hello"));
        assert_eq!(store.put(b"hello").unwrap(), a);
        assert_eq!(store.get(&a.hash).unwrap().unwrap(), b"hello");
        assert!(store.contains(&a.hash).unwrap());

        let missing = Hash::of(b"missing");
        assert_eq!(store.get(&missing).unwrap(), None);
        assert!(!store.contains(&missing).unwrap());

        let empty = store.put(b"").unwrap();
        assert_eq!(store.get(&empty.hash).unwrap().unwrap(), b"");
    }

    #[test]
    fn fs_store_behaves() {
        let dir = tempfile::tempdir().unwrap();
        exercise(&FsBlobStore::open(dir.path()).unwrap());
    }

    #[test]
    fn mem_store_behaves() {
        exercise(&MemBlobStore::new());
    }

    #[test]
    fn fs_store_detects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path()).unwrap();
        let blob = store.put(b"good").unwrap();
        let (_, path) = store.locate(&blob.hash);
        std::fs::write(&path, zstd::encode_all(&b"evil"[..], 3).unwrap()).unwrap();
        assert!(matches!(store.get(&blob.hash), Err(StoreError::Corrupt(h)) if h == blob.hash));
    }

    #[test]
    fn fs_store_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let blob = FsBlobStore::open(dir.path()).unwrap().put(b"durable").unwrap();
        let reopened = FsBlobStore::open(dir.path()).unwrap();
        assert_eq!(reopened.get(&blob.hash).unwrap().unwrap(), b"durable");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-store`
Expected: compile error, `no function or associated item named 'open' found for struct 'FsBlobStore'`.

- [ ] **Step 3: Implement both stores**

Insert between the struct definitions and the tests:

```rust
impl FsBlobStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// The fan-out directory and file path for `hash`.
    fn locate(&self, hash: &Hash) -> (PathBuf, PathBuf) {
        let hex = hash.to_hex();
        let dir = self.root.join(&hex[..2]);
        let path = dir.join(&hex[2..]);
        (dir, path)
    }
}

impl BlobStore for FsBlobStore {
    fn put(&self, data: &[u8]) -> Result<BlobRef> {
        let blob = BlobRef::of(data);
        let (dir, path) = self.locate(&blob.hash);
        if path.exists() {
            return Ok(blob);
        }
        std::fs::create_dir_all(&dir)?;
        let compressed = zstd::encode_all(data, 3)?;
        let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
        tmp.write_all(&compressed)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&path).map_err(|e| StoreError::Io(e.error))?;
        Ok(blob)
    }

    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        let (_, path) = self.locate(hash);
        let compressed = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let data =
            zstd::decode_all(compressed.as_slice()).map_err(|_| StoreError::Corrupt(*hash))?;
        if Hash::of(&data) != *hash {
            return Err(StoreError::Corrupt(*hash));
        }
        Ok(Some(data))
    }

    fn contains(&self, hash: &Hash) -> Result<bool> {
        Ok(self.locate(hash).1.exists())
    }
}

impl MemBlobStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BlobStore for MemBlobStore {
    fn put(&self, data: &[u8]) -> Result<BlobRef> {
        let blob = BlobRef::of(data);
        self.blobs
            .lock()
            .map_err(|_| StoreError::Poisoned)?
            .entry(blob.hash)
            .or_insert_with(|| data.to_vec());
        Ok(blob)
    }

    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(self.blobs.lock().map_err(|_| StoreError::Poisoned)?.get(hash).cloned())
    }

    fn contains(&self, hash: &Hash) -> Result<bool> {
        Ok(self.blobs.lock().map_err(|_| StoreError::Poisoned)?.contains_key(hash))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-store`
Expected: `test result: ok. 4 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-store
git commit -m "feat(store): add filesystem and in-memory blob stores"
```

---

### Task 7: `MetaStore` on SQLite (changesets, CAS refs, derived data)

**Files:**
- Create: `crates/trellis-store/src/meta.rs`, `crates/trellis-store/src/sqlite.rs`
- Modify: `crates/trellis-store/src/lib.rs`

- [ ] **Step 1: Write the trait and the failing tests**

`crates/trellis-store/src/meta.rs`:

```rust
use trellis_core::{Changeset, Hash};

use crate::error::Result;

/// A ref's current target and how many times it has moved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RefState {
    pub target: Hash,
    pub version: u64,
}

/// Changesets, refs, and derived data for many repositories.
pub trait MetaStore: Send + Sync {
    /// Stores a changeset (idempotent) and returns its id.
    fn put_changeset(&self, repo: &str, cs: &Changeset) -> Result<Hash>;
    fn get_changeset(&self, repo: &str, id: &Hash) -> Result<Option<Changeset>>;
    fn get_ref(&self, repo: &str, name: &str) -> Result<Option<RefState>>;
    /// Atomically moves `name` from `expected` to `new`; `expected = None` means the ref must
    /// not exist yet. Returns `false`, changing nothing, if the current state differs.
    fn cas_ref(&self, repo: &str, name: &str, expected: Option<Hash>, new: Hash) -> Result<bool>;
    /// Stores a derived value; the first write for a key wins (values are deterministic).
    fn put_derived(&self, repo: &str, cs: &Hash, name: &str, version: u32, value: &[u8]) -> Result<()>;
    fn get_derived(&self, repo: &str, cs: &Hash, name: &str, version: u32) -> Result<Option<Vec<u8>>>;
}
```

`crates/trellis-store/src/sqlite.rs`:

```rust
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, params};
use trellis_core::{Changeset, Hash};

use crate::error::{Result, StoreError};
use crate::meta::{MetaStore, RefState};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS changesets (
    repo TEXT NOT NULL,
    id BLOB NOT NULL,
    body BLOB NOT NULL,
    PRIMARY KEY (repo, id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS refs (
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    target BLOB NOT NULL,
    version INTEGER NOT NULL,
    PRIMARY KEY (repo, name)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS derived (
    repo TEXT NOT NULL,
    cs BLOB NOT NULL,
    name TEXT NOT NULL,
    version INTEGER NOT NULL,
    value BLOB NOT NULL,
    PRIMARY KEY (repo, cs, name, version)
) WITHOUT ROWID;
";

/// A [`MetaStore`] on one SQLite database.
pub struct SqliteMetaStore {
    conn: Mutex<Connection>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use trellis_core::Provenance;

    use super::*;

    fn cs(message: &str, parents: Vec<Hash>) -> Changeset {
        Changeset {
            parents,
            changes: BTreeMap::new(),
            provenance: Provenance { principal: "t".into(), ..Default::default() },
            message: message.into(),
            created_at_ms: 0,
        }
    }

    #[test]
    fn changesets_round_trip_idempotently_per_repo() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        let c = cs("a", vec![]);
        let id = s.put_changeset("r", &c).unwrap();
        assert_eq!(id, c.id());
        assert_eq!(s.put_changeset("r", &c).unwrap(), id);
        assert_eq!(s.get_changeset("r", &id).unwrap(), Some(c));
        assert_eq!(s.get_changeset("other", &id).unwrap(), None);
    }

    #[test]
    fn cas_ref_only_moves_from_the_expected_target() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        let (a, b) = (Hash::of(b"a"), Hash::of(b"b"));
        assert_eq!(s.get_ref("r", "main").unwrap(), None);
        assert!(s.cas_ref("r", "main", None, a).unwrap());
        assert!(!s.cas_ref("r", "main", None, b).unwrap());
        assert_eq!(s.get_ref("r", "main").unwrap(), Some(RefState { target: a, version: 1 }));
        assert!(!s.cas_ref("r", "main", Some(b), b).unwrap());
        assert!(s.cas_ref("r", "main", Some(a), b).unwrap());
        assert_eq!(s.get_ref("r", "main").unwrap(), Some(RefState { target: b, version: 2 }));
    }

    #[test]
    fn concurrent_cas_has_exactly_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let s = Arc::new(SqliteMetaStore::open(dir.path().join("meta.db")).unwrap());
        let start = Hash::of(b"start");
        assert!(s.cas_ref("r", "main", None, start).unwrap());
        let handles: Vec<_> = (0..8u8)
            .map(|i| {
                let s = Arc::clone(&s);
                std::thread::spawn(move || s.cas_ref("r", "main", Some(start), Hash::of(&[i])).unwrap())
            })
            .collect();
        let winners = handles.into_iter().map(|h| h.join().unwrap()).filter(|won| *won).count();
        assert_eq!(winners, 1);
        assert_eq!(s.get_ref("r", "main").unwrap().unwrap().version, 2);
    }

    #[test]
    fn derived_values_are_keyed_by_name_and_version_and_first_write_wins() {
        let s = SqliteMetaStore::open_in_memory().unwrap();
        let id = Hash::of(b"cs");
        s.put_derived("r", &id, "tree", 1, b"v1").unwrap();
        s.put_derived("r", &id, "tree", 1, b"other").unwrap();
        assert_eq!(s.get_derived("r", &id, "tree", 1).unwrap().as_deref(), Some(&b"v1"[..]));
        assert_eq!(s.get_derived("r", &id, "tree", 2).unwrap(), None);
        assert_eq!(s.get_derived("r", &id, "path_set", 1).unwrap(), None);
    }

    #[test]
    fn file_store_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.db");
        let c = cs("durable", vec![]);
        {
            let s = SqliteMetaStore::open(&path).unwrap();
            let id = s.put_changeset("r", &c).unwrap();
            assert!(s.cas_ref("r", "main", None, id).unwrap());
        }
        let s = SqliteMetaStore::open(&path).unwrap();
        let target = s.get_ref("r", "main").unwrap().unwrap().target;
        assert_eq!(s.get_changeset("r", &target).unwrap(), Some(c));
    }
}
```

Modify `crates/trellis-store/src/lib.rs`:

```rust
//! Trellis storage: content-addressed blobs and the metadata store.

mod blob;
mod error;
mod meta;
mod sqlite;

pub use blob::{BlobStore, FsBlobStore, MemBlobStore};
pub use error::{Result, StoreError};
pub use meta::{MetaStore, RefState};
pub use sqlite::SqliteMetaStore;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-store sqlite`
Expected: compile error, `no function or associated item named 'open_in_memory'`.

- [ ] **Step 3: Implement `SqliteMetaStore`**

Insert between the struct and the tests:

```rust
impl SqliteMetaStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| StoreError::Poisoned)
    }
}

impl MetaStore for SqliteMetaStore {
    fn put_changeset(&self, repo: &str, cs: &Changeset) -> Result<Hash> {
        let body = cs.encode();
        let id = Hash::of(&body);
        self.conn()?.execute(
            "INSERT OR IGNORE INTO changesets (repo, id, body) VALUES (?1, ?2, ?3)",
            params![repo, id.as_bytes().as_slice(), body],
        )?;
        Ok(id)
    }

    fn get_changeset(&self, repo: &str, id: &Hash) -> Result<Option<Changeset>> {
        let body: Option<Vec<u8>> = self
            .conn()?
            .query_row(
                "SELECT body FROM changesets WHERE repo = ?1 AND id = ?2",
                params![repo, id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(body) = body else { return Ok(None) };
        if Hash::of(&body) != *id {
            return Err(StoreError::Corrupt(*id));
        }
        Ok(Some(Changeset::decode(&body)?))
    }

    fn get_ref(&self, repo: &str, name: &str) -> Result<Option<RefState>> {
        let row: Option<(Vec<u8>, i64)> = self
            .conn()?
            .query_row(
                "SELECT target, version FROM refs WHERE repo = ?1 AND name = ?2",
                params![repo, name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(target, version)| {
            Ok(RefState { target: Hash::from_slice(&target)?, version: version as u64 })
        })
        .transpose()
    }

    fn cas_ref(&self, repo: &str, name: &str, expected: Option<Hash>, new: Hash) -> Result<bool> {
        let conn = self.conn()?;
        let changed = match expected {
            None => conn.execute(
                "INSERT OR IGNORE INTO refs (repo, name, target, version) VALUES (?1, ?2, ?3, 1)",
                params![repo, name, new.as_bytes().as_slice()],
            )?,
            Some(old) => conn.execute(
                "UPDATE refs SET target = ?3, version = version + 1
                 WHERE repo = ?1 AND name = ?2 AND target = ?4",
                params![repo, name, new.as_bytes().as_slice(), old.as_bytes().as_slice()],
            )?,
        };
        Ok(changed == 1)
    }

    fn put_derived(&self, repo: &str, cs: &Hash, name: &str, version: u32, value: &[u8]) -> Result<()> {
        self.conn()?.execute(
            "INSERT OR IGNORE INTO derived (repo, cs, name, version, value) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![repo, cs.as_bytes().as_slice(), name, version, value],
        )?;
        Ok(())
    }

    fn get_derived(&self, repo: &str, cs: &Hash, name: &str, version: u32) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn()?
            .query_row(
                "SELECT value FROM derived WHERE repo = ?1 AND cs = ?2 AND name = ?3 AND version = ?4",
                params![repo, cs.as_bytes().as_slice(), name, version],
                |row| row.get(0),
            )
            .optional()?)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-store`
Expected: `test result: ok. 9 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-store
git commit -m "feat(store): add SQLite metadata store with single-row CAS refs"
```

---

### Task 8: Text merge and the `merge3` dispatcher

**Files:**
- Create: `crates/trellis-merge/Cargo.toml`, `crates/trellis-merge/src/lib.rs`, `crates/trellis-merge/src/text.rs`, `crates/trellis-merge/src/structured.rs` (stub in this task)

- [ ] **Step 1: Create the crate with failing tests**

`crates/trellis-merge/Cargo.toml`:

```toml
[package]
name = "trellis-merge"
description = "Trellis three-way merge drivers: line-based text and field-level JSON/YAML."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
diffy = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }

[dev-dependencies]
proptest = { workspace = true }
```

`crates/trellis-merge/src/lib.rs`:

```rust
//! Three-way merge drivers chosen by [`FileKind`] (spec §4.2, the compose route).

mod structured;
mod text;

use trellis_core::FileKind;

/// The outcome of a three-way merge.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MergeResult {
    /// The merged content; `None` means the file ends up deleted.
    Clean(Option<Vec<u8>>),
    Conflict(Conflict),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConflictReason {
    ModifyDelete,
    TextOverlap,
    FieldOverlap,
    Binary,
    Unparseable,
}

/// Why a merge could not complete, with human- and agent-readable detail
/// (conflict markers for text, field paths for structured files).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Conflict {
    pub reason: ConflictReason,
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "one\ntwo\nthree\nfour\nfive\n";

    fn text(base: Option<&str>, ours: Option<&str>, theirs: Option<&str>) -> MergeResult {
        merge3(
            FileKind::Text,
            base.map(str::as_bytes),
            ours.map(str::as_bytes),
            theirs.map(str::as_bytes),
        )
    }

    fn clean(s: &str) -> MergeResult {
        MergeResult::Clean(Some(s.as_bytes().to_vec()))
    }

    #[test]
    fn trivial_cases_take_the_changed_side() {
        assert_eq!(text(Some(BASE), Some("x\n"), Some(BASE)), clean("x\n"));
        assert_eq!(text(Some(BASE), Some(BASE), Some("y\n")), clean("y\n"));
        assert_eq!(text(Some(BASE), Some("z\n"), Some("z\n")), clean("z\n"));
        assert_eq!(text(Some(BASE), None, None), MergeResult::Clean(None));
        assert_eq!(text(Some(BASE), None, Some(BASE)), MergeResult::Clean(None));
        assert_eq!(text(None, None, Some("new\n")), clean("new\n"));
    }

    #[test]
    fn modify_delete_conflicts() {
        let MergeResult::Conflict(c) = text(Some(BASE), None, Some("changed\n")) else {
            panic!("expected a conflict");
        };
        assert_eq!(c.reason, ConflictReason::ModifyDelete);
    }

    #[test]
    fn disjoint_text_edits_merge() {
        let ours = "ONE\ntwo\nthree\nfour\nfive\n";
        let theirs = "one\ntwo\nthree\nfour\nFIVE\n";
        assert_eq!(text(Some(BASE), Some(ours), Some(theirs)), clean("ONE\ntwo\nthree\nfour\nFIVE\n"));
    }

    #[test]
    fn overlapping_text_edits_conflict_with_markers() {
        let ours = "one\ntwo\nTHREE-A\nfour\nfive\n";
        let theirs = "one\ntwo\nTHREE-B\nfour\nfive\n";
        let MergeResult::Conflict(c) = text(Some(BASE), Some(ours), Some(theirs)) else {
            panic!("expected a conflict");
        };
        assert_eq!(c.reason, ConflictReason::TextOverlap);
        assert!(c.detail.contains("<<<<<<<"), "{}", c.detail);
    }

    #[test]
    fn binary_and_invalid_utf8_conflict() {
        let r = merge3(FileKind::Binary, Some(b"a"), Some(b"b"), Some(b"c"));
        assert!(matches!(r, MergeResult::Conflict(Conflict { reason: ConflictReason::Binary, .. })));
        let r = merge3(FileKind::Text, Some(b"a"), Some(&[0xff]), Some(b"c"));
        assert!(matches!(r, MergeResult::Conflict(Conflict { reason: ConflictReason::Unparseable, .. })));
    }
}
```

`crates/trellis-merge/src/text.rs`:

```rust
use crate::{ConflictReason, MergeResult};
```

`crates/trellis-merge/src/structured.rs` (stub, replaced in Task 9):

```rust
use crate::{ConflictReason, MergeResult};

pub(crate) fn merge_json(_base: Option<&[u8]>, _ours: &[u8], _theirs: &[u8]) -> MergeResult {
    MergeResult::conflict(ConflictReason::Unparseable, "structured merge not implemented")
}

pub(crate) fn merge_yaml(_base: Option<&[u8]>, _ours: &[u8], _theirs: &[u8]) -> MergeResult {
    MergeResult::conflict(ConflictReason::Unparseable, "structured merge not implemented")
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-merge`
Expected: compile error, `cannot find function 'merge3'`.

- [ ] **Step 3: Implement `merge3` and the text driver**

Add to `crates/trellis-merge/src/lib.rs`, before `#[cfg(test)]`:

```rust
impl MergeResult {
    pub(crate) fn conflict(reason: ConflictReason, detail: impl Into<String>) -> Self {
        Self::Conflict(Conflict { reason, detail: detail.into() })
    }
}

/// Merges `ours` and `theirs` against their common `base`; `None` means the file is absent.
pub fn merge3(
    kind: FileKind,
    base: Option<&[u8]>,
    ours: Option<&[u8]>,
    theirs: Option<&[u8]>,
) -> MergeResult {
    if ours == theirs || base == theirs {
        return MergeResult::Clean(ours.map(<[u8]>::to_vec));
    }
    if base == ours {
        return MergeResult::Clean(theirs.map(<[u8]>::to_vec));
    }
    let (Some(ours), Some(theirs)) = (ours, theirs) else {
        return MergeResult::conflict(
            ConflictReason::ModifyDelete,
            "one side deleted the file and the other changed it",
        );
    };
    match kind {
        FileKind::Text | FileKind::Markdown => text::merge(base.unwrap_or_default(), ours, theirs),
        FileKind::Json => structured::merge_json(base, ours, theirs),
        FileKind::Yaml => structured::merge_yaml(base, ours, theirs),
        FileKind::Binary => {
            MergeResult::conflict(ConflictReason::Binary, "both sides changed a binary file")
        }
    }
}
```

Replace `crates/trellis-merge/src/text.rs` with:

```rust
use crate::{ConflictReason, MergeResult};

/// Line-based three-way merge; overlapping edits produce conflict markers.
pub(crate) fn merge(base: &[u8], ours: &[u8], theirs: &[u8]) -> MergeResult {
    let (Ok(base), Ok(ours), Ok(theirs)) = (
        std::str::from_utf8(base),
        std::str::from_utf8(ours),
        std::str::from_utf8(theirs),
    ) else {
        return MergeResult::conflict(ConflictReason::Unparseable, "text is not valid UTF-8");
    };
    match diffy::merge(base, ours, theirs) {
        Ok(merged) => MergeResult::Clean(Some(merged.into_bytes())),
        Err(with_markers) => MergeResult::conflict(ConflictReason::TextOverlap, with_markers),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-merge`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-merge
git commit -m "feat(merge): add merge3 dispatcher and line-based text merge"
```

---

### Task 9: Structured JSON/YAML merge

**Files:**
- Modify: `crates/trellis-merge/src/structured.rs`

- [ ] **Step 1: Write the failing tests**

Replace `crates/trellis-merge/src/structured.rs` with the stub functions kept and tests added:

```rust
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::{ConflictReason, MergeResult};

pub(crate) fn merge_json(_base: Option<&[u8]>, _ours: &[u8], _theirs: &[u8]) -> MergeResult {
    MergeResult::conflict(ConflictReason::Unparseable, "structured merge not implemented")
}

pub(crate) fn merge_yaml(_base: Option<&[u8]>, _ours: &[u8], _theirs: &[u8]) -> MergeResult {
    MergeResult::conflict(ConflictReason::Unparseable, "structured merge not implemented")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;
    use trellis_core::FileKind;

    use crate::{Conflict, merge3};

    use super::*;

    fn run(base: Option<Value>, ours: Value, theirs: Value) -> MergeResult {
        let enc = |v: &Value| serde_json::to_vec(v).unwrap();
        merge3(
            FileKind::Json,
            base.as_ref().map(enc).as_deref(),
            Some(enc(&ours).as_slice()),
            Some(enc(&theirs).as_slice()),
        )
    }

    fn clean_value(r: &MergeResult) -> Value {
        match r {
            MergeResult::Clean(Some(bytes)) => serde_json::from_slice(bytes).unwrap(),
            other => panic!("expected a clean merge, got {other:?}"),
        }
    }

    fn conflict_detail(r: &MergeResult) -> String {
        match r {
            MergeResult::Conflict(Conflict { reason: ConflictReason::FieldOverlap, detail }) => detail.clone(),
            other => panic!("expected a field conflict, got {other:?}"),
        }
    }

    #[test]
    fn disjoint_fields_merge() {
        let r = run(Some(json!({"a": 1, "b": 1})), json!({"a": 2, "b": 1}), json!({"a": 1, "b": 2}));
        assert_eq!(clean_value(&r), json!({"a": 2, "b": 2}));
    }

    #[test]
    fn nested_disjoint_fields_merge() {
        let base = json!({"alice": {"hp": 3, "place": "inn"}});
        let r = run(Some(base), json!({"alice": {"hp": 2, "place": "inn"}}), json!({"alice": {"hp": 3, "place": "road"}}));
        assert_eq!(clean_value(&r), json!({"alice": {"hp": 2, "place": "road"}}));
    }

    #[test]
    fn same_field_changed_differently_conflicts_with_its_path() {
        let r = run(Some(json!({"a": {"b": 1}})), json!({"a": {"b": 2}}), json!({"a": {"b": 3}}));
        assert!(conflict_detail(&r).contains("/a/b"));
    }

    #[test]
    fn modify_delete_inside_a_document_conflicts() {
        let r = run(Some(json!({"a": 1, "b": 1})), json!({"b": 1}), json!({"a": 5, "b": 1}));
        assert!(conflict_detail(&r).contains("/a (modified on one side, deleted on the other)"));
    }

    #[test]
    fn arrays_of_objects_with_ids_merge_by_id() {
        let base = json!({"chars": [{"id": "alice", "hp": 3}, {"id": "bob", "hp": 3}]});
        let ours = json!({"chars": [{"id": "alice", "hp": 2}, {"id": "bob", "hp": 3}]});
        let theirs = json!({"chars": [{"id": "alice", "hp": 3}, {"id": "bob", "hp": 1}, {"id": "carol", "hp": 5}]});
        let r = run(Some(base), ours, theirs);
        assert_eq!(
            clean_value(&r),
            json!({"chars": [{"id": "alice", "hp": 2}, {"id": "bob", "hp": 1}, {"id": "carol", "hp": 5}]})
        );
    }

    #[test]
    fn deleting_an_id_item_on_one_side_merges() {
        let base = json!([{"id": "a"}, {"id": "b"}]);
        let r = run(Some(base), json!([{"id": "a"}]), json!([{"id": "a"}, {"id": "b"}, {"id": "c"}]));
        assert_eq!(clean_value(&r), json!([{"id": "a"}, {"id": "c"}]));
    }

    #[test]
    fn plain_arrays_changed_on_both_sides_conflict() {
        let r = run(Some(json!({"tags": [1]})), json!({"tags": [1, 2]}), json!({"tags": [1, 3]}));
        assert!(conflict_detail(&r).contains("/tags"));
    }

    #[test]
    fn both_added_documents_merge_disjoint_keys() {
        let r = run(None, json!({"a": 1}), json!({"b": 2}));
        assert_eq!(clean_value(&r), json!({"a": 1, "b": 2}));
    }

    #[test]
    fn unparseable_json_conflicts() {
        let r = merge3(FileKind::Json, Some(b"{}"), Some(b"{"), Some(b"{\"a\":1}"));
        assert!(matches!(r, MergeResult::Conflict(Conflict { reason: ConflictReason::Unparseable, .. })));
    }

    #[test]
    fn yaml_merges_by_field() {
        let r = merge3(FileKind::Yaml, Some(b"a: 1\nb: 1\n"), Some(b"a: 2\nb: 1\n"), Some(b"a: 1\nb: 3\n"));
        let MergeResult::Clean(Some(bytes)) = r else { panic!("expected a clean merge, got {r:?}") };
        let merged: Value = serde_yaml::from_slice(&bytes).unwrap();
        assert_eq!(merged, json!({"a": 2, "b": 3}));
    }

    proptest! {
        #[test]
        fn edits_to_disjoint_keys_always_merge(
            base in prop::collection::btree_map("[a-h]", 0i64..100, 1..8),
            ours_new in 0i64..100,
            theirs_new in 0i64..100,
        ) {
            let keys: Vec<String> = base.keys().cloned().collect();
            let mut ours = base.clone();
            let mut theirs = base.clone();
            ours.insert(keys[0].clone(), ours_new);
            theirs.insert("z".to_owned(), theirs_new);
            let r = run(Some(json!(base)), json!(ours), json!(theirs));
            let mut expected = ours.clone();
            expected.insert("z".to_owned(), theirs_new);
            prop_assert_eq!(clean_value(&r), json!(expected));
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-merge structured`
Expected: FAIL; `disjoint_fields_merge` panics with `expected a clean merge, got Conflict(... Unparseable ...)`.

- [ ] **Step 3: Implement the structured merge**

Replace the two stub functions with:

```rust
/// Field-level merge of JSON documents; output is pretty-printed with sorted keys.
pub(crate) fn merge_json(base: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> MergeResult {
    merge_with(
        base,
        ours,
        theirs,
        |bytes| serde_json::from_slice(bytes).ok(),
        |value| {
            let mut out = serde_json::to_vec_pretty(value).ok()?;
            out.push(b'\n');
            Some(out)
        },
    )
}

/// Field-level merge of YAML documents; output is re-serialized YAML.
pub(crate) fn merge_yaml(base: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> MergeResult {
    merge_with(
        base,
        ours,
        theirs,
        |bytes| serde_yaml::from_slice(bytes).ok(),
        |value| serde_yaml::to_string(value).ok().map(String::into_bytes),
    )
}

fn merge_with(
    base: Option<&[u8]>,
    ours: &[u8],
    theirs: &[u8],
    parse: impl Fn(&[u8]) -> Option<Value>,
    render: impl Fn(&Value) -> Option<Vec<u8>>,
) -> MergeResult {
    let unparseable =
        || MergeResult::conflict(ConflictReason::Unparseable, "document could not be parsed");
    let (Some(ours), Some(theirs)) = (parse(ours), parse(theirs)) else {
        return unparseable();
    };
    let base = match base {
        Some(bytes) => match parse(bytes) {
            Some(value) => value,
            None => return unparseable(),
        },
        None => Value::Null,
    };
    let mut conflicts = Vec::new();
    let merged = merge_value(&base, &ours, &theirs, "", &mut conflicts);
    if !conflicts.is_empty() {
        return MergeResult::conflict(ConflictReason::FieldOverlap, conflicts.join("\n"));
    }
    match render(&merged) {
        Some(bytes) => MergeResult::Clean(Some(bytes)),
        None => unparseable(),
    }
}

fn display_path(path: &str) -> String {
    if path.is_empty() { "/".to_owned() } else { path.to_owned() }
}

fn merge_value(
    base: &Value,
    ours: &Value,
    theirs: &Value,
    path: &str,
    conflicts: &mut Vec<String>,
) -> Value {
    if ours == theirs || base == theirs {
        return ours.clone();
    }
    if base == ours {
        return theirs.clone();
    }
    match (ours, theirs) {
        (Value::Object(o), Value::Object(t)) => {
            let empty = Map::new();
            let b = base.as_object().unwrap_or(&empty);
            let keys: BTreeSet<&String> = b.keys().chain(o.keys()).chain(t.keys()).collect();
            let mut out = Map::new();
            for key in keys {
                let child = format!("{path}/{key}");
                if let Some(v) = merge_entry(b.get(key), o.get(key), t.get(key), &child, conflicts) {
                    out.insert(key.clone(), v);
                }
            }
            Value::Object(out)
        }
        (Value::Array(o), Value::Array(t))
            if is_keyed(o) && is_keyed(t) && base.as_array().is_none_or(|b| is_keyed(b)) =>
        {
            let b = base.as_array().map(Vec::as_slice).unwrap_or_default();
            Value::Array(merge_keyed(b, o, t, path, conflicts))
        }
        _ => {
            conflicts.push(display_path(path));
            ours.clone()
        }
    }
}

fn merge_entry(
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
    path: &str,
    conflicts: &mut Vec<String>,
) -> Option<Value> {
    if ours == theirs || base == theirs {
        return ours.cloned();
    }
    if base == ours {
        return theirs.cloned();
    }
    match (ours, theirs) {
        (Some(o), Some(t)) => Some(merge_value(base.unwrap_or(&Value::Null), o, t, path, conflicts)),
        _ => {
            conflicts.push(format!(
                "{} (modified on one side, deleted on the other)",
                display_path(path)
            ));
            ours.cloned()
        }
    }
}

fn item_id(value: &Value) -> Option<&str> {
    value.get("id")?.as_str()
}

/// Every element is an object with a unique string `id`.
fn is_keyed(items: &[Value]) -> bool {
    let mut seen = BTreeSet::new();
    items.iter().all(|v| item_id(v).is_some_and(|id| seen.insert(id)))
}

fn by_id(items: &[Value]) -> BTreeMap<&str, &Value> {
    items.iter().filter_map(|v| Some((item_id(v)?, v))).collect()
}

/// Merges arrays item by item, matched by `id`: ours' order first, then items only theirs added.
fn merge_keyed(
    base: &[Value],
    ours: &[Value],
    theirs: &[Value],
    path: &str,
    conflicts: &mut Vec<String>,
) -> Vec<Value> {
    let (b, o, t) = (by_id(base), by_id(ours), by_id(theirs));
    let mut order: Vec<&str> = ours.iter().filter_map(item_id).collect();
    order.extend(theirs.iter().filter_map(item_id).filter(|id| !o.contains_key(id)));
    order
        .into_iter()
        .filter_map(|id| {
            let child = format!("{path}[id={id}]");
            merge_entry(b.get(id).copied(), o.get(id).copied(), t.get(id).copied(), &child, conflicts)
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-merge`
Expected: `test result: ok. 16 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-merge
git commit -m "feat(merge): add field-level JSON/YAML merge with id-keyed arrays"
```

---

### Task 10: Derived-data framework and `PathSet`

**Files:**
- Create: `crates/trellis-derive/Cargo.toml`, `crates/trellis-derive/src/lib.rs`, `crates/trellis-derive/src/error.rs`, `crates/trellis-derive/src/framework.rs`, `crates/trellis-derive/src/pathset.rs`, `crates/trellis-derive/src/testutil.rs`

- [ ] **Step 1: Create the crate, the test fixture, and failing tests**

`crates/trellis-derive/Cargo.toml`:

```toml
[package]
name = "trellis-derive"
description = "Trellis derived data: indexes computed recursively from changesets and cached by (changeset, name, version)."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
trellis-core = { workspace = true }
trellis-store = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
proptest = { workspace = true }
```

`crates/trellis-derive/src/lib.rs`:

```rust
//! Derived data (spec §4.8): indexes computed recursively from changesets and cached under
//! `(repo, changeset, name, version)`, so a new version can be re-derived side by side.

mod error;
mod framework;
mod pathset;
#[cfg(test)]
mod testutil;

pub use error::{DeriveError, Result};
pub use framework::{Derivable, DeriveCtx};
pub use pathset::PathSet;
```

`crates/trellis-derive/src/error.rs`:

```rust
use trellis_core::{CoreError, Hash};
use trellis_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum DeriveError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("changeset {0} not found")]
    MissingChangeset(Hash),
    #[error("object {0} not found")]
    MissingObject(Hash),
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
}

pub type Result<T> = std::result::Result<T, DeriveError>;
```

`crates/trellis-derive/src/framework.rs`:

```rust
use std::collections::HashSet;

use trellis_core::{Changeset, Hash};
use trellis_store::{BlobStore, MetaStore};

use crate::error::{DeriveError, Result};

/// A value computed from a changeset and, recursively, from its parents' values.
pub trait Derivable: Sized {
    /// Storage name; with `VERSION`, part of the cache key.
    const NAME: &'static str;
    /// Bump to re-derive with a new format; old rows stay until pruned.
    const VERSION: u32;
    /// Computes the value for `cs`. Every parent's value is already stored when this runs,
    /// so `ctx.get::<Self>(parent)` is a cache hit.
    fn derive(ctx: &DeriveCtx<'_>, id: &Hash, cs: &Changeset) -> Result<Self>;
    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Result<Self>;
}

/// Access to one repository's changesets, blobs, and derived values.
pub struct DeriveCtx<'a> {
    pub repo: &'a str,
    pub meta: &'a dyn MetaStore,
    pub blobs: &'a dyn BlobStore,
}
```

`crates/trellis-derive/src/pathset.rs`:

```rust
use std::collections::BTreeSet;

use trellis_core::codec::{Reader, Writer};
use trellis_core::{Changeset, Hash, RepoPath};

use crate::error::Result;
use crate::framework::{Derivable, DeriveCtx};

const MAGIC: &[u8] = b"TRLS-PS\x01";

/// The paths one changeset changes (relative to its first parent).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PathSet(pub BTreeSet<RepoPath>);

#[cfg(test)]
mod tests {
    use crate::testutil::Fixture;

    use super::*;

    #[test]
    fn path_set_lists_changed_paths_and_is_cached() {
        let fx = Fixture::new();
        let c1 = fx.commit(None, &[("a.txt", Some("1")), ("d/b.md", Some("2"))]);
        let c2 = fx.commit(Some(c1), &[("a.txt", None)]);
        let ctx = fx.ctx();

        let ps: PathSet = ctx.get(&c2).unwrap();
        assert_eq!(ps.0.iter().map(RepoPath::as_str).collect::<Vec<_>>(), ["a.txt"]);

        // Deriving c2 derived its ancestor c1 too, and stored both.
        let stored = fx.meta_get_derived(&c1, PathSet::NAME, PathSet::VERSION).unwrap();
        let ps1 = PathSet::decode(&stored).unwrap();
        assert_eq!(ps1.0.iter().map(RepoPath::as_str).collect::<Vec<_>>(), ["a.txt", "d/b.md"]);
        assert_eq!(ctx.get::<PathSet>(&c1).unwrap(), ps1);
    }

    #[test]
    fn missing_changeset_is_an_error() {
        let fx = Fixture::new();
        let err = fx.ctx().get::<PathSet>(&Hash::of(b"nope")).unwrap_err();
        assert!(matches!(err, crate::DeriveError::MissingChangeset(_)));
    }

    #[test]
    fn deep_chains_derive_without_recursion() {
        let fx = Fixture::new();
        let ids = fx.chain(None, 3000);
        let ps: PathSet = fx.ctx().get(ids.last().unwrap()).unwrap();
        assert_eq!(ps.0.len(), 1);
    }
}
```

`crates/trellis-derive/src/testutil.rs`:

```rust
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use trellis_core::{Changeset, FileChange, FileKind, Hash, Provenance, RepoPath};
use trellis_store::{BlobStore, MemBlobStore, MetaStore, SqliteMetaStore};

use crate::framework::DeriveCtx;

pub(crate) const REPO: &str = "repo";

/// An in-memory repository for derive tests.
pub(crate) struct Fixture {
    pub meta: SqliteMetaStore,
    pub blobs: MemBlobStore,
    /// Makes every `chain` call write distinct contents, so independent chains never share
    /// changeset ids.
    next_chain: AtomicUsize,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            meta: SqliteMetaStore::open_in_memory().unwrap(),
            blobs: MemBlobStore::new(),
            next_chain: AtomicUsize::new(0),
        }
    }

    pub fn ctx(&self) -> DeriveCtx<'_> {
        DeriveCtx::new(REPO, &self.meta, &self.blobs)
    }

    pub fn meta_get_derived(&self, id: &Hash, name: &str, version: u32) -> Option<Vec<u8>> {
        self.meta.get_derived(REPO, id, name, version).unwrap()
    }

    /// Stores a changeset on top of `parent`; each edit is `(path, Some(content))` or
    /// `(path, None)` for a delete.
    pub fn commit(&self, parent: Option<Hash>, edits: &[(&str, Option<&str>)]) -> Hash {
        let mut changes = BTreeMap::new();
        for (path, content) in edits {
            let path = RepoPath::parse(path).unwrap();
            let change = match content {
                Some(text) => FileChange::Upsert {
                    blob: self.blobs.put(text.as_bytes()).unwrap(),
                    kind: FileKind::infer(&path, text.as_bytes()),
                },
                None => FileChange::Delete,
            };
            changes.insert(path, change);
        }
        let cs = Changeset {
            parents: parent.into_iter().collect(),
            changes,
            provenance: Provenance { principal: "test".into(), ..Default::default() },
            message: String::new(),
            created_at_ms: 0,
        };
        self.meta.put_changeset(REPO, &cs).unwrap()
    }

    /// A linear chain of `n` changesets on top of `parent`, each rewriting `f.txt` with
    /// contents unique to this chain.
    pub fn chain(&self, parent: Option<Hash>, n: usize) -> Vec<Hash> {
        let chain = self.next_chain.fetch_add(1, Ordering::Relaxed);
        let mut ids = Vec::with_capacity(n);
        let mut tip = parent;
        for i in 0..n {
            let content = format!("chain {chain} step {i}");
            let id = self.commit(tip, &[("f.txt", Some(content.as_str()))]);
            ids.push(id);
            tip = Some(id);
        }
        ids
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-derive`
Expected: compile error, `no function or associated item named 'new' found for struct 'DeriveCtx'`.

- [ ] **Step 3: Implement `DeriveCtx` and `PathSet`**

Append to `crates/trellis-derive/src/framework.rs`:

```rust
impl<'a> DeriveCtx<'a> {
    pub fn new(repo: &'a str, meta: &'a dyn MetaStore, blobs: &'a dyn BlobStore) -> Self {
        Self { repo, meta, blobs }
    }

    pub fn changeset(&self, id: &Hash) -> Result<Changeset> {
        self.meta.get_changeset(self.repo, id)?.ok_or(DeriveError::MissingChangeset(*id))
    }

    fn stored<D: Derivable>(&self, id: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(self.meta.get_derived(self.repo, id, D::NAME, D::VERSION)?)
    }

    /// Returns `D` for `id`, first deriving it for every ancestor that lacks it, parents
    /// before children, with an explicit stack (no recursion, so long histories are fine).
    pub fn get<D: Derivable>(&self, id: &Hash) -> Result<D> {
        if let Some(bytes) = self.stored::<D>(id)? {
            return D::decode(&bytes);
        }
        let mut stack: Vec<(Hash, bool)> = vec![(*id, false)];
        let mut done: HashSet<Hash> = HashSet::new();
        while let Some((current, parents_ready)) = stack.pop() {
            if done.contains(&current) {
                continue;
            }
            if self.stored::<D>(&current)?.is_some() {
                done.insert(current);
                continue;
            }
            let cs = self.changeset(&current)?;
            if parents_ready {
                let value = D::derive(self, &current, &cs)?;
                self.meta.put_derived(self.repo, &current, D::NAME, D::VERSION, &value.encode())?;
                done.insert(current);
            } else {
                stack.push((current, true));
                for parent in &cs.parents {
                    if !done.contains(parent) {
                        stack.push((*parent, false));
                    }
                }
            }
        }
        let bytes = self.stored::<D>(id)?.ok_or(DeriveError::MissingChangeset(*id))?;
        D::decode(&bytes)
    }
}
```

Insert into `crates/trellis-derive/src/pathset.rs` between the struct and the tests:

```rust
impl Derivable for PathSet {
    const NAME: &'static str = "path_set";
    const VERSION: u32 = 1;

    fn derive(_ctx: &DeriveCtx<'_>, _id: &Hash, cs: &Changeset) -> Result<Self> {
        Ok(Self(cs.changes.keys().cloned().collect()))
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(MAGIC);
        w.count(self.0.len());
        for path in &self.0 {
            w.str(path.as_str());
        }
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, MAGIC)?;
        let n = r.count()?;
        let mut paths = BTreeSet::new();
        for _ in 0..n {
            paths.insert(RepoPath::parse(&r.string()?)?);
        }
        r.finish()?;
        Ok(Self(paths))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-derive`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-derive
git commit -m "feat(derive): add derived-data framework and PathSet"
```

---

### Task 11: `SkipList` and first-parent ancestry

**Files:**
- Create: `crates/trellis-derive/src/skiplist.rs`
- Modify: `crates/trellis-derive/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

`crates/trellis-derive/src/skiplist.rs`:

```rust
use trellis_core::codec::{Reader, Writer};
use trellis_core::{Changeset, Hash};

use crate::error::{DeriveError, Result};
use crate::framework::{Derivable, DeriveCtx};

const MAGIC: &[u8] = b"TRLS-SL\x01";

/// First-parent depth and ancestor jumps: `skips[i]` is the first-parent ancestor `2^i`
/// steps back (spec §4.8, "skip lists for the commit graph" in Scion).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SkipList {
    pub generation: u64,
    pub skips: Vec<Hash>,
}

#[cfg(test)]
mod tests {
    use crate::testutil::Fixture;

    use super::*;

    #[test]
    fn generations_and_ancestors_match_a_naive_walk() {
        let fx = Fixture::new();
        let ids = fx.chain(None, 130);
        let ctx = fx.ctx();
        for (i, id) in ids.iter().enumerate() {
            let sl: SkipList = ctx.get(id).unwrap();
            assert_eq!(sl.generation, i as u64);
        }
        let tip = ids.last().unwrap();
        for (g, expected) in ids.iter().enumerate() {
            assert_eq!(ancestor_at(&ctx, tip, g as u64).unwrap(), *expected, "generation {g}");
        }
        assert!(ancestor_at(&ctx, &ids[3], 4).is_err());
    }

    #[test]
    fn ancestry_checks() {
        let fx = Fixture::new();
        let ids = fx.chain(None, 40);
        let ctx = fx.ctx();
        assert!(is_first_parent_ancestor(&ctx, &ids[5], &ids[39]).unwrap());
        assert!(is_first_parent_ancestor(&ctx, &ids[39], &ids[39]).unwrap());
        assert!(!is_first_parent_ancestor(&ctx, &ids[39], &ids[5]).unwrap());
        let other = fx.chain(None, 3);
        assert!(!is_first_parent_ancestor(&ctx, &other[0], &ids[39]).unwrap());
    }

    #[test]
    fn merge_base_of_forked_chains() {
        let fx = Fixture::new();
        let trunk = fx.chain(None, 50);
        let fork_point = trunk[20];
        let left = fx.commit(Some(trunk[49]), &[("left.txt", Some("l"))]);
        let mut right_tip = fork_point;
        for i in 0..37 {
            let content = format!("r{i}");
            right_tip = fx.commit(Some(right_tip), &[("right.txt", Some(content.as_str()))]);
        }
        let ctx = fx.ctx();
        assert_eq!(first_parent_merge_base(&ctx, &left, &right_tip).unwrap(), Some(fork_point));
        assert_eq!(first_parent_merge_base(&ctx, &right_tip, &left).unwrap(), Some(fork_point));
        assert_eq!(first_parent_merge_base(&ctx, &left, &trunk[10]).unwrap(), Some(trunk[10]));
        assert_eq!(first_parent_merge_base(&ctx, &left, &left).unwrap(), Some(left));
        let unrelated = fx.chain(None, 5);
        assert_eq!(first_parent_merge_base(&ctx, &left, &unrelated[4]).unwrap(), None);
    }
}
```

Modify `crates/trellis-derive/src/lib.rs`:

```rust
//! Derived data (spec §4.8): indexes computed recursively from changesets and cached under
//! `(repo, changeset, name, version)`, so a new version can be re-derived side by side.

mod error;
mod framework;
mod pathset;
mod skiplist;
#[cfg(test)]
mod testutil;

pub use error::{DeriveError, Result};
pub use framework::{Derivable, DeriveCtx};
pub use pathset::PathSet;
pub use skiplist::{SkipList, ancestor_at, first_parent_merge_base, is_first_parent_ancestor};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-derive skiplist`
Expected: compile error, `the trait bound 'SkipList: Derivable' is not satisfied`.

- [ ] **Step 3: Implement `SkipList` and the ancestry functions**

Insert between the struct and the tests:

```rust
impl Derivable for SkipList {
    const NAME: &'static str = "skip_list";
    const VERSION: u32 = 1;

    fn derive(ctx: &DeriveCtx<'_>, _id: &Hash, cs: &Changeset) -> Result<Self> {
        let Some(parent) = cs.first_parent() else {
            return Ok(Self { generation: 0, skips: Vec::new() });
        };
        let parent_list: SkipList = ctx.get(&parent)?;
        let mut skips = vec![parent];
        loop {
            let level = skips.len() - 1;
            let ancestor: SkipList = ctx.get(&skips[level])?;
            match ancestor.skips.get(level) {
                Some(next) => skips.push(*next),
                None => break,
            }
        }
        Ok(Self { generation: parent_list.generation + 1, skips })
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(MAGIC);
        w.u64(self.generation);
        w.count(self.skips.len());
        for h in &self.skips {
            w.hash(h);
        }
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, MAGIC)?;
        let generation = r.u64()?;
        let n = r.count()?;
        let mut skips = Vec::new();
        for _ in 0..n {
            skips.push(r.hash()?);
        }
        r.finish()?;
        Ok(Self { generation, skips })
    }
}

/// The first-parent ancestor of `id` at `generation`, in O(log distance) jumps.
pub fn ancestor_at(ctx: &DeriveCtx<'_>, id: &Hash, generation: u64) -> Result<Hash> {
    let mut current = *id;
    let mut list: SkipList = ctx.get(&current)?;
    if generation > list.generation {
        return Err(DeriveError::InvalidArgument("generation is above the starting changeset"));
    }
    while list.generation > generation {
        let distance = list.generation - generation;
        let level = (u64::BITS - 1 - distance.leading_zeros()) as usize;
        current = *list
            .skips
            .get(level)
            .ok_or(DeriveError::InvalidArgument("skip list ended early"))?;
        list = ctx.get(&current)?;
    }
    Ok(current)
}

/// Whether `ancestor` is `descendant` or one of its first-parent ancestors.
pub fn is_first_parent_ancestor(ctx: &DeriveCtx<'_>, ancestor: &Hash, descendant: &Hash) -> Result<bool> {
    let generation = ctx.get::<SkipList>(ancestor)?.generation;
    if generation > ctx.get::<SkipList>(descendant)?.generation {
        return Ok(false);
    }
    Ok(ancestor_at(ctx, descendant, generation)? == *ancestor)
}

/// The nearest common first-parent ancestor of `a` and `b`, or `None` if their histories
/// share no root.
pub fn first_parent_merge_base(ctx: &DeriveCtx<'_>, a: &Hash, b: &Hash) -> Result<Option<Hash>> {
    let level = ctx.get::<SkipList>(a)?.generation.min(ctx.get::<SkipList>(b)?.generation);
    let mut x = ancestor_at(ctx, a, level)?;
    let mut y = ancestor_at(ctx, b, level)?;
    loop {
        if x == y {
            return Ok(Some(x));
        }
        let sx: SkipList = ctx.get(&x)?;
        let sy: SkipList = ctx.get(&y)?;
        let Some(top) = sx.skips.len().min(sy.skips.len()).checked_sub(1) else {
            return Ok(None);
        };
        let jump = (0..=top).rev().find(|&i| sx.skips[i] != sy.skips[i]).unwrap_or(0);
        x = sx.skips[jump];
        y = sy.skips[jump];
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-derive`
Expected: `test result: ok. 6 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-derive
git commit -m "feat(derive): add SkipList with ancestor jumps and merge base"
```

---

### Task 12: `TreeIndex` (Merkle tree) with lookup and flatten

**Files:**
- Create: `crates/trellis-derive/src/tree.rs`
- Modify: `crates/trellis-derive/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

`crates/trellis-derive/src/tree.rs`:

```rust
use std::collections::BTreeMap;

use trellis_core::codec::{Reader, Writer};
use trellis_core::{BlobRef, Changeset, CoreError, FileChange, FileKind, Hash, RepoPath};

use crate::error::{DeriveError, Result};
use crate::framework::{Derivable, DeriveCtx};

const TREE_MAGIC: &[u8] = b"TRLS-TR\x01";
const INDEX_MAGIC: &[u8] = b"TRLS-TI\x01";

/// One directory entry.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TreeEntry {
    File { blob: BlobRef, kind: FileKind },
    Dir(Hash),
}

/// One directory, stored as a content-addressed blob. Empty directories are never stored
/// except as the root of an empty repository.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Tree {
    pub entries: BTreeMap<String, TreeEntry>,
}

/// The root tree hash of the full file tree at a changeset (spec §4.1: the full tree is
/// derived data, never stored in the changeset).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TreeIndex {
    pub root: Hash,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use crate::testutil::Fixture;

    use super::*;

    fn read(fx: &Fixture, id: &Hash, path: &str) -> Option<String> {
        read_file(&fx.ctx(), id, &RepoPath::parse(path).unwrap())
            .unwrap()
            .map(|bytes| String::from_utf8(bytes).unwrap())
    }

    #[test]
    fn tree_tracks_upserts_deletes_and_empty_directories() {
        let fx = Fixture::new();
        let c1 = fx.commit(None, &[("a.txt", Some("A")), ("d/e/f.md", Some("F")), ("d/g.json", Some("{}"))]);
        let c2 = fx.commit(Some(c1), &[("d/e/f.md", None), ("a.txt", Some("A2"))]);
        assert_eq!(read(&fx, &c1, "d/e/f.md").as_deref(), Some("F"));
        assert_eq!(read(&fx, &c2, "d/e/f.md"), None);
        assert_eq!(read(&fx, &c2, "a.txt").as_deref(), Some("A2"));
        assert_eq!(read(&fx, &c2, "d/g.json").as_deref(), Some("{}"));

        let ctx = fx.ctx();
        let root = ctx.get::<TreeIndex>(&c2).unwrap().root;
        // "d/e" became empty and is removed; "d" survives because of "d/g.json".
        assert_eq!(lookup(&ctx, &root, &RepoPath::parse("d/e").unwrap()).unwrap(), None);
        assert!(matches!(lookup(&ctx, &root, &RepoPath::parse("d").unwrap()).unwrap(), Some(TreeEntry::Dir(_))));
        // A path through a file is absent, not an error.
        assert_eq!(lookup(&ctx, &root, &RepoPath::parse("a.txt/x").unwrap()).unwrap(), None);
    }

    #[test]
    fn deleting_everything_yields_the_empty_tree() {
        let fx = Fixture::new();
        let c1 = fx.commit(None, &[("x/y.txt", Some("1"))]);
        let c2 = fx.commit(Some(c1), &[("x/y.txt", None)]);
        let ctx = fx.ctx();
        let root = ctx.get::<TreeIndex>(&c2).unwrap().root;
        assert_eq!(root, Hash::of(&Tree::default().encode()));
        assert!(flatten(&ctx, &root).unwrap().is_empty());
    }

    #[test]
    fn tree_decode_rejects_unsorted_or_invalid_names() {
        let mut w = Writer::new(TREE_MAGIC);
        w.count(2);
        w.str("b");
        w.u8(2);
        w.hash(&Hash::of(b"x"));
        w.str("a");
        w.u8(2);
        w.hash(&Hash::of(b"y"));
        assert!(Tree::decode(&w.finish()).is_err());

        let mut w = Writer::new(TREE_MAGIC);
        w.count(1);
        w.str("a/b");
        w.u8(2);
        w.hash(&Hash::of(b"x"));
        assert!(Tree::decode(&w.finish()).is_err());
    }

    const UNIVERSE: [&str; 6] = ["a.txt", "b.md", "d/x.txt", "d/e/y.json", "d/e/z.txt", "f/g/h/i.txt"];

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Spec §7 invariant 7: the tree index equals a naive replay, and the root depends
        /// only on the final contents, not on the history that produced them.
        #[test]
        fn tree_index_matches_naive_replay(
            steps in prop::collection::vec(
                prop::collection::vec((0usize..UNIVERSE.len(), prop::option::of(0u8..4)), 1..5),
                1..12,
            )
        ) {
            let fx = Fixture::new();
            let ctx = fx.ctx();
            let mut naive: BTreeMap<String, String> = BTreeMap::new();
            let mut parent = None;
            for step in &steps {
                let mut edits: BTreeMap<&str, Option<String>> = BTreeMap::new();
                for (i, content) in step {
                    edits.insert(UNIVERSE[*i], content.map(|c| format!("content {c}")));
                }
                let refs: Vec<(&str, Option<&str>)> =
                    edits.iter().map(|(p, c)| (*p, c.as_deref())).collect();
                let id = fx.commit(parent, &refs);
                for (path, content) in &edits {
                    match content {
                        Some(c) => { naive.insert((*path).to_owned(), c.clone()); }
                        None => { naive.remove(*path); }
                    }
                }

                let root = ctx.get::<TreeIndex>(&id).unwrap().root;
                let actual: BTreeMap<String, String> = flatten(&ctx, &root)
                    .unwrap()
                    .into_iter()
                    .map(|(p, (blob, _))| {
                        let bytes = ctx.blobs.get(&blob.hash).unwrap().unwrap();
                        (p.as_str().to_owned(), String::from_utf8(bytes).unwrap())
                    })
                    .collect();
                prop_assert_eq!(&actual, &naive);

                let fresh_edits: Vec<(&str, Option<&str>)> =
                    naive.iter().map(|(p, c)| (p.as_str(), Some(c.as_str()))).collect();
                let fresh = fx.commit(None, &fresh_edits);
                prop_assert_eq!(ctx.get::<TreeIndex>(&fresh).unwrap().root, root);
                parent = Some(id);
            }
        }
    }
}
```

Modify `crates/trellis-derive/src/lib.rs`:

```rust
//! Derived data (spec §4.8): indexes computed recursively from changesets and cached under
//! `(repo, changeset, name, version)`, so a new version can be re-derived side by side.

mod error;
mod framework;
mod pathset;
mod skiplist;
#[cfg(test)]
mod testutil;
mod tree;

pub use error::{DeriveError, Result};
pub use framework::{Derivable, DeriveCtx};
pub use pathset::PathSet;
pub use skiplist::{SkipList, ancestor_at, first_parent_merge_base, is_first_parent_ancestor};
pub use tree::{Tree, TreeEntry, TreeIndex, flatten, lookup, read_file};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-derive tree`
Expected: compile error, `cannot find function 'read_file'`.

- [ ] **Step 3: Implement trees, the index, and the readers**

Insert between `TreeIndex` and the tests:

```rust
impl Tree {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(TREE_MAGIC);
        w.count(self.entries.len());
        for (name, entry) in &self.entries {
            w.str(name);
            match entry {
                TreeEntry::File { blob, kind } => {
                    w.u8(1);
                    w.hash(&blob.hash);
                    w.u64(blob.size);
                    w.u8(kind.to_u8());
                }
                TreeEntry::Dir(hash) => {
                    w.u8(2);
                    w.hash(hash);
                }
            }
        }
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, TREE_MAGIC)?;
        let n = r.count()?;
        let mut entries = BTreeMap::new();
        let mut previous: Option<String> = None;
        for _ in 0..n {
            let name = r.string()?;
            if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                return Err(CoreError::Decode("invalid tree entry name").into());
            }
            if previous.as_ref().is_some_and(|prev| prev >= &name) {
                return Err(CoreError::Decode("tree entries not strictly sorted").into());
            }
            let entry = match r.u8()? {
                1 => {
                    let hash = r.hash()?;
                    let size = r.u64()?;
                    let kind = FileKind::from_u8(r.u8()?)?;
                    TreeEntry::File { blob: BlobRef { hash, size }, kind }
                }
                2 => TreeEntry::Dir(r.hash()?),
                _ => return Err(CoreError::Decode("unknown tree entry tag").into()),
            };
            previous = Some(name.clone());
            entries.insert(name, entry);
        }
        r.finish()?;
        Ok(Self { entries })
    }
}

pub(crate) fn load_tree(ctx: &DeriveCtx<'_>, hash: &Hash) -> Result<Tree> {
    let bytes = ctx.blobs.get(hash)?.ok_or(DeriveError::MissingObject(*hash))?;
    Tree::decode(&bytes)
}

fn store_tree(ctx: &DeriveCtx<'_>, tree: &Tree) -> Result<Hash> {
    Ok(ctx.blobs.put(&tree.encode())?.hash)
}

type Edits = Vec<(Vec<String>, FileChange)>;

/// Applies `changes` (paths split into segments, relative to this directory) to the
/// directory `dir`, writing only the directories on changed paths. Returns `None` when the
/// directory ends up empty. Cost is O(changes × depth), independent of repository size.
fn apply_dir(ctx: &DeriveCtx<'_>, dir: Option<Hash>, changes: Edits) -> Result<Option<Hash>> {
    let mut tree = match dir {
        Some(hash) => load_tree(ctx, &hash)?,
        None => Tree::default(),
    };
    let mut nested: BTreeMap<String, Edits> = BTreeMap::new();
    for (segments, change) in changes {
        let mut segments = segments.into_iter();
        let Some(head) = segments.next() else { continue };
        let rest: Vec<String> = segments.collect();
        if rest.is_empty() {
            match change {
                FileChange::Upsert { blob, kind } => {
                    tree.entries.insert(head, TreeEntry::File { blob, kind });
                }
                FileChange::Delete => {
                    tree.entries.remove(&head);
                }
            }
        } else {
            nested.entry(head).or_default().push((rest, change));
        }
    }
    for (name, sub_changes) in nested {
        let existing = match tree.entries.get(&name) {
            Some(TreeEntry::Dir(hash)) => Some(*hash),
            _ => None,
        };
        match apply_dir(ctx, existing, sub_changes)? {
            Some(hash) => {
                tree.entries.insert(name, TreeEntry::Dir(hash));
            }
            None => {
                if existing.is_some() {
                    tree.entries.remove(&name);
                }
            }
        }
    }
    if tree.entries.is_empty() {
        return Ok(None);
    }
    Ok(Some(store_tree(ctx, &tree)?))
}

impl Derivable for TreeIndex {
    const NAME: &'static str = "tree_index";
    const VERSION: u32 = 1;

    fn derive(ctx: &DeriveCtx<'_>, _id: &Hash, cs: &Changeset) -> Result<Self> {
        let base = match cs.first_parent() {
            Some(parent) => Some(ctx.get::<TreeIndex>(&parent)?.root),
            None => None,
        };
        let changes: Edits = cs
            .changes
            .iter()
            .map(|(path, change)| (path.segments().map(str::to_owned).collect(), change.clone()))
            .collect();
        let root = match apply_dir(ctx, base, changes)? {
            Some(hash) => hash,
            None => store_tree(ctx, &Tree::default())?,
        };
        Ok(Self { root })
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(INDEX_MAGIC);
        w.hash(&self.root);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, INDEX_MAGIC)?;
        let root = r.hash()?;
        r.finish()?;
        Ok(Self { root })
    }
}

/// The entry at `path` under the tree `root`, or `None`.
pub fn lookup(ctx: &DeriveCtx<'_>, root: &Hash, path: &RepoPath) -> Result<Option<TreeEntry>> {
    let mut tree = load_tree(ctx, root)?;
    let mut segments = path.segments().peekable();
    while let Some(segment) = segments.next() {
        let Some(entry) = tree.entries.get(segment).cloned() else {
            return Ok(None);
        };
        if segments.peek().is_none() {
            return Ok(Some(entry));
        }
        match entry {
            TreeEntry::Dir(hash) => tree = load_tree(ctx, &hash)?,
            TreeEntry::File { .. } => return Ok(None),
        }
    }
    Ok(None)
}

/// Every file under the tree `root`.
pub fn flatten(ctx: &DeriveCtx<'_>, root: &Hash) -> Result<BTreeMap<RepoPath, (BlobRef, FileKind)>> {
    let mut out = BTreeMap::new();
    flatten_into(ctx, root, "", &mut out)?;
    Ok(out)
}

fn flatten_into(
    ctx: &DeriveCtx<'_>,
    tree: &Hash,
    prefix: &str,
    out: &mut BTreeMap<RepoPath, (BlobRef, FileKind)>,
) -> Result<()> {
    for (name, entry) in load_tree(ctx, tree)?.entries {
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        match entry {
            TreeEntry::File { blob, kind } => {
                out.insert(RepoPath::parse(&path)?, (blob, kind));
            }
            TreeEntry::Dir(hash) => flatten_into(ctx, &hash, &path, out)?,
        }
    }
    Ok(())
}

/// The content of `path` at changeset `id`, or `None` if it is not a file there.
pub fn read_file(ctx: &DeriveCtx<'_>, id: &Hash, path: &RepoPath) -> Result<Option<Vec<u8>>> {
    let root = ctx.get::<TreeIndex>(id)?.root;
    match lookup(ctx, &root, path)? {
        Some(TreeEntry::File { blob, .. }) => {
            Ok(Some(ctx.blobs.get(&blob.hash)?.ok_or(DeriveError::MissingObject(blob.hash))?))
        }
        _ => Ok(None),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-derive`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/trellis-derive
git commit -m "feat(derive): add Merkle TreeIndex with lookup, flatten, and read_file"
```

---

### Task 13: `paths_changed_since`, and final verification

**Files:**
- Create: `crates/trellis-derive/src/paths.rs`
- Modify: `crates/trellis-derive/src/lib.rs`, `README.md` (Contributing commands)

- [ ] **Step 1: Write the failing tests**

`crates/trellis-derive/src/paths.rs`:

```rust
use std::collections::BTreeSet;

use trellis_core::{Hash, RepoPath};

use crate::error::{DeriveError, Result};
use crate::framework::DeriveCtx;
use crate::pathset::PathSet;
use crate::skiplist::is_first_parent_ancestor;

#[cfg(test)]
mod tests {
    use crate::testutil::Fixture;

    use super::*;

    fn names(set: &BTreeSet<RepoPath>) -> Vec<&str> {
        set.iter().map(RepoPath::as_str).collect()
    }

    #[test]
    fn unions_paths_on_the_first_parent_range() {
        let fx = Fixture::new();
        let c1 = fx.commit(None, &[("a", Some("1")), ("b", Some("1"))]);
        let c2 = fx.commit(Some(c1), &[("b", Some("2")), ("c", Some("2"))]);
        let c3 = fx.commit(Some(c2), &[("d", Some("3"))]);
        let ctx = fx.ctx();
        assert_eq!(names(&paths_changed_since(&ctx, &c1, &c3).unwrap()), ["b", "c", "d"]);
        assert!(paths_changed_since(&ctx, &c3, &c3).unwrap().is_empty());
        assert!(matches!(
            paths_changed_since(&ctx, &c3, &c1),
            Err(DeriveError::InvalidArgument(_))
        ));
    }
}
```

Modify `crates/trellis-derive/src/lib.rs`:

```rust
//! Derived data (spec §4.8): indexes computed recursively from changesets and cached under
//! `(repo, changeset, name, version)`, so a new version can be re-derived side by side.

mod error;
mod framework;
mod paths;
mod pathset;
mod skiplist;
#[cfg(test)]
mod testutil;
mod tree;

pub use error::{DeriveError, Result};
pub use framework::{Derivable, DeriveCtx};
pub use paths::paths_changed_since;
pub use pathset::PathSet;
pub use skiplist::{SkipList, ancestor_at, first_parent_merge_base, is_first_parent_ancestor};
pub use tree::{Tree, TreeEntry, TreeIndex, flatten, lookup, read_file};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p trellis-derive paths`
Expected: compile error, `cannot find function 'paths_changed_since'`.

- [ ] **Step 3: Implement `paths_changed_since`**

Insert above the tests in `crates/trellis-derive/src/paths.rs`:

```rust
/// Every path changed by the changesets in `(from, to]` along `to`'s first-parent chain.
/// Landing uses this to find what changed on main since a session's base (spec §4.4).
pub fn paths_changed_since(ctx: &DeriveCtx<'_>, from: &Hash, to: &Hash) -> Result<BTreeSet<RepoPath>> {
    if !is_first_parent_ancestor(ctx, from, to)? {
        return Err(DeriveError::InvalidArgument("`from` is not a first-parent ancestor of `to`"));
    }
    let mut paths = BTreeSet::new();
    let mut current = *to;
    while current != *from {
        paths.extend(ctx.get::<PathSet>(&current)?.0);
        current = ctx
            .changeset(&current)?
            .first_parent()
            .ok_or(DeriveError::InvalidArgument("reached a root before `from`"))?;
    }
    Ok(paths)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p trellis-derive`
Expected: `test result: ok. 11 passed`.

- [ ] **Step 5: Run the full workspace checks**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: no clippy warnings; every crate reports `test result: ok` (core 14, store 9, merge 16, derive 11).

If `cargo fmt` reformatted files, that is expected; include the changes in the commit.

- [ ] **Step 6: Update the README's Contributing commands to stable tooling**

In `README.md`, replace:

```bash
cargo +nightly fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

with:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 7: Commit**

```bash
git add crates/trellis-derive README.md
git commit -m "feat(derive): add paths_changed_since; M0 complete"
```

---

## Out of scope for this plan (next plans)

- **M1 (landing):** sessions/overlays, the admission routes (parallel/compose/serialize/fail-closed), the landing loop with CAS retries and group commit, idempotent receipts, first-class `Conflict` objects, `seq` on main, file/directory name-collision rejection at landing, and `trellis-sim` for invariants 1, 3, 6, and 8.
- **M2:** tasks, leases, intents, read-set notifications, the context log, and policies.
- **M3:** HTTP/SSE server, MCP, CLI, client checkout/sync, and the Git bridge.
