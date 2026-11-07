use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// 앱 전체 설정. Windows/Android 모두 같은 JSON 형식을 사용합니다.
///
/// 동기화 대상은 [`Profile`] 목록으로 관리합니다. 프로필 하나가
/// "내 기기 폴더 1개 ↔ 원격 폴더 1개" 의 1:1 연결입니다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppConfig {
    /// 동기화 프로필 목록
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// 원격 변경 확인 주기(초)
    #[serde(default = "default_poll")]
    pub poll_interval_secs: u64,
    /// 로컬 변경 후 동기화까지 대기 시간(ms). 연속 저장을 묶어 처리합니다.
    #[serde(default = "default_debounce")]
    pub debounce_ms: u64,
    /// 실시간 파일 감시 사용 여부
    #[serde(default = "default_true")]
    pub watch_local: bool,
    /// 앱 실행 시 자동으로 동기화 시작
    #[serde(default = "default_true")]
    pub start_on_launch: bool,
    /// 충돌 파일 이름에 붙일 기기 이름 (비우면 자동)
    #[serde(default)]
    pub device_name: String,
}

/// 동기화 프로필: 로컬 폴더 1개 ↔ 원격 폴더 1개
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Profile {
    /// 고유 ID (동기화 기록 구분용, 바꾸지 않음)
    #[serde(default = "new_profile_id")]
    pub id: String,
    /// 화면에 보이는 이름 (예: "Obsidian", "사진")
    #[serde(default)]
    pub name: String,
    /// 이 프로필을 동기화할지 여부
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 폴더 종류
    #[serde(default)]
    pub kind: ProfileKind,
    /// 내 기기의 폴더 경로
    #[serde(default)]
    pub local_path: String,
    /// 원격 저장소
    #[serde(default)]
    pub backend: BackendConfig,
    /// 동기화 방향
    #[serde(default)]
    pub mode: SyncMode,
    /// 한쪽에서 삭제된 파일을 다른 쪽에서도 삭제할지 여부 (양방향 모드에서만 사용)
    #[serde(default = "default_true")]
    pub propagate_deletes: bool,
    /// 한 방향 모드에서 원본 쪽에서 지운 파일을 대상 쪽에서도 지울지 여부
    /// (올리기만: 내 기기에서 지우면 원격도 삭제 / 내려받기만: 원격에서 지우면 내 기기도 삭제).
    /// Synology Cloud Sync의 "원본에서 삭제된 파일을 대상에서 삭제하지 않음" 옵션의 반대입니다.
    #[serde(default)]
    pub delete_in_target: bool,
    /// 데이터 기준: 양쪽이 모두 바뀌었거나 처음 연결할 때 양쪽에 같은 이름의 다른 파일이 있을 때
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
    /// `.obsidian` 설정 폴더(플러그인, 테마 등)도 동기화할지 여부 (Obsidian 볼트에서만 사용)
    #[serde(default = "default_true")]
    pub sync_obsidian_folder: bool,
    /// 추가 제외 패턴 (glob, 폴더 기준 상대경로. 예: `Private/**`, `*.pdf`)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    /// Obsidian 볼트 (`.obsidian` 설정 폴더 처리)
    #[default]
    Obsidian,
    /// 일반 폴더 (사진, 문서 등)
    Folder,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// 양방향 동기화
    #[default]
    TwoWay,
    /// 올리기만 (백업): 내 기기 → 원격. 원격 변경은 내려받지 않음
    UploadOnly,
    /// 내려받기만: 원격 → 내 기기. 내 기기 변경은 올리지 않음
    DownloadOnly,
}

/// 데이터 기준 (충돌 시 어느 쪽을 따를지)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// 둘 다 보존: 원격본을 원래 이름으로, 내 기기 쪽은 `(충돌 …)` 사본으로 남김 (Synology Drive 방식)
    #[default]
    KeepBoth,
    /// 내 기기 기준: 내 기기 내용으로 원격을 덮어씀
    PreferLocal,
    /// 원격 기준: 원격 내용으로 내 기기를 덮어씀 (덮어쓰기 전 파일은 `.trash`에 보관)
    PreferRemote,
}

impl ConflictPolicy {
    pub fn display_name(&self) -> &'static str {
        match self {
            ConflictPolicy::KeepBoth => "둘 다 보존",
            ConflictPolicy::PreferLocal => "내 기기 기준",
            ConflictPolicy::PreferRemote => "저장소 기준",
        }
    }
}

impl SyncMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            SyncMode::TwoWay => "양방향 동기화",
            SyncMode::UploadOnly => "올리기만 (백업)",
            SyncMode::DownloadOnly => "내려받기만",
        }
    }
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

