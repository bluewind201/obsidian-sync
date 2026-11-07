//! 백그라운드 동기화 서비스: 프로필마다 실시간 파일 감시 + 주기적 원격 확인
use crate::config::{AppConfig, BackendConfig, Profile};
use crate::engine::{SyncEngine, SyncReport};
use crate::scan::{rel_of, Ignore};
use anyhow::{bail, Result};
use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_LOGS: usize = 300;

#[derive(Clone, Debug, Serialize)]
pub struct LogLine {
    pub time: String,
    pub text: String,
    pub error: bool,
}

/// 프로필 하나의 상태
#[derive(Clone, Debug, Serialize, Default)]
pub struct ProfileStatus {
    pub id: String,
    pub name: String,
    /// 이 프로필의 동기화가 실행 중인지 (시작 실패 시 false)
    pub active: bool,
    pub syncing: bool,
    pub last_sync: Option<String>,
    pub last_summary: String,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct ServiceStatus {
    pub running: bool,
    /// 어느 프로필이든 동기화 중이면 true
    pub syncing: bool,
    /// 가장 최근 동기화 시각
    pub last_sync: Option<String>,
    pub last_summary: String,
    /// 첫 번째 오류 (프로필 이름 포함)
    pub last_error: Option<String>,
    pub logs: Vec<LogLine>,
    pub profiles: Vec<ProfileStatus>,
}

#[derive(Default)]
struct Shared {
    running: bool,
    logs: VecDeque<LogLine>,
    profiles: Vec<ProfileStatus>,
}

impl Shared {
    fn push(&mut self, text: String, error: bool) {
        if error {
            log::warn!("{text}");
        } else {
            log::info!("{text}");
        }
        self.logs.push_back(LogLine {
            time: chrono::Local::now().format("%m-%d %H:%M:%S").to_string(),
            text,
            error,
        });
        while self.logs.len() > MAX_LOGS {
            self.logs.pop_front();
        }
    }
}

enum Cmd {
    SyncNow,
    LocalChange,
    Stop,
}

pub struct SyncService {
    txs: Vec<Sender<Cmd>>,
    shared: Arc<Mutex<Shared>>,
    threads: Vec<JoinHandle<()>>,
    _watchers: Vec<notify::RecommendedWatcher>,
}

/// 서비스 외부(트레이 메뉴 등)에서 "지금 동기화"를 요청할 때 사용하는 핸들
#[derive(Clone)]
pub struct Trigger(Vec<Sender<Cmd>>);
impl Trigger {
    pub fn sync_now(&self) {
        for tx in &self.0 {
            let _ = tx.send(Cmd::SyncNow);
        }
    }
}

fn now_str() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn start_watcher(
    profile: &Profile,
    tx: Sender<Cmd>,
) -> std::result::Result<notify::RecommendedWatcher, String> {
    let root = PathBuf::from(profile.local_path.trim());
    let ignore = Ignore::new(profile).map_err(|e| e.to_string())?;
    let watch_root = root.clone();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, EventKind::Access(_)) {
            return;
        }
        let relevant = ev.paths.iter().any(|p| match rel_of(&watch_root, p) {
            Some(rel) => !ignore.is_ignored(&rel),
            None => false,
        });
        if relevant {
            let _ = tx.send(Cmd::LocalChange);
        }
    })
    .map_err(|e| e.to_string())?;
    w.watch(&root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    Ok(w)
}

