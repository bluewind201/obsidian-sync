//! 로컬/네트워크 폴더 저장소.
//! Synology SMB 공유폴더(`\\NAS\home\Obsidian`), 매핑된 드라이브, Synology Drive 동기화 폴더 등에 사용합니다.
use super::{check_path, Backend, RemoteEntry, CHANGE_MARKER};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    pub fn new(path: &str) -> Result<Self> {
        let root = PathBuf::from(path.trim());
        if !root.exists() {
            std::fs::create_dir_all(&root)
                .with_context(|| format!("대상 폴더를 만들 수 없습니다: {}", root.display()))?;
        }
        if !root.is_dir() {
            bail!("대상 경로가 폴더가 아닙니다: {}", root.display());
        }
        Ok(Self { root })
    }

    fn full(&self, rel: &str) -> PathBuf {
        let mut p = self.root.clone();
        for seg in rel.split('/') {
            p.push(seg);
        }
        p
    }
}

pub fn rev_of(meta: &std::fs::Metadata) -> String {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{}-{}", meta.len(), mtime)
}

/// 임시파일에 쓴 뒤 이름 변경 (원자적 쓰기)
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = temp_path(path);
    std::fs::write(&tmp, data)?;
    replace_file(&tmp, path)
}

/// 임시파일 경로 (같은 폴더, 동기화 제외 이름)
pub fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    path.with_file_name(format!(".obsync-tmp-{name}"))
}

/// 임시파일을 대상 이름으로 바꿈 (Windows에서 대상이 있으면 지운 뒤 다시 시도)
pub fn replace_file(tmp: &Path, path: &Path) -> Result<()> {
    if let Err(e) = std::fs::rename(tmp, path) {
        let _ = std::fs::remove_file(path);
        if std::fs::rename(tmp, path).is_err() {
            let _ = std::fs::remove_file(tmp);
            return Err(e.into());
        }
    }
    Ok(())
}

/// 파일을 스트리밍 복사한 뒤 원자적으로 교체
pub fn copy_atomic(src: &Path, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = temp_path(path);
    if let Err(e) = std::fs::copy(src, &tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    replace_file(&tmp, path)
}

/// 빈 상위 폴더 정리
pub fn cleanup_empty_dirs(root: &Path, file: &Path) {
    let mut cur = file.parent();
    while let Some(dir) = cur {
        if dir == root || !dir.starts_with(root) {
            break;
        }
        if std::fs::remove_dir(dir).is_err() {
            break;
        }
        cur = dir.parent();
    }
}

impl Backend for LocalBackend {
    fn id(&self) -> String {
        format!("local:{}", self.root.display())
    }

    fn list(&mut self) -> Result<Vec<RemoteEntry>> {
        let mut out = Vec::new();
        for entry in walkdir::WalkDir::new(&self.root).follow_links(false) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry.path().strip_prefix(&self.root)?;
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            if rel
                .rsplit('/')
                .next()
                .is_some_and(|n| n.starts_with(".obsync-tmp-"))
            {
                continue;
            }
            let meta = entry.metadata()?;
            out.push(RemoteEntry {
                path: rel,
                size: meta.len(),
                rev: rev_of(&meta),
            });
        }
        Ok(out)
    }

    fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        check_path(path)?;
        Ok(std::fs::read(self.full(path))?)
    }

    fn upload(&mut self, path: &str, data: &[u8]) -> Result<RemoteEntry> {
        check_path(path)?;
        let full = self.full(path);
        write_atomic(&full, data)?;
        let meta = std::fs::metadata(&full)?;
        Ok(RemoteEntry {
            path: path.into(),
            size: meta.len(),
            rev: rev_of(&meta),
        })
    }

    fn upload_file(&mut self, path: &str, src: &Path) -> Result<RemoteEntry> {
        check_path(path)?;
        let full = self.full(path);
        copy_atomic(src, &full)?;
        let meta = std::fs::metadata(&full)?;
        Ok(RemoteEntry {
            path: path.into(),
            size: meta.len(),
            rev: rev_of(&meta),
        })
    }

    fn download_to(&mut self, path: &str, dest: &mut dyn std::io::Write) -> Result<()> {
        check_path(path)?;
        let mut f = std::fs::File::open(self.full(path))?;
        std::io::copy(&mut f, dest)?;
        Ok(())
    }

    fn change_marker(&mut self) -> Result<Option<String>> {
        match std::fs::read_to_string(self.full(CHANGE_MARKER)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Some(String::new())),
            Err(e) => Err(e.into()),
        }
    }

    fn touch_change_marker(&mut self, token: &str) -> Result<()> {
        write_atomic(&self.full(CHANGE_MARKER), token.as_bytes())
    }

    fn delete(&mut self, path: &str) -> Result<()> {
        check_path(path)?;
        let full = self.full(path);
        match std::fs::remove_file(&full) {
            Ok(()) => {
                cleanup_empty_dirs(&self.root, &full);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