/// 새 프로필 ID (시간 기반, 충분히 고유함)
pub fn new_profile_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let h = crate::engine::hash_bytes(format!("{nanos}-{seq}-{}", std::process::id()).as_bytes());
    format!("p{}", &h[..10])
}

impl Default for AppConfig {
    fn default() -> Self {
        serde_json::from_str("{}").expect("default config")
    }
}

impl Default for Profile {
    fn default() -> Self {
        serde_json::from_str("{}").expect("default profile")
    }
}

/// 원격 저장소 종류별 설정
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendConfig {
    /// Synology NAS (DSM File Station API)
    Synology {
        /// DSM 접속 주소 (예: `https://nas.example.com:5001`)
        url: String,
        username: String,
        password: String,
        /// NAS 안의 폴더 (공유 폴더부터 시작, 예: `/home/Obsidian`, `/photo/휴대폰`)
        folder: String,
    },
    /// 로컬/네트워크 폴더 (SMB 공유폴더, Synology Drive 클라이언트 폴더, USB 등)
    LocalFolder { path: String },
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
        BackendConfig::Synology {
            url: String::new(),
            username: String::new(),
            password: String::new(),
            // 폴더를 고르기 전 기본값 (NAS에서 고르기로 바꿀 수 있음)
            folder: "/home".into(),
        }
    }
}

