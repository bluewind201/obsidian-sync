//! 양방향 동기화 엔진.
//!
//! 로컬 현재 상태 / 원격 현재 상태 / 마지막 동기화 상태를 비교(3-way)하여
//! 업로드·다운로드·삭제·충돌 처리를 결정합니다.
//!
//! 안전장치
//! - 로컬에서 지워지는 파일은 볼트의 `.trash/` 폴더로 이동 (Obsidian 휴지통과 동일)
//! - 양쪽 모두 수정된 경우 로컬 파일을 `이름 (충돌 기기 날짜).md` 로 보존
//! - 원격/로컬이 갑자기 비었거나 대량 삭제가 예정되면 동기화 중단

use crate::backend::{self, Backend, RemoteEntry};
use crate::config::{ConflictPolicy, Profile, SyncMode};
use crate::scan::{self, mtime_ms, Ignore, LocalFile};
use crate::state::{FileState, SyncState};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncReport {
    pub uploaded: usize,
    pub downloaded: usize,
    pub deleted_local: usize,
    pub deleted_remote: usize,
    pub conflicts: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

impl SyncReport {
    pub fn changes(&self) -> usize {
        self.uploaded + self.downloaded + self.deleted_local + self.deleted_remote + self.conflicts
    }
    pub fn summary(&self) -> String {
        if self.changes() == 0 && self.errors.is_empty() {
            return "변경 없음".into();
        }
        let mut parts = Vec::new();
        if self.uploaded > 0 {
            parts.push(format!("업로드 {}", self.uploaded));
        }
        if self.downloaded > 0 {
            parts.push(format!("다운로드 {}", self.downloaded));
        }
        if self.deleted_local > 0 {
            parts.push(format!("로컬 삭제 {}", self.deleted_local));
        }
        if self.deleted_remote > 0 {
            parts.push(format!("원격 삭제 {}", self.deleted_remote));
        }
        if self.conflicts > 0 {
            parts.push(format!("충돌 {}", self.conflicts));
        }
        if !self.errors.is_empty() {
            parts.push(format!("오류 {}", self.errors.len()));
        }
        parts.join(", ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Upload,
    Download,
    DeleteLocal,
    DeleteRemote,
    Conflict,
    Forget,
    Touch,
    /// 원격에 이미 있는 같은 파일을 동기화된 것으로 기록 (한 방향 모드 첫 동기화)
    Adopt,
    /// 원격 내용으로 덮어씀. 내용이 다르면 내 기기 파일은 `.trash`에 보관
    Replace,
}

pub struct SyncEngine {
    cfg: Profile,
    vault: PathBuf,
    backend: Box<dyn Backend>,
    ignore: Ignore,
    state_path: PathBuf,
    state: SyncState,
    device: String,
}

pub fn hash_bytes(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// 쓰면서 sha256을 계산하는 Writer
struct HashWriter<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn state_file(data_dir: &Path, vault: &str, backend_id: &str) -> PathBuf {
    let key = hash_bytes(format!("{vault}\n{backend_id}").as_bytes());
    data_dir.join(format!("state-{}.json", &key[..16]))
}

impl SyncEngine {
    /// 프로필 하나의 동기화 엔진. `device`는 충돌 파일 이름에 붙일 기기 이름
    pub fn new(cfg: Profile, device: &str, data_dir: &Path) -> Result<Self> {
        cfg.validate(true)?;
        let backend = backend::create_backend(&cfg.backend, data_dir)?;
        Self::with_backend(cfg, device, backend, data_dir)
    }

    pub fn with_backend(
        cfg: Profile,
        device: &str,
        backend: Box<dyn Backend>,
        data_dir: &Path,
    ) -> Result<Self> {
        let vault = PathBuf::from(cfg.local_path.trim());
        let ignore = Ignore::new(&cfg).context("제외 패턴 오류")?;
        let id = backend.id();
        let state_path = state_file(data_dir, &cfg.local_path, &id);
        let mut state = SyncState::load(&state_path);
        if state.backend_id != id {
            state = SyncState {
                backend_id: id,
                ..Default::default()
            };
        }
        let device = device.to_string();
        Ok(Self {
            cfg,
            vault,
            backend,
            ignore,
            state_path,
            state,
            device,
        })
    }

    /// 동기화 기록 초기화 (다음 동기화 때 양쪽을 처음부터 비교)
    pub fn reset_state(&mut self) -> Result<()> {
        self.state.files.clear();
        let _ = std::fs::remove_file(&self.state_path);
        Ok(())
    }

    pub fn profile(&self) -> &Profile {
        &self.cfg
    }

    pub fn is_ignored(&self, rel: &str) -> bool {
        self.ignore.is_ignored(rel)
    }

    fn full(&self, rel: &str) -> PathBuf {
        let mut p = self.vault.clone();
        for seg in rel.split('/') {
            p.push(seg);
        }
        p
    }

    pub fn sync_once(&mut self, log: &dyn Fn(&str)) -> Result<SyncReport> {
        let started = Instant::now();
        if !self.vault.is_dir() {
            bail!("폴더를 찾을 수 없습니다: {}", self.vault.display());
        }
        let mut report = SyncReport::default();

        // 1) 로컬 스캔 + 해시 (크기/수정시각이 같으면 이전 해시 재사용)
        let local = scan::scan(&self.vault, &self.ignore)?;
        let mut local_hash: HashMap<String, String> = HashMap::new();
        for (p, f) in &local {
            let cached = self
                .state
                .files
                .get(p)
                .filter(|s| s.size == f.size && s.mtime == f.mtime)
                .map(|s| s.hash.clone());
            let h = match cached {
                Some(h) => h,
                None => match hash_file(&self.full(p)) {
                    Ok(h) => h,
                    Err(e) => {
                        report.errors.push(format!("{p}: 읽기 실패 ({e})"));
                        continue;
                    }
                },
            };
            local_hash.insert(p.clone(), h);
        }

        // 2) 원격 목록
        let remote: HashMap<String, RemoteEntry> = self
            .backend
            .list()
            .context("원격 목록 조회 실패")?
            .into_iter()
            .filter(|e| backend::is_safe_rel_path(&e.path) && !self.ignore.is_ignored(&e.path))
            .map(|e| (e.path.clone(), e))
            .collect();

        // 3) 안전장치: 한쪽이 통째로 비었는데 그쪽 기준으로 다른 쪽을 지우게 되는 경우 중단
        let known = self.state.files.len();
        let mode = self.cfg.mode;
        let policy = self.cfg.conflict_policy;
        let remote_wipes_local = match mode {
            SyncMode::TwoWay => true,
            SyncMode::DownloadOnly => self.cfg.delete_in_target,
            SyncMode::UploadOnly => false,
        };
        let local_wipes_remote = match mode {
            SyncMode::TwoWay => true,
            SyncMode::UploadOnly => self.cfg.delete_in_target,
            SyncMode::DownloadOnly => false,
        };
        if remote_wipes_local && known >= 5 && remote.is_empty() && !local.is_empty() {
            bail!(
                "원격 저장소가 비어 있습니다. 설정이 바뀌었거나 원격 파일이 지워진 것 같아 안전을 위해 중단했습니다. \
                 의도한 경우 '동기화 기록 초기화'를 눌러 주세요"
            );
        }
        if local_wipes_remote && known >= 5 && local.is_empty() && !remote.is_empty() {
            bail!(
                "내 폴더가 비어 있습니다. 안전을 위해 중단했습니다. 의도한 경우 '동기화 기록 초기화'를 눌러 주세요"
            );
        }

        // 4) 계획 수립
        let all: BTreeSet<&String> = local
            .keys()
            .chain(remote.keys())
            .chain(self.state.files.keys())
            .collect();
        let mut plan: Vec<(String, Action)> = Vec::new();
        for p in all {
            let l = local.get(p);
            let r = remote.get(p);
            let s = self.state.files.get(p);
            if l.is_some() && !local_hash.contains_key(p) {
                continue; // 읽기 실패한 파일
            }
            let lchanged = match (l, s) {
                (Some(_), Some(s)) => local_hash.get(p) != Some(&s.hash),
                (Some(_), None) => true,
                _ => false,
            };
            let rchanged = match (r, s) {
                (Some(r), Some(s)) => r.rev != s.remote_rev,
                (Some(_), None) => true,
                _ => false,
            };
            let touched = |lf: &LocalFile, s: &FileState| s.size != lf.size || s.mtime != lf.mtime;
            let action = match mode {
                // 올리기만: 내 기기 → 원격. 원격 변경은 무시, 원격 삭제는 다시 올림
                SyncMode::UploadOnly => match (l, r, s) {
                    // 처음 연결: 이미 올라가 있는 파일(크기 같음)은 다시 올리지 않고 기록만 함
                    (Some(lf), Some(rf), None) if lf.size == rf.size => Action::Adopt,
                    (Some(_), Some(_), None) | (Some(_), None, _) => Action::Upload,
                    (Some(_), Some(_), Some(_)) if lchanged => Action::Upload,
                    (Some(lf), Some(_), Some(s)) if touched(lf, s) => Action::Touch,
                    (Some(_), Some(_), Some(_)) => continue,
                    (None, Some(_), Some(_)) if self.cfg.delete_in_target => Action::DeleteRemote,
                    (None, _, Some(_)) => Action::Forget,
                    (None, _, None) => continue,
                },
                // 내려받기만: 원격 → 내 기기. 내 기기 변경은 올리지 않음
                SyncMode::DownloadOnly => match (l, r, s) {
                    (Some(lf), Some(rf), None) if lf.size == rf.size => Action::Adopt,
                    // 처음 연결 때 내용이 다르면 원격 기준 (내 기기 파일은 .trash에 보관)
                    (Some(_), Some(_), None) => Action::Replace,
                    (None, Some(_), _) => Action::Download,
                    (Some(_), Some(_), Some(_)) if rchanged && lchanged => Action::Replace,
                    (Some(_), Some(_), Some(_)) if rchanged => Action::Download,
                    (Some(lf), Some(_), Some(s)) if !lchanged && touched(lf, s) => Action::Touch,
                    // 내 기기에서 고친 파일은 원격이 바뀔 때까지 그대로 둠
                    (Some(_), Some(_), Some(_)) => continue,
                    (Some(_), None, Some(_)) if self.cfg.delete_in_target && !lchanged => {
                        Action::DeleteLocal
                    }
                    (Some(_), None, Some(_)) | (None, None, Some(_)) => Action::Forget,
                    // 내 기기에만 있는 파일은 올리지 않음
                    (Some(_), None, None) | (None, None, None) => continue,
                },
                SyncMode::TwoWay => {
                    let propagate = self.cfg.propagate_deletes;
                    let conflict = match policy {
                        ConflictPolicy::KeepBoth => Action::Conflict,
                        ConflictPolicy::PreferLocal => Action::Upload,
                        ConflictPolicy::PreferRemote => Action::Replace,
                    };
                    match (l, r, s) {
                        (Some(_), None, None) => Action::Upload,
                        (None, Some(_), None) => Action::Download,
                        (Some(_), Some(_), None) => conflict,
                        (None, None, Some(_)) => Action::Forget,
                        (None, None, None) => continue,
                        (Some(lf), Some(_), Some(s)) => match (lchanged, rchanged) {
                            (false, false) => {
                                if touched(lf, s) {
                                    Action::Touch
                                } else {
                                    continue;
                                }
                            }
                            (true, false) => Action::Upload,
                            (false, true) => Action::Download,
                            (true, true) => conflict,
                        },
                        (None, Some(_), Some(_)) => {
                            if rchanged || !propagate {
                                Action::Download
                            } else {
                                Action::DeleteRemote
                            }
                        }
                        (Some(_), None, Some(_)) => {
                            if lchanged || !propagate {
                                Action::Upload
                            } else {
                                Action::DeleteLocal
                            }
                        }
                    }
                }
            };
            plan.push((p.clone(), action));
        }

        let deletions = plan
            .iter()
            .filter(|(_, a)| matches!(a, Action::DeleteLocal | Action::DeleteRemote))
            .count();
        if deletions > 20 && deletions * 2 > known {
            bail!(
                "한 번에 {deletions}개 파일이 삭제될 예정이라 안전을 위해 중단했습니다. \
                 의도한 경우 '동기화 기록 초기화'를 누른 뒤 다시 동기화하세요 (이 경우 삭제 대신 복원됩니다)"
            );
        }

        // 5) 실행
        let mut since_save = 0;
        for (p, action) in plan {
            let res = match action {
                Action::Upload => self.do_upload(&p).map(|_| {
                    report.uploaded += 1;
                    log(&format!("↑ {p}"));
                }),
                Action::Download => {
                    let r = remote[&p].clone();
                    self.do_download(&p, &r, local.get(&p)).map(|done| {
                        if done {
                            report.downloaded += 1;
                            log(&format!("↓ {p}"));
                        }
                    })
                }
                Action::DeleteLocal => self.do_delete_local(&p).map(|_| {
                    report.deleted_local += 1;
                    log(&format!("✕ (로컬) {p}"));
                }),
                Action::DeleteRemote => self.do_delete_remote(&p).map(|_| {
                    report.deleted_remote += 1;
                    log(&format!("✕ (원격) {p}"));
                }),
                Action::Conflict => {
                    let r = remote[&p].clone();
                    let lh = local_hash[&p].clone();
                    self.do_conflict(&p, &r, &lh).map(|c| match c {
                        Some(name) => {
                            report.conflicts += 1;
                            log(&format!("⚠ 충돌: {p} → 로컬본을 '{name}' 로 보존"));
                        }
                        None => {}
                    })
                }
                Action::Forget => {
                    self.state.files.remove(&p);
                    Ok(())
                }
                Action::Replace => {
                    let r = remote[&p].clone();
                    self.do_replace(&p, &r).map(|kept| {
                        report.downloaded += 1;
                        match kept {
                            true => log(&format!("↓ {p} (저장소 기준, 이전 내용은 .trash에 보관)")),
                            false => log(&format!("↓ {p}")),
                        }
                    })
                }
                Action::Adopt => {
                    let rev = remote[&p].rev.clone();
                    let h = local_hash[&p].clone();
                    self.record(&p, h, rev)
                }
                Action::Touch => {
                    if let (Some(lf), Some(s)) = (local.get(&p), self.state.files.get_mut(&p)) {
                        s.size = lf.size;
                        s.mtime = lf.mtime;
                    }
                    Ok(())
                }
            };
            if let Err(e) = res {
                let msg = format!("{p}: {e:#}");
                log(&format!("오류 {msg}"));
                report.errors.push(msg);
            }
            since_save += 1;
            if since_save >= 25 {
                since_save = 0;
                let _ = self.state.save(&self.state_path);
            }
        }
        self.state.save(&self.state_path)?;
        report.duration_ms = started.elapsed().as_millis() as u64;
        Ok(report)
    }

    fn record(&mut self, p: &str, hash: String, remote_rev: String) -> Result<()> {
        let meta = std::fs::metadata(self.full(p))?;
        self.state.files.insert(
            p.to_string(),
            FileState {
                hash,
                size: meta.len(),
                mtime: mtime_ms(&meta),
                remote_rev,
            },
        );
        Ok(())
    }

    fn do_upload(&mut self, p: &str) -> Result<()> {
        let full = self.full(p);
        let hash = hash_file(&full)?;
        let entry = self.backend.upload_file(p, &full)?;
        self.record(p, hash, entry.rev)
    }

    /// 원격 파일을 같은 폴더의 임시파일로 내려받고 (임시파일 경로, sha256) 반환
    fn fetch(&mut self, p: &str) -> Result<(PathBuf, String)> {
        let full = self.full(p);
        if let Some(d) = full.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = backend::local::temp_path(&full);
        let res = (|| {
            let file = std::fs::File::create(&tmp)?;
            let mut w = HashWriter {
                inner: std::io::BufWriter::with_capacity(256 * 1024, file),
                hasher: Sha256::new(),
            };
            self.backend.download_to(p, &mut w)?;
            w.flush()?;
            Ok::<_, anyhow::Error>(hex::encode(w.hasher.finalize()))
        })();
        match res {
            Ok(h) => Ok((tmp, h)),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// 다운로드. 스캔 이후 로컬 파일이 바뀌었으면 덮어쓰지 않고 건너뜀 (false 반환)
    fn do_download(
        &mut self,
        p: &str,
        r: &RemoteEntry,
        expected: Option<&LocalFile>,
    ) -> Result<bool> {
        let full = self.full(p);
        let (tmp, hash) = self.fetch(p)?;
        let changed_meanwhile = match (std::fs::metadata(&full), expected) {
            (Ok(m), Some(e)) => m.len() != e.size || mtime_ms(&m) != e.mtime,
            (Ok(_), None) => true,
            _ => false,
        };
        if changed_meanwhile {
            let _ = std::fs::remove_file(&tmp);
            return Ok(false);
        }
        let same = full.is_file() && hash_file(&full).map(|h| h == hash).unwrap_or(false);
        if same {
            let _ = std::fs::remove_file(&tmp);
        } else {
            backend::local::replace_file(&tmp, &full)?;
        }
        self.record(p, hash, r.rev.clone())?;
        Ok(!same)
    }

    /// 원격 내용으로 덮어씀. 덮어쓴 내 기기 파일을 .trash에 보관했으면 true
    fn do_replace(&mut self, p: &str, r: &RemoteEntry) -> Result<bool> {
        let full = self.full(p);
        let (tmp, hash) = self.fetch(p)?;
        let mut kept = false;
        if full.is_file() && hash_file(&full).map(|h| h != hash).unwrap_or(true) {
            if let Err(e) = self.move_to_trash(p) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            kept = true;
        }
        backend::local::replace_file(&tmp, &full)?;
        self.record(p, hash, r.rev.clone())?;
        Ok(kept)
    }

    /// 파일을 폴더 안 `.trash/`로 옮김 (같은 이름이 있으면 시각을 붙임)
    fn move_to_trash(&self, p: &str) -> Result<()> {
        let full = self.full(p);
        let mut dest = self.vault.join(".trash");
        for seg in p.split('/') {
            dest.push(seg);
        }
        if dest.exists() {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let name = format!(
                "{} ({stamp})",
                dest.file_name().unwrap_or_default().to_string_lossy()
            );
            dest.set_file_name(name);
        }
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d)?;
        }
        if std::fs::rename(&full, &dest).is_err() {
            std::fs::copy(&full, &dest)?;
            std::fs::remove_file(&full)?;
        }
        Ok(())
    }

    fn do_delete_local(&mut self, p: &str) -> Result<()> {
        let full = self.full(p);
        if full.exists() {
            self.move_to_trash(p)?;
            backend::local::cleanup_empty_dirs(&self.vault, &full);
        }
        self.state.files.remove(p);
        Ok(())
    }

    fn do_delete_remote(&mut self, p: &str) -> Result<()> {
        self.backend.delete(p)?;
        self.state.files.remove(p);
        Ok(())
    }

    fn conflict_name(&self, p: &str) -> String {
        let (dir, file) = match p.rsplit_once('/') {
            Some((d, f)) => (format!("{d}/"), f),
            None => (String::new(), p),
        };
        let (stem, ext) = match file.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
            _ => (file, String::new()),
        };
        let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
        let mut name = format!("{dir}{stem} (충돌 {} {stamp}){ext}", self.device);
        let mut n = 2;
        while self.full(&name).exists() {
            name = format!("{dir}{stem} (충돌 {} {stamp} {n}){ext}", self.device);
            n += 1;
        }
        name
    }

    /// 양쪽이 모두 바뀐 경우. 내용이 같으면 None, 다르면 보존한 로컬 사본 이름 반환
    fn do_conflict(
        &mut self,
        p: &str,
        r: &RemoteEntry,
        local_hash: &str,
    ) -> Result<Option<String>> {
        let (tmp, rhash) = self.fetch(p)?;
        if rhash == local_hash {
            let _ = std::fs::remove_file(&tmp);
            self.record(p, rhash, r.rev.clone())?;
            return Ok(None);
        }
        let full = self.full(p);
        let cname = self.conflict_name(p);
        let cfull = self.full(&cname);
        if let Err(e) = std::fs::rename(&full, &cfull) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        backend::local::replace_file(&tmp, &full)?;
        self.record(p, rhash, r.rev.clone())?;
        // 보존한 로컬 사본도 원격에 올려 다른 기기에서도 보이게 함
        let chash = hash_file(&cfull)?;
        let centry = self.backend.upload_file(&cname, &cfull)?;
        self.record(&cname, chash, centry.rev)?;
        Ok(Some(cname))
    }
}
