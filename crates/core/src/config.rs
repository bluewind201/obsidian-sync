use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 앱 전체 설정. Windows/Android 모두 같은 JSON 형식을 사용합니다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppConfig {
    /// Obsidian 볼트(저장소) 폴더 경로
    #[serde(default)]
    pub vault_path: String,
    /// 원격 저장소 설정
    #[serde(default)]
    pub backend: BackendConfig,
    /// 원격 변경 확인 주기(초)
    #[serde(default = "default_poll")]
    pub poll_interval_secs: u64,
    /// 로컬 변경 후 동기화까지 대기 시간(ms). 연속 저장을 묶어 처리합니다.
    #[serde(default = "default_debounce")]
    pub debounce_ms: u64,
    /// `.obsidian` 설정 폴더(플러그인, 테마 등)도 동기화할지 여부
    #[serde(default = "default_true")]
    pub sync_obsidian_folder: bool,
    /// 한쪽에서 삭제된 파일을 다른 쪽에서도 삭제할지 여부
    #[serde(default = "default_true")]
    pub propagate_deletes: bool,
    /// 실시간 파일 감시 사용 여부
    #[serde(default = "default_true")]
    pub watch_local: bool,
    /// 앱 실행 시 자동으로 동기화 시작
    #[serde(default = "default_true")]
    pub start_on_launch: bool,
    /// 충돌 파일 이름에 붙일 기기 이름 (비우면 자동)
    #[serde(default)]
    pub device_name: String,
    /// 추가 제외 패턴 (glob, 볼트 기준 상대경로. 예: `Private/**`, `*.pdf`)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

fn default_poll() -> u64 {
    60
}
fn default_debounce() -> u64 {
    1500
}
fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        serde_json::from_str("{}").expect("default config")
    }
}

/// 원격 저장소 종류별 설정
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendConfig {
    /// 로컬/네트워크 폴더 (Synology SMB 공유폴더, Synology Drive 클라이언트 폴더, USB 등)
    LocalFolder { path: String },
    /// WebDAV (Synology WebDAV Server, Nextcloud 등)
    WebDav {
        url: String,
        username: String,
        password: String,
    },
    /// S3 호환 오브젝트 스토리지 (네이버 클라우드 Object Storage, Google Cloud Storage(HMAC), Synology C2, AWS S3, MinIO 등)
    S3 {
        endpoint: String,
        region: String,
        bucket: String,
        #[serde(default)]
        prefix: String,
        access_key: String,
        secret_key: String,
    },
    /// Microsoft OneDrive (Graph API)
    OneDrive {
        client_id: String,
        #[serde(default = "default_tenant")]
        tenant: String,
        #[serde(default = "default_folder")]
        folder: String,
    },
    /// Google Drive
    GoogleDrive {
        client_id: String,
        client_secret: String,
        #[serde(default = "default_folder")]
        folder: String,
    },
}

fn default_tenant() -> String {
    "common".into()
}
pub fn default_folder() -> String {
    "ObsidianSync".into()
}

impl Default for BackendConfig {
    fn default() -> Self {
        BackendConfig::LocalFolder {
            path: String::new(),
        }
    }
}

impl BackendConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            BackendConfig::LocalFolder { .. } => "local_folder",
            BackendConfig::WebDav { .. } => "web_dav",
            BackendConfig::S3 { .. } => "s3",
            BackendConfig::OneDrive { .. } => "one_drive",
            BackendConfig::GoogleDrive { .. } => "google_drive",
        }
    }

    /// 사람이 읽을 수 있는 이름
    pub fn display_name(&self) -> &'static str {
        match self {
            BackendConfig::LocalFolder { .. } => "로컬/NAS 폴더",
            BackendConfig::WebDav { .. } => "WebDAV (Synology 등)",
            BackendConfig::S3 { .. } => "S3 호환 (네이버클라우드/GCS 등)",
            BackendConfig::OneDrive { .. } => "OneDrive",
            BackendConfig::GoogleDrive { .. } => "Google Drive",
        }
    }

    /// OAuth 로그인이 필요한 저장소인지
    pub fn needs_oauth(&self) -> bool {
        matches!(
            self,
            BackendConfig::OneDrive { .. } | BackendConfig::GoogleDrive { .. }
        )
    }

    pub fn validate(&self) -> Result<()> {
        fn req(v: &str, name: &str) -> Result<()> {
            if v.trim().is_empty() {
                bail!("{name} 항목을 입력해 주세요");
            }
            Ok(())
        }
        match self {
            BackendConfig::LocalFolder { path } => req(path, "대상 폴더"),
            BackendConfig::WebDav { url, .. } => {
                req(url, "WebDAV 주소")?;
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    bail!("WebDAV 주소는 http:// 또는 https:// 로 시작해야 합니다");
                }
                Ok(())
            }
            BackendConfig::S3 {
                endpoint,
                region,
                bucket,
                access_key,
                secret_key,
                ..
            } => {
                req(endpoint, "엔드포인트")?;
                req(region, "리전")?;
                req(bucket, "버킷")?;
                req(access_key, "Access Key")?;
                req(secret_key, "Secret Key")
            }
            BackendConfig::OneDrive {
                client_id, folder, ..
            } => {
                req(client_id, "Client ID")?;
                req(folder, "폴더")
            }
            BackendConfig::GoogleDrive {
                client_id,
                client_secret,
                folder,
            } => {
                req(client_id, "Client ID")?;
                req(client_secret, "Client Secret")?;
                req(folder, "폴더")
            }
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("설정 파일을 읽을 수 없습니다: {}", path.display()))?;
        Ok(serde_json::from_str(&text).context("설정 파일 형식이 잘못되었습니다")?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.vault_path.trim().is_empty() {
            bail!("Obsidian 볼트 폴더를 선택해 주세요");
        }
        if !Path::new(&self.vault_path).is_dir() {
            bail!("볼트 폴더가 존재하지 않습니다: {}", self.vault_path);
        }
        self.backend.validate()
    }

    pub fn effective_device_name(&self) -> String {
        if !self.device_name.trim().is_empty() {
            return self.device_name.trim().to_string();
        }
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| {
                if cfg!(target_os = "android") {
                    "android".into()
                } else {
                    "device".into()
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrip() {
        let c = AppConfig::default();
        assert_eq!(c.poll_interval_secs, 60);
        assert!(c.sync_obsidian_folder);
        let s = serde_json::to_string(&c).unwrap();
        let c2: AppConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn backend_json() {
        let j = r#"{"vault_path":"x","backend":{"type":"s3","endpoint":"https://kr.object.ncloudstorage.com","region":"kr-standard","bucket":"b","access_key":"a","secret_key":"s"}}"#;
        let c: AppConfig = serde_json::from_str(j).unwrap();
        assert_eq!(c.backend.kind(), "s3");
    }
}