impl BackendConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            BackendConfig::Synology { .. } => "synology",
            BackendConfig::LocalFolder { .. } => "local_folder",
            BackendConfig::S3 { .. } => "s3",
            BackendConfig::OneDrive { .. } => "one_drive",
            BackendConfig::GoogleDrive { .. } => "google_drive",
        }
    }

    /// 사람이 읽을 수 있는 이름
    pub fn display_name(&self) -> &'static str {
        match self {
            BackendConfig::Synology { .. } => "Synology NAS",
            BackendConfig::LocalFolder { .. } => "로컬/NAS 폴더",
            BackendConfig::S3 { .. } => "S3 호환 (네이버클라우드/GCS 등)",
            BackendConfig::OneDrive { .. } => "OneDrive",
            BackendConfig::GoogleDrive { .. } => "Google Drive",
        }
    }

    /// OAuth(디바이스 코드) 로그인이 필요한 저장소인지
    pub fn needs_oauth(&self) -> bool {
        matches!(
            self,
            BackendConfig::OneDrive { .. } | BackendConfig::GoogleDrive { .. }
        )
    }

    /// 한 줄 요약 (예: "Synology · nas.example.com:5001 /home/Obsidian")
    pub fn summary(&self) -> String {
        match self {
            BackendConfig::Synology { url, folder, .. } => {
                let host = url
                    .split_once("://")
                    .map(|x| x.1)
                    .unwrap_or(url)
                    .trim_end_matches('/');
                format!("Synology · {host} {folder}")
            }
            BackendConfig::LocalFolder { path } => format!("폴더 · {path}"),
            BackendConfig::S3 {
                endpoint,
                bucket,
                prefix,
                ..
            } => format!("S3 · {endpoint} / {bucket}/{prefix}"),
            BackendConfig::OneDrive { folder, .. } => format!("OneDrive · /{folder}"),
            BackendConfig::GoogleDrive { folder, .. } => format!("Google Drive · /{folder}"),
        }
    }

    pub fn validate(&self) -> Result<()> {
        fn req(v: &str, name: &str) -> Result<()> {
            if v.trim().is_empty() {
                bail!("{name} 항목을 입력해 주세요");
            }
            Ok(())
        }
        match self {
            BackendConfig::Synology {
                url,
                username,
                password,
                folder,
            } => {
                req(url, "NAS 주소")?;
                let u = url.trim();
                if !(u.starts_with("http://") || u.starts_with("https://")) {
                    bail!("NAS 주소는 http:// 또는 https:// 로 시작해야 합니다 (예: https://nas.example.com:5001)");
                }
                req(username, "NAS 아이디")?;
                req(password, "NAS 비밀번호")?;
                req(folder, "NAS 폴더")?;
                let f = folder.trim();
                if !f.starts_with('/') || f.trim_matches('/').is_empty() {
                    bail!("NAS 폴더는 공유 폴더부터 / 로 시작해야 합니다 (예: /home/Obsidian)");
                }
                if f.split('/').any(|s| s == "." || s == "..") {
                    bail!("NAS 폴더 경로가 올바르지 않습니다: {f}");
                }
                Ok(())
            }
            BackendConfig::LocalFolder { path } => req(path, "대상 폴더"),
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

impl Profile {
    /// 화면 표시용 이름 (비어 있으면 폴더 이름)
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        let p = self.local_path.trim().trim_end_matches(['/', '\\']);
        let last = p.rsplit(['/', '\\']).next().unwrap_or("");
        if last.is_empty() {
            "이름 없음".into()
        } else {
            last.to_string()
        }
    }

    /// 설정 검사. `check_dir`가 true면 로컬 폴더가 실제로 있는지도 확인
    pub fn validate(&self, check_dir: bool) -> Result<()> {
        let name = self.display_name();
        if self.local_path.trim().is_empty() {
            bail!("[{name}] 내 기기의 폴더를 선택해 주세요");
        }
        if check_dir && !Path::new(self.local_path.trim()).is_dir() {
            bail!(
                "[{name}] 폴더가 존재하지 않습니다: {}",
                self.local_path.trim()
            );
        }
        self.backend
            .validate()
            .with_context(|| format!("[{name}] 저장소 설정"))?;
        if let BackendConfig::LocalFolder { path } = &self.backend {
            if paths_overlap(path, &self.local_path) {
                bail!("[{name}] 저장 위치와 내 폴더가 같거나 서로 안에 있습니다");
            }
        }
        Ok(())
    }
}

fn norm_path(p: &str) -> String {
    let mut s = p.trim().replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    s
}

/// 두 경로가 같거나 한쪽이 다른 쪽 안에 있는지
pub fn paths_overlap(a: &str, b: &str) -> bool {
    let (a, b) = (norm_path(a), norm_path(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let inside =
        |x: &str, y: &str| x == y || x.starts_with(&format!("{}/", y.trim_end_matches('/')));
    inside(&a, &b) || inside(&b, &a)
}

/// 이전 버전(볼트 1개 + WebDAV) 설정을 현재 형식으로 바꿉니다.
fn migrate(mut v: Value) -> Value {
    let Some(obj) = v.as_object_mut() else {
        return v;
    };
    let legacy = obj.contains_key("vault_path") || obj.contains_key("backend");
    if legacy && !obj.contains_key("profiles") {
        let mut p = serde_json::Map::new();
        p.insert("id".into(), Value::String("default".into()));
        p.insert("name".into(), Value::String("Obsidian".into()));
        p.insert("kind".into(), Value::String("obsidian".into()));
        if let Some(x) = obj.remove("vault_path") {
            p.insert("local_path".into(), x);
        }
        for key in [
            "backend",
            "propagate_deletes",
            "sync_obsidian_folder",
            "ignore_patterns",
        ] {
            if let Some(x) = obj.remove(key) {
                p.insert(key.into(), x);
            }
        }
        obj.insert("profiles".into(), Value::Array(vec![Value::Object(p)]));
    }
    if let Some(Value::Array(list)) = obj.get_mut("profiles") {
        for p in list.iter_mut() {
            if let Some(b) = p.get_mut("backend") {
                migrate_webdav(b);
            }
        }
    }
    v
}

/// WebDAV(Synology) 설정 → Synology File Station 설정
/// `https://nas:5006/home/Obsidian` → 주소 `https://nas:5001`, 폴더 `/home/Obsidian`
fn migrate_webdav(b: &mut Value) {
    if b.get("type").and_then(Value::as_str) != Some("web_dav") {
        return;
    }
    let url = b.get("url").and_then(Value::as_str).unwrap_or("").trim();
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let hostport = match hostport.rsplit_once(':') {
        Some((h, "5006")) => format!("{h}:5001"),
        Some((h, "5005")) => format!("{h}:5000"),
        _ => hostport.to_string(),
    };
    let folder = percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .trim_end_matches('/')
        .to_string();
    let get = |k: &str| b.get(k).cloned().unwrap_or(Value::String(String::new()));
    *b = serde_json::json!({
        "type": "synology",
        "url": format!("{scheme}://{hostport}"),
        "username": get("username"),
        "password": get("password"),
        "folder": folder,
    });
}

impl AppConfig {
    /// JSON 문자열에서 읽기 (이전 형식 자동 변환)
    pub fn from_json(text: &str) -> Result<Self> {
        let v: Value = serde_json::from_str(text).context("설정 형식이 잘못되었습니다")?;
        let mut cfg: AppConfig =
            serde_json::from_value(migrate(v)).context("설정 형식이 잘못되었습니다")?;
        cfg.fix_ids();
        Ok(cfg)
    }

    /// 빈 ID나 중복 ID를 새 ID로 바꿈
    fn fix_ids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for p in &mut self.profiles {
            if p.id.trim().is_empty() || !seen.insert(p.id.clone()) {
                p.id = new_profile_id();
                seen.insert(p.id.clone());
            }
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("설정 파일을 읽을 수 없습니다: {}", path.display()))?;
        Self::from_json(&text)
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

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn enabled_profiles(&self) -> impl Iterator<Item = &Profile> {
        self.profiles.iter().filter(|p| p.enabled)
    }

    pub fn validate(&self) -> Result<()> {
        if self.profiles.is_empty() {
            bail!("동기화 프로필을 하나 이상 추가해 주세요");
        }
        let mut ids = std::collections::HashSet::new();
        for p in &self.profiles {
            if !ids.insert(p.id.as_str()) {
                bail!("프로필 ID가 중복되었습니다: {}", p.id);
            }
            p.validate(p.enabled)?;
        }
        let enabled: Vec<&Profile> = self.enabled_profiles().collect();
        if enabled.is_empty() {
            bail!("켜져 있는 프로필이 없습니다");
        }
        for (i, a) in enabled.iter().enumerate() {
            for b in &enabled[i + 1..] {
                if paths_overlap(&a.local_path, &b.local_path) {
                    bail!(
                        "'{}'와 '{}' 프로필의 폴더가 같거나 서로 안에 있습니다. 폴더가 겹치지 않게 해 주세요",
                        a.display_name(),
                        b.display_name()
                    );
                }
            }
        }
        Ok(())
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

    /// 충돌 파일 이름에 쓸 수 있게 정리한 기기 이름
    pub fn safe_device_name(&self) -> String {
        let s: String = self
            .effective_device_name()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if s.is_empty() {
            "device".into()
        } else {
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrip() {
        let c = AppConfig::default();
        assert_eq!(c.poll_interval_secs, 60);
        assert!(c.profiles.is_empty());
        let s = serde_json::to_string(&c).unwrap();
        let c2 = AppConfig::from_json(&s).unwrap();
        assert_eq!(c, c2);
        let p = Profile::default();
        assert!(p.enabled && p.sync_obsidian_folder && p.propagate_deletes);
        assert_eq!(p.mode, SyncMode::TwoWay);
        assert_eq!(p.backend.kind(), "synology");
    }

    #[test]
    fn legacy_config_migrates_to_profile() {
        let j = r#"{"vault_path":"C:\\Vault","propagate_deletes":false,"ignore_patterns":["Private"],
            "backend":{"type":"s3","endpoint":"https://kr.object.ncloudstorage.com","region":"kr-standard","bucket":"b","access_key":"a","secret_key":"s"}}"#;
        let c = AppConfig::from_json(j).unwrap();
        assert_eq!(c.profiles.len(), 1);
        let p = &c.profiles[0];
        assert_eq!(p.id, "default");
        assert_eq!(p.local_path, "C:\\Vault");
        assert_eq!(p.backend.kind(), "s3");
        assert!(!p.propagate_deletes);
        assert_eq!(p.ignore_patterns, vec!["Private".to_string()]);
        assert_eq!(p.kind, ProfileKind::Obsidian);
    }

    #[test]
    fn webdav_migrates_to_synology() {
        let j = r#"{"vault_path":"x","backend":{"type":"web_dav","url":"https://nas.local:5006/home/Obsidian%20Vault/","username":"u","password":"p"}}"#;
        let c = AppConfig::from_json(j).unwrap();
        assert_eq!(
            c.profiles[0].backend,
            BackendConfig::Synology {
                url: "https://nas.local:5001".into(),
                username: "u".into(),
                password: "p".into(),
                folder: "/home/Obsidian Vault".into(),
            }
        );
    }

    #[test]
    fn duplicate_ids_fixed() {
        let j = r#"{"profiles":[{"id":"a"},{"id":"a"},{}]}"#;
        let c = AppConfig::from_json(j).unwrap();
        assert_eq!(c.profiles[0].id, "a");
        assert_ne!(c.profiles[1].id, "a");
        assert_ne!(c.profiles[1].id, c.profiles[2].id);
    }

    #[test]
    fn overlap() {
        assert!(paths_overlap("/a/b", "/a/b/"));
        assert!(paths_overlap("/a/b", "/a/b/c"));
        assert!(!paths_overlap("/a/b", "/a/bc"));
        assert!(paths_overlap(r"C:\X", r"C:\X\Y"));
    }

    #[test]
    fn synology_validation() {
        let mut b = BackendConfig::Synology {
            url: "https://nas:5001".into(),
            username: "u".into(),
            password: "p".into(),
            folder: "home/x".into(),
        };
        assert!(b.validate().is_err());
        if let BackendConfig::Synology { folder, .. } = &mut b {
            *folder = "/home/x".into();
        }
        assert!(b.validate().is_ok());
    }
}