/// 프로필 하나의 작업 스레드 루프
fn worker(
    mut engine: SyncEngine,
    idx: usize,
    rx: Receiver<Cmd>,
    sh: Arc<Mutex<Shared>>,
    poll: Duration,
    debounce: Duration,
) {
    let name = engine.profile().display_name();
    let mut next_poll = Instant::now();
    'outer: loop {
        let timeout = next_poll.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Cmd::LocalChange) => {
                // 연속 저장이 끝날 때까지 대기
                loop {
                    match rx.recv_timeout(debounce) {
                        Ok(Cmd::LocalChange) => continue,
                        Ok(Cmd::SyncNow) | Err(RecvTimeoutError::Timeout) => break,
                        Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break 'outer,
                    }
                }
            }
            Ok(Cmd::SyncNow) | Err(RecvTimeoutError::Timeout) => {}
        }
        sh.lock().unwrap().profiles[idx].syncing = true;
        let log_sh = sh.clone();
        let prefix = format!("[{name}] ");
        let result = engine.sync_once(&|line: &str| {
            let err = line.starts_with("오류");
            log_sh.lock().unwrap().push(format!("{prefix}{line}"), err);
        });
        let mut s = sh.lock().unwrap();
        let mut logs = Vec::new();
        {
            let st = &mut s.profiles[idx];
            st.syncing = false;
            st.last_sync = Some(now_str());
            match result {
                Ok(rep) => {
                    st.last_summary = rep.summary();
                    if rep.changes() > 0 || !rep.errors.is_empty() {
                        logs.push((
                            format!("{prefix}동기화 완료: {}", rep.summary()),
                            !rep.errors.is_empty(),
                        ));
                    }
                    st.last_error = rep.errors.first().cloned();
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    st.last_summary = "실패".into();
                    logs.push((format!("{prefix}동기화 실패: {msg}"), true));
                    st.last_error = Some(msg);
                }
            }
        }
        for (t, e) in logs {
            s.push(t, e);
        }
        drop(s);
        // 로컬 변경으로 인한 동기화 후에도 원격 확인 주기는 유지
        next_poll = Instant::now() + poll;
    }
    let mut s = sh.lock().unwrap();
    s.profiles[idx].syncing = false;
    s.profiles[idx].active = false;
}

