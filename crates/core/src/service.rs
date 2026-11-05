//! 백그라운드 동기화 서비스: 실시간 파일 감시 + 주기적 원격 확인
use crate::config::AppConfig;
use crate::engine::SyncEngine;
use crate::scan::{rel_of, Ignore};
use anyhow::Result;
use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
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

#[derive(Clone, Debug, Serialize, Default)]
pub struct ServiceStatus {
    pub running: bool,
    pub syncing: bool,
    pub last_sync: Option<String>,
    pub last_summary: String,
    pub last_error: Option<String>,
    pub logs: Vec<LogLine>,
}

#[derive(Default)]
struct Shared {
    running: bool,
    syncing: bool,
    last_sync: Option<String>,
    last_summary: String,
    last_error: Option<String>,
    logs: VecDeque<LogLine>,
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
    tx: Sender<Cmd>,
    shared: Arc<Mutex<Shared>>,
    thread: Option<JoinHandle<()>>,
    _watcher: Option<notify::RecommendedWatcher>,
}

/// 서비스 외부(트레이 메뉴 등)에서 "지금 동기화"를 요청할 때 사용하는 핸들
#[derive(Clone)]
pub struct Trigger(Sender<Cmd>);
impl Trigger {
    pub fn sync_now(&self) {
        let _ = self.0.send(Cmd::SyncNow);
    }
}

impl SyncService {
    pub fn start(cfg: AppConfig, data_dir: &Path) -> Result<Self> {
        let mut engine = SyncEngine::new(cfg.clone(), data_dir)?;
        let (tx, rx) = channel::<Cmd>();
        let shared = Arc::new(Mutex::new(Shared {
            running: true,
            ..Default::default()
        }));
        shared
            .lock()
            .unwrap()
            .push(format!("동기화 시작: {}", cfg.backend.display_name()), false);

        // 실시간 파일 감시
        let watcher = if cfg.watch_local {
            let vault = PathBuf::from(cfg.vault_path.trim());
            let ignore = Ignore::new(&cfg)?;
            let wtx = tx.clone();
            let root = vault.clone();
            let w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(ev) = res else { return };
                if matches!(ev.kind, EventKind::Access(_)) {
                    return;
                }
                let relevant = ev.paths.iter().any(|p| match rel_of(&root, p) {
                    Some(rel) => !ignore.is_ignored(&rel),
                    None => false,
                });
                if relevant {
                    let _ = wtx.send(Cmd::LocalChange);
                }
            });
            match w {
                Ok(mut w) => match w.watch(&vault, RecursiveMode::Recursive) {
                    Ok(()) => Some(w),
                    Err(e) => {
                        shared
                            .lock()
                            .unwrap()
                            .push(format!("실시간 감시를 사용할 수 없어 주기 동기화만 합니다: {e}"), true);
                        None
                    }
                },
                Err(e) => {
                    shared
                        .lock()
                        .unwrap()
                        .push(format!("실시간 감시 초기화 실패: {e}"), true);
                    None
                }
            }
        } else {
            None
        };

        let poll = Duration::from_secs(cfg.poll_interval_secs.max(10));
        let debounce = Duration::from_millis(cfg.debounce_ms.clamp(200, 60_000));
        let sh = shared.clone();
        let thread = std::thread::Builder::new()
            .name("obsidian-sync".into())
            .spawn(move || {
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
                                    Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => {
                                        break 'outer
                                    }
                                }
                            }
                        }
                        Ok(Cmd::SyncNow) | Err(RecvTimeoutError::Timeout) => {}
                    }
                    sh.lock().unwrap().syncing = true;
                    let log_sh = sh.clone();
                    let result = engine.sync_once(&|line: &str| {
                        let err = line.starts_with("오류");
                        log_sh.lock().unwrap().push(line.to_string(), err);
                    });
                    let mut s = sh.lock().unwrap();
                    s.syncing = false;
                    s.last_sync = Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
                    match result {
                        Ok(rep) => {
                            s.last_summary = rep.summary();
                            if rep.changes() > 0 || !rep.errors.is_empty() {
                                let summary = format!("동기화 완료: {}", rep.summary());
                                s.push(summary, !rep.errors.is_empty());
                            }
                            s.last_error = rep.errors.first().cloned();
                        }
                        Err(e) => {
                            let msg = format!("{e:#}");
                            s.last_summary = "실패".into();
                            s.push(format!("동기화 실패: {msg}"), true);
                            s.last_error = Some(msg);
                        }
                    }
                    drop(s);
                    // 로컬 변경으로 인한 동기화 후에도 원격 확인 주기는 유지
                    next_poll = Instant::now() + poll;
                }
                let mut s = sh.lock().unwrap();
                s.running = false;
                s.syncing = false;
                s.push("동기화 중지".into(), false);
            })?;

        Ok(Self {
            tx,
            shared,
            thread: Some(thread),
            _watcher: watcher,
        })
    }

    pub fn sync_now(&self) {
        let _ = self.tx.send(Cmd::SyncNow);
    }

    pub fn trigger(&self) -> Trigger {
        Trigger(self.tx.clone())
    }

    pub fn status(&self) -> ServiceStatus {
        let s = self.shared.lock().unwrap();
        ServiceStatus {
            running: s.running,
            syncing: s.syncing,
            last_sync: s.last_sync.clone(),
            last_summary: s.last_summary.clone(),
            last_error: s.last_error.clone(),
            logs: s.logs.iter().cloned().collect(),
        }
    }

    /// 중지 (진행 중인 동기화가 끝날 때까지 대기)
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self._watcher = None;
        let _ = self.tx.send(Cmd::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for SyncService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 한 번만 동기화 (CLI/테스트용)
pub fn sync_once(cfg: AppConfig, data_dir: &Path, log: &dyn Fn(&str)) -> Result<crate::SyncReport> {
    let mut engine = SyncEngine::new(cfg, data_dir)?;
    engine.sync_once(log)
}

/// 동기화 기록 초기화
pub fn reset_state(cfg: AppConfig, data_dir: &Path) -> Result<()> {
    let mut engine = SyncEngine::new(cfg, data_dir)?;
    engine.reset_state()
}

/// 연결 테스트: 원격 목록을 한 번 조회하고 파일 개수를 반환
pub fn test_connection(cfg: &crate::BackendConfig, data_dir: &Path) -> Result<usize> {
    let mut backend = crate::backend::create_backend(cfg, data_dir)?;
    Ok(backend.list()?.len())
}
