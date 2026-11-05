//! OAuth 2.0 디바이스 코드 로그인 (OneDrive / Google Drive)
//!
//! 디바이스 코드 방식은 Windows와 Android에서 똑같이 동작합니다.
//! 앱이 짧은 코드를 보여주면 사용자가 브라우저에서 코드를 입력해 로그인합니다.

use crate::config::BackendConfig;
use crate::http::{self, HttpError};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OAuthToken {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// 만료 시각 (unix 초)
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    #[serde(alias = "verification_url")]
    pub verification_uri: String,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default = "default_expires")]
    pub expires_in: u64,
}
fn default_interval() -> u64 {
    5
}
fn default_expires() -> u64 {
    900
}

#[derive(Clone, Debug)]
pub struct OAuthProvider {
    pub name: &'static str,
    pub device_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl OAuthProvider {
    pub fn microsoft(client_id: &str, tenant: &str) -> Self {
        let tenant = if tenant.trim().is_empty() {
            "common"
        } else {
            tenant.trim()
        };
        Self {
            name: "onedrive",
            device_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/devicecode"),
            token_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"),
            client_id: client_id.trim().to_string(),
            client_secret: None,
            scope: "Files.ReadWrite offline_access".into(),
        }
    }

    pub fn google(client_id: &str, client_secret: &str) -> Self {
        Self {
            name: "gdrive",
            device_url: "https://oauth2.googleapis.com/device/code".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            client_id: client_id.trim().to_string(),
            client_secret: Some(client_secret.trim().to_string()),
            // drive.file: 이 앱이 만든 파일만 접근 (디바이스 코드 방식에서 허용되는 범위)
            scope: "https://www.googleapis.com/auth/drive.file".into(),
        }
    }

    pub fn for_backend(cfg: &BackendConfig) -> Option<Self> {
        match cfg {
            BackendConfig::OneDrive {
                client_id, tenant, ..
            } => Some(Self::microsoft(client_id, tenant)),
            BackendConfig::GoogleDrive {
                client_id,
                client_secret,
                ..
            } => Some(Self::google(client_id, client_secret)),
            _ => None,
        }
    }

    fn base_form(&self) -> Vec<(&str, &str)> {
        let mut f = vec![("client_id", self.client_id.as_str())];
        if let Some(s) = &self.client_secret {
            f.push(("client_secret", s.as_str()));
        }
        f
    }

    /// 1단계: 디바이스 코드 발급
    pub fn start_device(&self) -> Result<DeviceCode> {
        let agent = http::agent();
        let form = vec![
            ("client_id", self.client_id.as_str()),
            ("scope", self.scope.as_str()),
        ];
        let resp = http::check(agent.post(&self.device_url).send_form(&form))
            .context("로그인 코드 발급 실패 (Client ID를 확인하세요)")?;
        Ok(resp.into_json()?)
    }

    /// 2단계: 사용자가 브라우저에서 승인할 때까지 대기
    pub fn poll_device(&self, dc: &DeviceCode, cancel: &AtomicBool) -> Result<OAuthToken> {
        let agent = http::agent();
        let deadline = Instant::now() + Duration::from_secs(dc.expires_in.max(60));
        let mut interval = dc.interval.max(1);
        loop {
            for _ in 0..interval * 10 {
                if cancel.load(Ordering::Relaxed) {
                    bail!("로그인이 취소되었습니다");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if Instant::now() > deadline {
                bail!("로그인 코드가 만료되었습니다. 다시 시도해 주세요");
            }
            let mut form = self.base_form();
            form.push(("grant_type", "urn:ietf:params:oauth:grant-type:device_code"));
            form.push(("device_code", dc.device_code.as_str()));
            match http::check(agent.post(&self.token_url).send_form(&form)) {
                Ok(resp) => {
                    let t: TokenResponse = resp.into_json()?;
                    return Ok(OAuthToken {
                        access_token: t.access_token,
                        refresh_token: t.refresh_token,
                        expires_at: now() + t.expires_in.unwrap_or(3600),
                    });
                }
                Err(e) => {
                    let body = e
                        .downcast_ref::<HttpError>()
                        .map(|h| h.body.clone())
                        .unwrap_or_default();
                    let code = serde_json::from_str::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|v| v.get("error").and_then(|x| x.as_str()).map(String::from))
                        .unwrap_or_default();
                    match code.as_str() {
                        "authorization_pending" => continue,
                        "slow_down" => {
                            interval += 5;
                            continue;
                        }
                        "access_denied" | "authorization_declined" => {
                            bail!("로그인이 거부되었습니다")
                        }
                        "expired_token" => bail!("로그인 코드가 만료되었습니다"),
                        _ => return Err(e.context("로그인 실패")),
                    }
                }
            }
        }
    }

    pub fn refresh(&self, token: &OAuthToken) -> Result<OAuthToken> {
        let rt = token
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow!("갱신 토큰이 없습니다. 다시 로그인해 주세요"))?;
        let agent = http::agent();
        let mut form = self.base_form();
        form.push(("grant_type", "refresh_token"));
        form.push(("refresh_token", rt));
        if self.name == "onedrive" {
            form.push(("scope", self.scope.as_str()));
        }
        let resp = http::check(agent.post(&self.token_url).send_form(&form))
            .context("로그인 갱신 실패. 다시 로그인해 주세요")?;
        let t: TokenResponse = resp.into_json()?;
        Ok(OAuthToken {
            access_token: t.access_token,
            // 갱신 응답에 새 refresh_token이 없으면 기존 것을 유지
            refresh_token: t.refresh_token.or_else(|| token.refresh_token.clone()),
            expires_at: now() + t.expires_in.unwrap_or(3600),
        })
    }
}

/// 토큰 파일 경로 (저장소 종류 + client id 별)
pub fn token_path(data_dir: &Path, cfg: &BackendConfig) -> Option<PathBuf> {
    let p = OAuthProvider::for_backend(cfg)?;
    let id: String = p
        .client_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    Some(data_dir.join(format!("token-{}-{}.json", p.name, id)))
}

pub fn load_token(path: &Path) -> Option<OAuthToken> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn save_token(path: &Path, t: &OAuthToken) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string(t)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// 로그인 여부
pub fn is_logged_in(data_dir: &Path, cfg: &BackendConfig) -> bool {
    token_path(data_dir, cfg)
        .and_then(|p| load_token(&p))
        .is_some()
}

pub fn logout(data_dir: &Path, cfg: &BackendConfig) {
    if let Some(p) = token_path(data_dir, cfg) {
        let _ = std::fs::remove_file(p);
    }
}

/// 전체 로그인 흐름: 코드 발급 → `on_code` 콜백으로 사용자에게 표시 → 승인 대기 → 토큰 저장
pub fn login_device(
    data_dir: &Path,
    cfg: &BackendConfig,
    on_code: impl FnOnce(&DeviceCode),
    cancel: &AtomicBool,
) -> Result<()> {
    let provider =
        OAuthProvider::for_backend(cfg).ok_or_else(|| anyhow!("로그인이 필요 없는 저장소입니다"))?;
    cfg.validate()?;
    let dc = provider.start_device()?;
    on_code(&dc);
    let token = provider.poll_device(&dc, cancel)?;
    save_token(&token_path(data_dir, cfg).unwrap(), &token)?;
    Ok(())
}

/// 백엔드가 사용하는 토큰 관리자 (자동 갱신 + 저장)
pub struct TokenManager {
    provider: OAuthProvider,
    path: PathBuf,
    token: Option<OAuthToken>,
}

impl TokenManager {
    pub fn new(data_dir: &Path, cfg: &BackendConfig) -> Result<Self> {
        let provider = OAuthProvider::for_backend(cfg).ok_or_else(|| anyhow!("OAuth 저장소 아님"))?;
        let path = token_path(data_dir, cfg).unwrap();
        let token = load_token(&path);
        Ok(Self {
            provider,
            path,
            token,
        })
    }

    pub fn access_token(&mut self) -> Result<String> {
        if self.token.is_none() {
            self.token = load_token(&self.path);
        }
        let tok = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow!("로그인이 필요합니다. 설정에서 '로그인' 버튼을 눌러 주세요"))?;
        if tok.expires_at - 120 > now() {
            return Ok(tok.access_token.clone());
        }
        let fresh = self.provider.refresh(tok)?;
        save_token(&self.path, &fresh)?;
        let at = fresh.access_token.clone();
        self.token = Some(fresh);
        Ok(at)
    }

    /// 401 응답 시 강제 갱신
    pub fn invalidate(&mut self) {
        if let Some(t) = &mut self.token {
            t.expires_at = 0;
        }
    }
}