impl SyncService {
    /// 켜져 있는 모든 프로필의 동기화를 시작합니다.
    /// 일부 프로필이 시작에 실패해도 나머지는 계속 동작하며, 실패 내용은 상태에 표시됩니다.
    pub fn start(cfg: AppConfig, data_dir: &Path) -> Result<Self> {
        let device = cfg.safe_device_name();
        let profiles: Vec<Profile> = cfg.enabled_profiles().cloned().collect();
        if profiles.is_empty() {
            bail!("켜져 있는 동기화 프로필이 없습니다");
        }
        let shared = Arc::new(Mutex::new(Shared {
            running: true,
            profiles: profiles
                .iter()
                .map(|p| ProfileStatus {
                    id: p.id.clone(),
                    name: p.display_name(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
        let poll = Duration::from_secs(cfg.poll_interval_secs.max(10));
        let debounce = Duration::from_millis(cfg.debounce_ms.clamp(200, 60_000));

        let mut txs = Vec::new();
        let mut threads = Vec::new();
        let mut watchers = Vec::new();
        let mut first_err = None;
        for (idx, p) in profiles.into_iter().enumerate() {
            let name = p.display_name();
            let engine = match SyncEngine::new(p.clone(), &device, data_dir) {
                Ok(e) => e,
                Err(e) => {
                    let msg = format!("{e:#}");
                    let mut s = shared.lock().unwrap();
                    s.profiles[idx].last_error = Some(msg.clone());
                    s.profiles[idx].last_summary = "시작 실패".into();
                    s.push(format!("[{name}] 시작하지 못했습니다: {msg}"), true);
                    first_err.get_or_insert(e);
                    continue;
                }
            };
            let (tx, rx) = channel::<Cmd>();
            if cfg.watch_local {
                match start_watcher(&p, tx.clone()) {
                    Ok(w) => watchers.push(w),
                    Err(e) => shared.lock().unwrap().push(
                        format!("[{name}] 실시간 감시를 사용할 수 없어 주기 동기화만 합니다: {e}"),
                        true,
                    ),
                }
            }
            {
                let mut s = shared.lock().unwrap();
                s.profiles[idx].active = true;
                s.push(
                    format!(
                        "[{name}] 동기화 시작 · {} · {}",
                        p.backend.display_name(),
                        p.mode.display_name()
                    ),
                    false,
                );
            }
            let sh = shared.clone();
            let t = std::thread::Builder::new()
                .name(format!("obsidian-sync-{idx}"))
                .spawn(move || worker(engine, idx, rx, sh, poll, debounce))?;
            txs.push(tx);
            threads.push(t);
        }
        if threads.is_empty() {
            return Err(first_err.unwrap_or_else(|| anyhow::anyhow!("시작할 프로필이 없습니다")));
        }
        Ok(Self {
            txs,
            shared,
            threads,
            _watchers: watchers,
        })
    }

    pub fn sync_now(&self) {
        for tx in &self.txs {
            let _ = tx.send(Cmd::SyncNow);
        }
    }

    pub fn trigger(&self) -> Trigger {
        Trigger(self.txs.clone())
    }

    pub fn status(&self) -> ServiceStatus {
        let s = self.shared.lock().unwrap();
        let last = s
            .profiles
            .iter()
            .filter_map(|p| p.last_sync.as_ref().map(|t| (t, p)))
            .max_by(|a, b| a.0.cmp(b.0));
        let summary = if s.profiles.len() == 1 {
            s.profiles[0].last_summary.clone()
        } else {
            s.profiles
                .iter()
                .filter(|p| !p.last_summary.is_empty())
                .map(|p| format!("{} {}", p.name, p.last_summary))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        ServiceStatus {
            running: s.running,
            syncing: s.profiles.iter().any(|p| p.syncing),
            last_sync: last.map(|(t, _)| t.clone()),
            last_summary: summary,
            last_error: s.profiles.iter().find_map(|p| {
                p.last_error.as_ref().map(|e| {
                    if s.profiles.len() > 1 {
                        format!("[{}] {e}", p.name)
                    } else {
                        e.clone()
                    }
                })
            }),
            logs: s.logs.iter().cloned().collect(),
            profiles: s.profiles.clone(),
        }
    }

    /// 중지 (진행 중인 동기화가 끝날 때까지 대기)
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self._watchers.clear();
        for tx in &self.txs {
            let _ = tx.send(Cmd::Stop);
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        let mut s = self.shared.lock().unwrap();
        if s.running {
            s.running = false;
            s.push("동기화 중지".into(), false);
        }
    }
}

impl Drop for SyncService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 켜진 프로필을 한 번씩 동기화 (CLI/Android 예비 작업용).
/// 프로필별 실패는 보고서의 `errors`에 담깁니다.
pub fn sync_once(cfg: AppConfig, data_dir: &Path, log: &dyn Fn(&str)) -> Result<SyncReport> {
    let device = cfg.safe_device_name();
    let profiles: Vec<Profile> = cfg.enabled_profiles().cloned().collect();
    if profiles.is_empty() {
        bail!("켜져 있는 동기화 프로필이 없습니다");
    }
    let started = Instant::now();
    let mut total = SyncReport::default();
    for p in profiles {
        let name = p.display_name();
        let prefix = format!("[{name}] ");
        let res = SyncEngine::new(p, &device, data_dir)
            .and_then(|mut e| e.sync_once(&|l| log(&format!("{prefix}{l}"))));
        match res {
            Ok(r) => {
                total.uploaded += r.uploaded;
                total.downloaded += r.downloaded;
                total.deleted_local += r.deleted_local;
                total.deleted_remote += r.deleted_remote;
                total.conflicts += r.conflicts;
                total
                    .errors
                    .extend(r.errors.into_iter().map(|e| format!("{prefix}{e}")));
            }
            Err(e) => total.errors.push(format!("{prefix}{e:#}")),
        }
    }
    total.duration_ms = started.elapsed().as_millis() as u64;
    Ok(total)
}

/// 프로필 하나의 동기화 기록 초기화
pub fn reset_state(cfg: &AppConfig, profile_id: &str, data_dir: &Path) -> Result<()> {
    let p = cfg
        .profile(profile_id)
        .ok_or_else(|| anyhow::anyhow!("프로필을 찾을 수 없습니다: {profile_id}"))?;
    let mut engine = SyncEngine::new(p.clone(), &cfg.safe_device_name(), data_dir)?;
    engine.reset_state()
}

/// 연결 테스트: 원격 목록을 한 번 조회하고 파일 개수를 반환
pub fn test_connection(cfg: &BackendConfig, data_dir: &Path) -> Result<usize> {
    let mut backend = crate::backend::create_backend(cfg, data_dir)?;
    backend.check()?;
    Ok(backend.list()?.len())
}
