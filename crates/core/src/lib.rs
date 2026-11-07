//! Obsidian Sync 공통 코어.
//!
//! Windows 데스크톱 앱과 Android 앱이 함께 사용하는 동기화 엔진입니다.
//! - [`config`]: 설정 구조 (프로필 = 로컬 폴더 1개 ↔ 원격 폴더 1개)
//! - [`backend`]: 플러그인 방식의 원격 저장소 (Synology File Station, 로컬 폴더, S3 호환, OneDrive, Google Drive)
//! - [`engine`]: 3-way 비교 기반 양방향 동기화
//! - [`service`]: 파일 감시 + 주기 동기화 백그라운드 서비스

pub mod auth;
pub mod backend;
pub mod config;
pub mod engine;
pub mod http;
pub mod scan;
pub mod service;
pub mod state;
mod xml;

pub use config::{AppConfig, BackendConfig, ConflictPolicy, Profile, ProfileKind, SyncMode};
pub use engine::{MassDeletePending, SyncEngine, SyncReport};
pub use service::{LogLine, ProfileStatus, ServiceStatus, SyncService};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
