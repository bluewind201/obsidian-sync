//! Append-only vault replication. Remote history is never overwritten or deleted.
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use opendal::{Operator, Scheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use uuid::Uuid;

const MAX_FILE: u64 = 32 * 1024 * 1024;
const PREFIX: &str = "vault-sync-v1";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub vault: PathBuf,
    pub backend: String,
    pub options: HashMap<String, String>,
    pub propagate_deletes: bool,
    pub interval_seconds: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            vault: PathBuf::new(),
            backend: "fs".into(),
            options: HashMap::from([("root".into(), String::new())]),
            propagate_deletes: false,
            interval_seconds: 15,
        }
    }
}
impl Config {
    pub fn operator(&self) -> Result<Operator> {
        ensure!(
            ["fs", "webdav", "s3", "onedrive", "gdrive"].contains(&self.backend.as_str()),
            "Unsupported backend"
        );
        ensure!(self.vault.is_dir(), "Select an existing vault folder");
        ensure!(
            self.options
                .get("root")
                .is_some_and(|s| !s.trim().is_empty()),
            "A dedicated remote root is required"
        );
        if let Some(endpoint) = self.options.get("endpoint") {
            ensure!(endpoint.starts_with("https://"), "Use an HTTPS endpoint");
        }
        if self.backend == "fs" {
            let remote = Path::new(&self.options["root"]);
            ensure!(
                remote.is_dir(),
                "Remote folder must already exist / NAS must be mounted"
            );
            let local = self.vault.canonicalize()?;
            let remote = remote.canonicalize()?;
            ensure!(
                !local.starts_with(&remote) && !remote.starts_with(&local),
                "Vault and remote folders must not overlap"
            );
        }
        let mut options = self.options.clone();
        options.retain(|_, value| !value.is_empty());
        if options.contains_key("refresh_token") {
            options.remove("access_token");
        }
        Ok(Operator::via_map(
            Scheme::from_str(&self.backend)?,
            options,
        )?)
    }
    fn identity(&self) -> Result<String> {
        let identity: BTreeMap<_, _> = self
            .options
            .iter()
            .filter(|(k, _)| {
                [
                    "root",
                    "bucket",
                    "endpoint",
                    "region",
                    "username",
                    "client_id",
                ]
                .contains(&k.as_str())
            })
            .collect();
        Ok(hash(
            serde_json::to_vec(&(self.vault.canonicalize()?, &self.backend, identity))?.as_slice(),
        ))
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Event {
    id: String,
    path: String,
    parents: Vec<String>,
    blob: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Baseline {
    heads: Vec<String>,
    blob: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
struct State {
    files: BTreeMap<String, Baseline>,
}
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Report {
    pub uploaded: usize,
    pub downloaded: usize,
    pub deleted: usize,
    pub conflicts: usize,
    pub deferred: usize,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex_id(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
pub fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= 1024,
        "Invalid path length"
    );
    for part in path.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != "..",
            "Unsafe path"
        );
        ensure!(!part.ends_with([' ', '.']), "Nonportable path");
        ensure!(
            !part
                .chars()
                .any(|c| c.is_control() || "\\:<>\"|?*".contains(c)),
            "Unsafe path character"
        );
        let stem = part.split('.').next().unwrap().to_ascii_uppercase();
        ensure!(
            ![
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9"
            ]
            .contains(&stem.as_str()),
            "Reserved filename"
        );
    }
    ensure!(!excluded(path), "Internal path is excluded");
    Ok(())
}
fn excluded(path: &str) -> bool {
    path.split('/')
        .any(|p| [".git", ".trash", ".sync-conflicts", ".sync-backups"].contains(&p))
        || path.starts_with(".obsidian/workspace")
        || path.ends_with(".tmp")
}
fn safe_target(root: &Path, path: &str) -> Result<PathBuf> {
    validate_path(path)?;
    let mut target = root.to_path_buf();
    for part in path.split('/') {
        target.push(part);
        if let Ok(meta) = fs::symlink_metadata(&target) {
            ensure!(!meta.file_type().is_symlink(), "Symlink refused");
            ensure!(
                target.canonicalize()?.starts_with(root.canonicalize()?),
                "Path escapes vault"
            );
        }
    }
    Ok(target)
}
fn read_file(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::metadata(path) {
        Ok(m) => {
            ensure!(
                m.is_file() && m.len() <= MAX_FILE,
                "File exceeds 32 MiB or is not a regular file"
            );
            let bytes = fs::read(path)?;
            ensure!(bytes.len() as u64 <= MAX_FILE, "File exceeds 32 MiB");
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn scan(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let mut folded = BTreeSet::new();
    for item in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .is_none_or(|p| !excluded(&p.to_string_lossy().replace('\\', "/")))
        })
    {
        let item = item?;
        ensure!(
            !item.file_type().is_symlink(),
            "Remove symlinks from the vault before syncing"
        );
        if !item.file_type().is_file() {
            continue;
        }
        let path = item
            .path()
            .strip_prefix(root)?
            .to_str()
            .context("Non UTF-8 filename")?
            .replace('\\', "/");
        validate_path(&path)?;
        ensure!(
            folded.insert(path.to_lowercase()),
            "Case-insensitive filename collision"
        );
        let target = safe_target(root, &path)?;
        if let Some(bytes) = read_file(&target)? {
            files.insert(path, hash(&bytes));
        }
    }
    Ok(files)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("No parent folder")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn archive(root: &Path, folder: &str, path: &str, bytes: &[u8]) -> Result<()> {
    // Hash filenames avoid collisions, path injection and Windows path-length expansion.
    let dir = root.join(folder);
    if dir.exists() {
        ensure!(
            !fs::symlink_metadata(&dir)?.file_type().is_symlink(),
            "Unsafe recovery folder"
        );
    }
    fs::create_dir_all(&dir)?;
    ensure!(
        dir.canonicalize()?.starts_with(root.canonicalize()?),
        "Recovery folder escapes vault"
    );
    let filename = format!("{}-{}", hash(path.as_bytes()), hash(bytes));
    let dest = dir.join(&filename);
    if !dest.exists() {
        atomic_write(&dest, bytes)?;
    }
    atomic_write(
        &dir.join(format!("{filename}.json")),
        &serde_json::to_vec(&serde_json::json!({"original_path": path, "sha256": hash(bytes)}))?,
    )?;
    Ok(())
}
async fn fetch_blob(op: &Operator, id: &str) -> Result<Vec<u8>> {
    ensure!(hex_id(id, 64), "Invalid blob identifier");
    let key = format!("{PREFIX}/blobs/{id}");
    ensure!(
        op.stat(&key).await?.content_length() <= MAX_FILE,
        "Remote file exceeds 32 MiB"
    );
    let bytes = op.read(&key).await?.to_vec();
    ensure!(hash(&bytes) == id, "Remote content checksum mismatch");
    Ok(bytes)
}

/// One complete reconciliation. A local lock serializes desktop, Android and manual calls.
pub async fn sync(config: &Config, state_root: &Path) -> Result<Report> {
    let op = config.operator()?;
    sync_with(config, state_root, op).await
}
async fn sync_with(config: &Config, state_root: &Path, op: Operator) -> Result<Report> {
    ensure!(
        config.vault.is_dir(),
        "Vault is unavailable; refusing to interpret this as deletion"
    );
    let root = config.vault.canonicalize()?;
    fs::create_dir_all(state_root)?;
    ensure!(
        !state_root.canonicalize()?.starts_with(&root),
        "State directory must be outside the vault"
    );
    let state_dir = state_root.join(config.identity()?);
    fs::create_dir_all(&state_dir)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state_dir.join("lock"))?;
    lock.try_lock_exclusive().context("Sync already running")?;
    let state_path = state_dir.join("state.json");
    let mut state: State = if state_path.exists() {
        serde_json::from_slice(&fs::read(&state_path)?)
            .context("State is damaged; restore it before syncing")?
    } else {
        State::default()
    };
    let local = scan(&root)?;
    let mut events: BTreeMap<String, Event> = BTreeMap::new();
    let mut report = Report::default();
    // A listing failure aborts the run. Missing history never means delete everything.
    if config.backend == "onedrive" {
        for dir in [
            format!("{PREFIX}/"),
            format!("{PREFIX}/blobs/"),
            format!("{PREFIX}/events/"),
        ] {
            match op.stat(&dir).await {
                Ok(meta) => ensure!(meta.is_dir(), "Remote history path is not a folder"),
                Err(error) if error.kind() == opendal::ErrorKind::NotFound => {
                    op.create_dir(&dir).await?
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    let entries = op
        .list_with(&format!("{PREFIX}/events/"))
        .recursive(true)
        .await?;
    ensure!(
        entries.len() <= 100_000,
        "History exceeds the v1 safety limit"
    );
    for entry in entries {
        if !entry.path().ends_with(".json") {
            continue;
        }
        ensure!(
            op.stat(entry.path()).await?.content_length() <= 65536,
            "Oversized event"
        );
        let event: Event = serde_json::from_slice(&op.read(entry.path()).await?.to_vec())?;
        validate_path(&event.path)?;
        ensure!(
            hex_id(&event.id, 32) && event.parents.iter().all(|p| hex_id(p, 32)),
            "Invalid event ID"
        );
        ensure!(
            event.blob.as_ref().is_none_or(|b| hex_id(b, 64)),
            "Invalid blob ID"
        );
        ensure!(
            entry.path() == format!("{PREFIX}/events/{}.json", event.id),
            "Event key mismatch"
        );
        events.insert(event.id.clone(), event);
    }
    // Detect lost/partial remote history before any local or remote mutation.
    for base in state.files.values() {
        for id in &base.heads {
            ensure!(
                events.contains_key(id),
                "Remote history is missing; check remote root or restore its history"
            );
        }
    }
    for event in events.values() {
        for parent in &event.parents {
            ensure!(
                events.get(parent).is_some_and(|p| p.path == event.path),
                "Incomplete remote history; retry later"
            );
        }
    }
    let mut names = BTreeMap::new();
    for path in local.keys().chain(events.values().map(|e| &e.path)) {
        if let Some(previous) = names.insert(path.to_lowercase(), path) {
            ensure!(previous == path, "Remote filename case collision");
        }
    }
    for path in names.keys() {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            ensure!(
                !names.contains_key(prefix),
                "File and folder path collision"
            );
            parent = prefix;
        }
    }
    let paths: BTreeSet<_> = local.keys().chain(state.files.keys()).cloned().collect();
    let deletes = paths
        .iter()
        .filter(|p| {
            !local.contains_key(*p) && state.files.get(*p).is_some_and(|b| b.blob.is_some())
        })
        .count();
    if config.propagate_deletes {
        ensure!(
            deletes <= 10 && (state.files.len() < 10 || deletes * 5 <= state.files.len()),
            "Mass deletion blocked; restore files or disable deletion propagation"
        );
    }
    for path in paths {
        let base = state.files.get(&path).cloned().unwrap_or_default();
        let now = local.get(&path).cloned();
        if now == base.blob || (now.is_none() && !config.propagate_deletes) {
            continue;
        }
        let target = safe_target(&root, &path)?;
        let content = read_file(&target)?;
        if content.as_ref().map(|b| hash(b)) != now {
            report.deferred += 1;
            continue;
        }
        let event = Event {
            id: Uuid::new_v4().simple().to_string(),
            path: path.clone(),
            parents: base.heads,
            blob: now.clone(),
        };
        if let Some(bytes) = content {
            op.write(&format!("{PREFIX}/blobs/{}", now.as_ref().unwrap()), bytes)
                .await?;
        }
        op.write(
            &format!("{PREFIX}/events/{}.json", event.id),
            serde_json::to_vec(&event)?,
        )
        .await?;
        // Save after publication: restart cannot reinterpret a successful upload as a new edit.
        state.files.insert(
            path,
            Baseline {
                heads: vec![event.id.clone()],
                blob: now,
            },
        );
        atomic_write(&state_path, &serde_json::to_vec(&state)?)?;
        events.insert(event.id.clone(), event);
        report.uploaded += 1;
    }
    let referenced: BTreeSet<_> = events
        .values()
        .flat_map(|e| e.parents.iter().cloned())
        .collect();
    let mut heads: BTreeMap<String, Vec<Event>> = BTreeMap::new();
    for event in events.values().filter(|e| !referenced.contains(&e.id)) {
        heads
            .entry(event.path.clone())
            .or_default()
            .push(event.clone());
    }
    let mut folded = BTreeMap::new();
    for path in heads.keys().chain(local.keys()) {
        if let Some(previous) = folded.insert(path.to_lowercase(), path) {
            ensure!(previous == path, "Remote filename case collision");
        }
    }
    for (path, versions) in heads {
        let expected = local.get(&path).cloned();
        let target = safe_target(&root, &path)?;
        if read_file(&target)?.as_ref().map(|b| hash(b)) != expected {
            report.deferred += 1;
            continue;
        }
        // A live edit wins against concurrent deletion. Other conflicts use stable event ordering.
        let winner = versions
            .iter()
            .filter(|e| e.blob.is_some())
            .next_back()
            .unwrap_or_else(|| versions.last().unwrap());
        let distinct: BTreeSet<_> = versions.iter().map(|e| &e.blob).collect();
        if distinct.len() > 1 {
            report.conflicts += 1;
            for version in &versions {
                if let Some(blob) = &version.blob {
                    archive(
                        &root,
                        ".sync-conflicts",
                        &path,
                        &fetch_blob(&op, blob).await?,
                    )?;
                }
            }
        }
        let mut applied = expected.clone();
        if winner.blob != expected {
            if let Some(blob) = &winner.blob {
                let bytes = fetch_blob(&op, blob).await?;
                // Recheck after network IO; don't overwrite a note edited while downloading.
                let old = read_file(&target)?;
                if old.as_ref().map(|b| hash(b)) != expected {
                    report.deferred += 1;
                    continue;
                }
                if let Some(old) = old {
                    archive(&root, ".sync-backups", &path, &old)?;
                }
                let target = safe_target(&root, &path)?;
                atomic_write(&target, &bytes)?;
                applied = Some(blob.clone());
                report.downloaded += 1;
            } else if config.propagate_deletes {
                if let Some(old) = read_file(&target)? {
                    archive(&root, ".sync-backups", &path, &old)?;
                    if read_file(&target)?.as_ref().map(|b| hash(b)) != expected {
                        report.deferred += 1;
                        continue;
                    }
                    fs::remove_file(&target)?;
                    report.deleted += 1;
                }
                applied = None;
            }
        }
        state.files.insert(
            path,
            Baseline {
                heads: versions.iter().map(|e| e.id.clone()).collect(),
                blob: applied,
            },
        );
    }
    atomic_write(&state_path, &serde_json::to_vec(&state)?)?;
    Ok(report)
}

pub fn run_json(config_json: &str, state_dir: &Path) -> String {
    let result = (|| -> Result<Report> {
        let config: Config = serde_json::from_str(config_json)?;
        tokio::runtime::Runtime::new()?.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(300),
                sync(&config, state_dir),
            )
            .await
            .context("Sync timed out; retry safely")?
        })
    })();
    match result {
        Ok(report) => serde_json::json!({"ok": true, "report": report}).to_string(),
        // Provider errors may contain credential-bearing URLs. Keep them out of UI/logs.
        Err(_) => serde_json::json!({"ok": false, "error": "Sync stopped safely. Check folder access, credentials, network, portable filenames and remote history. Files over 32 MiB and mass deletions are blocked."}).to_string(),
    }
}
