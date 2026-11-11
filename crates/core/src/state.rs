//! 마지막 동기화 상태 (3-way 비교의 기준점)
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileState {
    /// 마지막 동기화 시점 로컬 내용 sha256
    pub hash: String,
    pub size: u64,
    pub mtime: i64,
    /// 마지막 동기화 시점 원격 리비전
    pub remote_rev: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct SyncState {
    #[serde(default)]
    pub backend_id: String,
    #[serde(default)]
    pub files: BTreeMap<String, FileState>,
    /// Once a mass deletion is held, changing the file count must not approve it.
    /// Cleared only by explicit approval or a complete plan with no deletions.
    #[serde(default)]
    pub deletion_pending: bool,
}

impl SyncState {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}
