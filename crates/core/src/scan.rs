//! 로컬 볼트 스캔 + 제외 규칙
use crate::config::AppConfig;
use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::path::Path;

/// 항상 제외되는 패턴
pub const DEFAULT_IGNORES: &[&str] = &[
    ".trash/**",
    ".git/**",
    ".obsidian/workspace.json",
    ".obsidian/workspace-mobile.json",
    ".obsidian/workspaces.json",
    "**/.DS_Store",
    "**/Thumbs.db",
    "**/desktop.ini",
    "**/*.tmp",
    "**/~$*",
    "**/.obsync-tmp-*",
    "**/@eaDir/**",
    "**/#recycle/**",
    "**/.SynologyWorkingDirectory/**",
];

pub struct Ignore {
    set: GlobSet,
}

impl Ignore {
    pub fn new(cfg: &AppConfig) -> Result<Self> {
        let mut b = GlobSetBuilder::new();
        for p in DEFAULT_IGNORES {
            b.add(Glob::new(p)?);
        }
        if !cfg.sync_obsidian_folder {
            b.add(Glob::new(".obsidian/**")?);
        }
        for p in &cfg.ignore_patterns {
            let p = p.trim();
            if p.is_empty() {
                continue;
            }
            b.add(Glob::new(p)?);
            // 폴더 이름만 적은 경우 하위 전체 제외
            if !p.contains('*') {
                b.add(Glob::new(&format!("{}/**", p.trim_end_matches('/')))?);
            }
        }
        Ok(Self { set: b.build()? })
    }

    pub fn is_ignored(&self, rel: &str) -> bool {
        self.set.is_match(rel)
    }
}

#[derive(Clone, Debug)]
pub struct LocalFile {
    pub size: u64,
    /// 수정 시각 (unix ms)
    pub mtime: i64,
}

pub fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn rel_of(root: &Path, p: &Path) -> Option<String> {
    let rel = p.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

pub fn scan(root: &Path, ignore: &Ignore) -> Result<HashMap<String, LocalFile>> {
    let mut out = HashMap::new();
    let mut it = walkdir::WalkDir::new(root).follow_links(true).into_iter();
    while let Some(entry) = it.next() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::warn!("스캔 건너뜀: {e}");
                continue;
            }
        };
        let Some(rel) = rel_of(root, entry.path()) else {
            continue;
        };
        if entry.file_type().is_dir() {
            if ignore.is_ignored(&format!("{rel}/x")) {
                it.skip_current_dir();
            }
            continue;
        }
        if !entry.file_type().is_file() || ignore.is_ignored(&rel) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        out.insert(
            rel,
            LocalFile {
                size: meta.len(),
                mtime: mtime_ms(&meta),
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores() {
        let mut cfg = AppConfig::default();
        cfg.ignore_patterns = vec!["Private".into(), "*.pdf".into()];
        let ig = Ignore::new(&cfg).unwrap();
        assert!(ig.is_ignored(".obsidian/workspace.json"));
        assert!(!ig.is_ignored(".obsidian/app.json"));
        assert!(ig.is_ignored(".trash/a.md"));
        assert!(ig.is_ignored("Private/a.md"));
        assert!(ig.is_ignored("docs/a.pdf"));
        assert!(!ig.is_ignored("notes/a.md"));
        cfg.sync_obsidian_folder = false;
        let ig = Ignore::new(&cfg).unwrap();
        assert!(ig.is_ignored(".obsidian/app.json"));
    }
}
