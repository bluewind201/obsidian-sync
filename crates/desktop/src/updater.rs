//! GitHub 릴리즈 기반 자동 업데이트 확인, 다운로드, SHA-256 검증 및 무중단 자가 교체(Windows).
//! - Android 앱은 Java `Updater.java` (`PackageInstaller`)를 사용하며, Windows 데스크톱은 본 모듈을 사용합니다.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const GITHUB_REPO: &str = "bluewind201/obsidian-sync";
pub const RELEASE_ASSET_EXE: &str = "OnyxStudio.exe";
pub const LEGACY_ASSET_EXE: &str = "ObsidianSync.exe";
pub const SUMS_ASSET_NAME: &str = "SHA256SUMS.txt";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UpdateStatusDto {
    pub supported: bool,
    pub current_version: String,
    pub auto_update: bool,
    pub checking: bool,
    pub downloading: bool,
    pub ready_to_install: bool,
    pub applying: bool,
    pub available_tag: Option<String>,
    pub release_url: Option<String>,
    pub release_notes: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub last_checked_at: Option<String>,
    pub message: String,
    pub error: Option<String>,
}

impl Default for UpdateStatusDto {
    fn default() -> Self {
        Self {
            supported: cfg!(windows),
            current_version: obsidian_sync_core::VERSION.to_string(),
            auto_update: true,
            checking: false,
            downloading: false,
            ready_to_install: false,
            applying: false,
            available_tag: None,
            release_url: None,
            release_notes: None,
            downloaded_bytes: 0,
            total_bytes: 0,
            last_checked_at: None,
            message: String::new(),
            error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub html_url: String,
    pub notes: String,
    pub exe_name: String,
    pub exe_url: String,
    pub exe_size: u64,
    pub sums_url: Option<String>,
}

/// `"v1.1.0"`, `"1.1.0"`, `"v1.1.0-b42"` 형식의 시맨틱 버전을 `(major, minor, patch, build)`로 파싱합니다.
pub fn parse_semver_tuple(raw: &str) -> Option<(u64, u64, u64, i64)> {
    let clean = raw.trim().trim_start_matches(['v', 'V']);
    if clean.is_empty() {
        return None;
    }
    let (ver_part, suffix) = match clean.split_once('-') {
        Some((v, s)) => (v, Some(s)),
        None => (clean, None),
    };
    let mut parts = ver_part.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    let patch: u64 = parts.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
    if parts.next().is_some() {
        return None;
    }
    let build = suffix
        .and_then(|s| s.strip_prefix('b').or_else(|| s.strip_prefix('B')))
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(-1);
    Some((major, minor, patch, build))
}

/// `remote_tag`가 `local_version`보다 최신 릴리즈인지 비교합니다.
pub fn is_newer_release(remote_tag: &str, local_version: &str) -> bool {
    let Some((rmaj, rmin, rpat, _)) = parse_semver_tuple(remote_tag) else {
        return false;
    };
    let Some((lmaj, lmin, lpat, _)) = parse_semver_tuple(local_version) else {
        return false;
    };
    (rmaj, rmin, rpat) > (lmaj, lmin, lpat)
}

/// GitHub `/releases/latest` JSON 응답에서 Windows 실행 바이너리 및 체크섬 에셋 정보를 추출합니다.
pub fn parse_github_release_json(v: &serde_json::Value) -> Option<ReleaseInfo> {
    if v.get("draft").and_then(|d| d.as_bool()).unwrap_or(false) {
        return None;
    }
    let tag = v.get("tag_name")?.as_str()?.trim().to_string();
    if tag.is_empty() {
        return None;
    }
    let html_url = v
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();
    let notes = v
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("")
        .to_string();

    let assets = v.get("assets")?.as_array()?;
    let mut picked_exe: Option<(String, String, u64)> = None;
    let mut legacy_exe: Option<(String, String, u64)> = None;
    let mut sums_url: Option<String> = None;

    for a in assets {
        let name = a.get("name").and_then(|n| n.as_str()).unwrap_or("").trim();
        let url = a
            .get("browser_download_url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .trim();
        let size = a.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
        if url.is_empty() {
            continue;
        }
        if name.eq_ignore_ascii_case(RELEASE_ASSET_EXE) {
            picked_exe = Some((name.to_string(), url.to_string(), size));
        } else if name.eq_ignore_ascii_case(LEGACY_ASSET_EXE) && legacy_exe.is_none() {
            legacy_exe = Some((name.to_string(), url.to_string(), size));
        } else if name.eq_ignore_ascii_case(SUMS_ASSET_NAME) {
            sums_url = Some(url.to_string());
        }
    }

    let (exe_name, exe_url, exe_size) = picked_exe.or(legacy_exe)?;
    Some(ReleaseInfo {
        tag,
        html_url,
        notes,
        exe_name,
        exe_url,
        exe_size,
        sums_url,
    })
}

/// `SHA256SUMS.txt` 본문에서 특정 에셋의 SHA-256 해시(소문자 64자리 hex)를 찾아 반환합니다.
pub fn extract_sha256_for_asset(sums_text: &str, asset_name: &str) -> Option<String> {
    let target_lower = asset_name.trim().to_ascii_lowercase();
    for line in sums_text.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(file_token) = parts.next() else {
            continue;
        };
        let clean_name = file_token
            .trim_start_matches('*')
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file_token)
            .to_ascii_lowercase();
        if clean_name == target_lower
            && hash.len() == 64
            && hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct UpdaterPrefs {
    #[serde(default = "default_true")]
    auto_update: bool,
}

fn default_true() -> bool {
    true
}

impl Default for UpdaterPrefs {
    fn default() -> Self {
        Self { auto_update: true }
    }
}

fn prefs_path(data_dir: &Path) -> PathBuf {
    data_dir.join("updater.json")
}

fn load_prefs(data_dir: &Path) -> UpdaterPrefs {
    std::fs::read_to_string(prefs_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_prefs(data_dir: &Path, prefs: &UpdaterPrefs) {
    let _ = std::fs::create_dir_all(data_dir);
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        let _ = std::fs::write(prefs_path(data_dir), json);
    }
}

#[derive(Clone)]
pub struct AppUpdater {
    data_dir: PathBuf,
    status: Arc<Mutex<UpdateStatusDto>>,
    latest_release: Arc<Mutex<Option<ReleaseInfo>>>,
    staged_path: Arc<Mutex<Option<PathBuf>>>,
    expected_sha256: Arc<Mutex<Option<String>>>,
    busy: Arc<AtomicBool>,
    main_hwnd: Arc<Mutex<isize>>,
}

impl AppUpdater {
    pub fn new(data_dir: PathBuf) -> Self {
        let prefs = load_prefs(&data_dir);
        let initial = UpdateStatusDto {
            auto_update: prefs.auto_update,
            ..Default::default()
        };
        Self {
            data_dir,
            status: Arc::new(Mutex::new(initial)),
            latest_release: Arc::new(Mutex::new(None)),
            staged_path: Arc::new(Mutex::new(None)),
            expected_sha256: Arc::new(Mutex::new(None)),
            busy: Arc::new(AtomicBool::new(false)),
            main_hwnd: Arc::new(Mutex::new(0)),
        }
    }

    pub fn set_main_hwnd(&self, hwnd: isize) {
        *self.main_hwnd.lock().unwrap() = hwnd;
    }

    pub fn main_hwnd(&self) -> isize {
        *self.main_hwnd.lock().unwrap()
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn status(&self) -> UpdateStatusDto {
        self.status.lock().unwrap().clone()
    }

    pub fn is_auto_update_enabled(&self) -> bool {
        self.status.lock().unwrap().auto_update
    }

    pub fn set_auto_update(&self, enabled: bool) -> UpdateStatusDto {
        save_prefs(
            &self.data_dir,
            &UpdaterPrefs {
                auto_update: enabled,
            },
        );
        let should_download = {
            let mut st = self.status.lock().unwrap();
            st.auto_update = enabled;
            enabled && st.available_tag.is_some() && !st.ready_to_install && !st.downloading
        };
        if should_download {
            self.check_async(true);
        }
        self.status()
    }

    #[cfg(windows)]
    pub fn check_async(&self, allow_auto_download: bool) {
        let this = self.clone();
        std::thread::Builder::new()
            .name("onyx-update-check".into())
            .spawn(move || {
                let _ = this.check_and_maybe_download_sync(allow_auto_download);
            })
            .ok();
    }

    #[cfg(not(windows))]
    pub fn check_async(&self, _allow_auto_download: bool) {}
}

#[cfg(windows)]
mod win_impl {
    use super::*;
    use chrono::Local;
    use sha2::{Digest, Sha256};
    use std::io::Read;

    fn sanitize_tag_for_file(tag: &str) -> String {
        tag.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    fn file_sha256_hex(path: &Path) -> anyhow::Result<String> {
        let mut f = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    fn verify_downloaded_exe(
        path: &Path,
        expected_size: u64,
        expected_sha256: Option<&str>,
    ) -> anyhow::Result<()> {
        let meta = std::fs::metadata(path)?;
        if meta.len() < 64 * 1024 {
            anyhow::bail!(
                "다운로드한 파일 크기가 너무 작습니다 ({} bytes)",
                meta.len()
            );
        }
        if expected_size > 0 && meta.len() != expected_size {
            anyhow::bail!(
                "다운로드 파일 크기가 일치하지 않습니다 (기대: {}, 실제: {})",
                expected_size,
                meta.len()
            );
        }
        let mut head = [0u8; 2];
        {
            let mut f = std::fs::File::open(path)?;
            f.read_exact(&mut head)?;
        }
        if &head != b"MZ" {
            anyhow::bail!("올바른 Windows 실행 파일(PE MZ)이 아닙니다");
        }
        if let Some(exp) = expected_sha256 {
            let actual = file_sha256_hex(path)?;
            if actual != exp.to_ascii_lowercase() {
                anyhow::bail!("SHA-256 무결성 검증에 실패했습니다");
            }
        }
        Ok(())
    }

    fn fetch_latest_release() -> anyhow::Result<Option<ReleaseInfo>> {
        let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
        let resp = ureq::get(&url)
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "OnyxStudio-Desktop")
            .timeout(std::time::Duration::from_secs(15))
            .call()?;
        let val: serde_json::Value = resp.into_json()?;
        Ok(parse_github_release_json(&val))
    }

    fn fetch_expected_sha256(rel: &ReleaseInfo) -> Option<String> {
        let sums_url = rel.sums_url.as_deref()?;
        let resp = ureq::get(sums_url)
            .set("User-Agent", "OnyxStudio-Desktop")
            .timeout(std::time::Duration::from_secs(12))
            .call()
            .ok()?;
        let body = resp.into_string().ok()?;
        extract_sha256_for_asset(&body, &rel.exe_name)
    }

    pub fn cleanup_old_exe_on_startup() {
        let Ok(cur) = std::env::current_exe() else {
            return;
        };
        let old_path = PathBuf::from(format!("{}.old", cur.display()));
        let new_tmp = PathBuf::from(format!("{}.new.tmp", cur.display()));
        if old_path.exists() {
            let _ = std::fs::remove_file(&old_path);
        }
        if new_tmp.exists() {
            let _ = std::fs::remove_file(&new_tmp);
        }
    }

    impl AppUpdater {
        pub fn check_and_maybe_download_sync(
            &self,
            allow_auto_download: bool,
        ) -> anyhow::Result<UpdateStatusDto> {
            if self.busy.swap(true, Ordering::SeqCst) {
                return Ok(self.status());
            }
            let result = self.do_check_and_download(allow_auto_download);
            self.busy.store(false, Ordering::SeqCst);
            result
        }

        fn do_check_and_download(
            &self,
            allow_auto_download: bool,
        ) -> anyhow::Result<UpdateStatusDto> {
            {
                let mut st = self.status.lock().unwrap();
                st.checking = true;
                st.error = None;
                st.message = "GitHub에서 최신 버전을 확인하는 중…".into();
            }

            let checked_at = Local::now().format("%Y-%m-%d %H:%M").to_string();
            let rel_opt = match fetch_latest_release() {
                Ok(v) => v,
                Err(e) => {
                    let msg = format!("업데이트 확인 실패: {e}");
                    let mut st = self.status.lock().unwrap();
                    st.checking = false;
                    st.last_checked_at = Some(checked_at);
                    st.error = Some(msg.clone());
                    st.message = msg.clone();
                    return Err(anyhow::anyhow!(msg));
                }
            };

            let Some(rel) = rel_opt else {
                let mut st = self.status.lock().unwrap();
                st.checking = false;
                st.last_checked_at = Some(checked_at);
                st.available_tag = None;
                st.ready_to_install = false;
                st.message = "현재 최신 버전을 사용 중입니다.".into();
                return Ok(st.clone());
            };

            let local_ver = obsidian_sync_core::VERSION;
            if !is_newer_release(&rel.tag, local_ver) {
                *self.latest_release.lock().unwrap() = None;
                *self.staged_path.lock().unwrap() = None;
                let mut st = self.status.lock().unwrap();
                st.checking = false;
                st.last_checked_at = Some(checked_at);
                st.available_tag = None;
                st.release_url = None;
                st.release_notes = None;
                st.ready_to_install = false;
                st.downloaded_bytes = 0;
                st.total_bytes = 0;
                st.message = format!("현재 최신 버전(v{local_ver})입니다.");
                return Ok(st.clone());
            }

            let expected_hash = fetch_expected_sha256(&rel);
            *self.latest_release.lock().unwrap() = Some(rel.clone());
            *self.expected_sha256.lock().unwrap() = expected_hash.clone();

            {
                let mut st = self.status.lock().unwrap();
                st.checking = false;
                st.last_checked_at = Some(checked_at);
                st.available_tag = Some(rel.tag.clone());
                st.release_url = Some(rel.html_url.clone());
                st.release_notes = Some(rel.notes.chars().take(1200).collect());
                st.total_bytes = rel.exe_size;
                st.message = format!("새 버전 {} 출시!", rel.tag);
            }

            let staged_target = self.staged_file_for(&rel.tag);
            if staged_target.is_file()
                && verify_downloaded_exe(&staged_target, rel.exe_size, expected_hash.as_deref())
                    .is_ok()
            {
                *self.staged_path.lock().unwrap() = Some(staged_target);
                let mut st = self.status.lock().unwrap();
                st.ready_to_install = true;
                st.downloaded_bytes = rel.exe_size;
                st.total_bytes = rel.exe_size;
                st.message = format!("새 버전 {} 업데이트 준비 완료", rel.tag);
                return Ok(st.clone());
            }

            if allow_auto_download && self.is_auto_update_enabled() {
                self.download_release_into_cache(&rel, expected_hash.as_deref())?;
            }
            Ok(self.status())
        }

        fn staged_file_for(&self, tag: &str) -> PathBuf {
            self.data_dir
                .join("updates")
                .join(format!("OnyxStudio-{}.exe", sanitize_tag_for_file(tag)))
        }

        fn download_release_into_cache(
            &self,
            rel: &ReleaseInfo,
            expected_sha256: Option<&str>,
        ) -> anyhow::Result<PathBuf> {
            let updates_dir = self.data_dir.join("updates");
            std::fs::create_dir_all(&updates_dir)?;
            let out_path = self.staged_file_for(&rel.tag);

            if out_path.is_file()
                && verify_downloaded_exe(&out_path, rel.exe_size, expected_sha256).is_ok()
            {
                *self.staged_path.lock().unwrap() = Some(out_path.clone());
                let mut st = self.status.lock().unwrap();
                st.downloading = false;
                st.ready_to_install = true;
                st.downloaded_bytes = rel.exe_size;
                st.total_bytes = rel.exe_size;
                st.message = format!("새 버전 {} 업데이트 준비 완료", rel.tag);
                return Ok(out_path);
            }

            // 이전 버전 캐시 파일 정리
            if let Ok(entries) = std::fs::read_dir(&updates_dir) {
                let target_name = out_path.file_name().map(|n| n.to_os_string());
                for entry in entries.flatten() {
                    if target_name.as_ref() != Some(&entry.file_name()) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }

            let part_path = updates_dir.join(format!("{}.part", sanitize_tag_for_file(&rel.tag)));
            {
                let mut st = self.status.lock().unwrap();
                st.downloading = true;
                st.ready_to_install = false;
                st.downloaded_bytes = 0;
                st.total_bytes = rel.exe_size;
                st.error = None;
                st.message = format!("새 버전 {} 다운로드 중…", rel.tag);
            }

            let dl_res: anyhow::Result<()> = (|| {
                let resp = ureq::get(&rel.exe_url)
                    .set("User-Agent", "OnyxStudio-Desktop")
                    .timeout(std::time::Duration::from_secs(120))
                    .call()?;
                let header_len = resp
                    .header("Content-Length")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(rel.exe_size);
                if header_len > 0 {
                    self.status.lock().unwrap().total_bytes = header_len;
                }
                let mut reader = resp.into_reader();
                let mut out_file = std::fs::File::create(&part_path)?;
                let mut buf = [0u8; 64 * 1024];
                let mut written = 0u64;
                loop {
                    let n = reader.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    std::io::Write::write_all(&mut out_file, &buf[..n])?;
                    written += n as u64;
                    self.status.lock().unwrap().downloaded_bytes = written;
                }
                use std::io::Write;
                out_file.flush()?;
                drop(out_file);
                verify_downloaded_exe(&part_path, rel.exe_size, expected_sha256)?;
                if out_path.exists() {
                    let _ = std::fs::remove_file(&out_path);
                }
                std::fs::rename(&part_path, &out_path)?;
                Ok(())
            })();

            match dl_res {
                Ok(()) => {
                    *self.staged_path.lock().unwrap() = Some(out_path.clone());
                    let mut st = self.status.lock().unwrap();
                    st.downloading = false;
                    st.ready_to_install = true;
                    st.error = None;
                    st.message = format!("새 버전 {} 업데이트 준비 완료", rel.tag);
                    Ok(out_path)
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&part_path);
                    let msg = format!("업데이트 다운로드 실패: {e}");
                    let mut st = self.status.lock().unwrap();
                    st.downloading = false;
                    st.ready_to_install = false;
                    st.error = Some(msg.clone());
                    st.message = msg.clone();
                    Err(anyhow::anyhow!(msg))
                }
            }
        }

        pub fn prepare_staged_for_apply(&self) -> anyhow::Result<PathBuf> {
            // 진행 중인 백그라운드 확인/다운로드가 있으면 완료 대기 (최대 60초)
            let start = std::time::Instant::now();
            while self.busy.load(Ordering::SeqCst) {
                if start.elapsed() > std::time::Duration::from_secs(60) {
                    anyhow::bail!("이전 업데이트 확인 작업이 아직 진행 중입니다.");
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            }

            if let Some(staged) = self.staged_path.lock().unwrap().clone() {
                if staged.is_file() {
                    return Ok(staged);
                }
            }

            let rel = match self.latest_release.lock().unwrap().clone() {
                Some(r) => r,
                None => {
                    self.check_and_maybe_download_sync(false)?;
                    self.latest_release
                        .lock()
                        .unwrap()
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("이미 최신 버전을 사용 중입니다."))?
                }
            };

            let hash = self.expected_sha256.lock().unwrap().clone();
            self.busy.store(true, Ordering::SeqCst);
            let res = self.download_release_into_cache(&rel, hash.as_deref());
            self.busy.store(false, Ordering::SeqCst);
            res
        }

        pub fn swap_and_restart(&self, staged_exe: &Path) -> anyhow::Result<()> {
            {
                let mut st = self.status.lock().unwrap();
                st.applying = true;
                st.message = "새 버전을 적용하고 재시작하는 중…".into();
            }

            let current_exe = std::env::current_exe()?;
            let new_tmp = PathBuf::from(format!("{}.new.tmp", current_exe.display()));
            let old_exe = PathBuf::from(format!("{}.old", current_exe.display()));

            if new_tmp.exists() {
                let _ = std::fs::remove_file(&new_tmp);
            }
            std::fs::copy(staged_exe, &new_tmp).map_err(|e| {
                anyhow::anyhow!("업데이트 파일 복사 실패 (관리자 권한 폴더인지 확인하세요): {e}")
            })?;

            if old_exe.exists() {
                let _ = std::fs::remove_file(&old_exe);
            }

            if let Err(rename_err) = std::fs::rename(&current_exe, &old_exe) {
                let _ = std::fs::remove_file(&new_tmp);
                anyhow::bail!("실행 파일 교체 준비 실패: {rename_err}");
            }
            if let Err(swap_err) = std::fs::rename(&new_tmp, &current_exe) {
                let _ = std::fs::rename(&old_exe, &current_exe);
                let _ = std::fs::remove_file(&new_tmp);
                anyhow::bail!("새 실행 파일 배치 실패: {swap_err}");
            }

            crate::platform::ensure_autostart_current_exe();
            let hwnd = self.main_hwnd();
            let was_hidden = hwnd != 0 && !crate::platform::is_window_visible_hwnd(hwnd);

            crate::platform::release_single_instance_for_restart();

            let mut cmd = std::process::Command::new(&current_exe);
            cmd.arg(format!("--replacing-pid={}", std::process::id()));
            if was_hidden {
                cmd.arg("--minimized");
            }
            if let Err(spawn_err) = cmd.spawn() {
                // 새 바이너리 실행 실패 시 구 바이너리 복원 시도
                let _ = std::fs::rename(&current_exe, &new_tmp);
                let _ = std::fs::rename(&old_exe, &current_exe);
                anyhow::bail!("새 버전 실행 실패: {spawn_err}");
            }

            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod non_win_impl {
    use super::*;

    pub fn cleanup_old_exe_on_startup() {}

    impl AppUpdater {
        pub fn check_and_maybe_download_sync(
            &self,
            _allow_auto_download: bool,
        ) -> anyhow::Result<UpdateStatusDto> {
            Ok(self.status())
        }

        pub fn prepare_staged_for_apply(&self) -> anyhow::Result<PathBuf> {
            anyhow::bail!("이 운영체제에서는 내장 자동 업데이트를 지원하지 않습니다.")
        }

        pub fn swap_and_restart(&self, _staged_exe: &Path) -> anyhow::Result<()> {
            anyhow::bail!("이 운영체제에서는 내장 자동 업데이트를 지원하지 않습니다.")
        }
    }
}

#[cfg(not(windows))]
pub use non_win_impl::cleanup_old_exe_on_startup;
#[cfg(windows)]
pub use win_impl::cleanup_old_exe_on_startup;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semver_parsing_and_comparison_work() {
        assert_eq!(parse_semver_tuple("v1.1.0"), Some((1, 1, 0, -1)));
        assert_eq!(parse_semver_tuple("1.0.1"), Some((1, 0, 1, -1)));
        assert_eq!(parse_semver_tuple("v1.2.3-b45"), Some((1, 2, 3, 45)));
        assert!(is_newer_release("v1.1.0", "1.0.1"));
        assert!(is_newer_release("v2.0.0-b99", "1.9.9"));
        assert!(!is_newer_release("v1.0.1", "1.0.1"));
        assert!(!is_newer_release("v1.0.0", "1.0.1"));
    }

    #[test]
    fn parse_github_release_extracts_onyx_exe_and_sha256() {
        let payload = json!({
            "tag_name": "v1.1.0",
            "html_url": "https://github.com/bluewind201/obsidian-sync/releases/tag/v1.1.0",
            "body": "## 변경사항\n- 자동 업데이트 지원",
            "draft": false,
            "assets": [
                {
                    "name": "OnyxStudio-android.apk",
                    "browser_download_url": "https://example.com/OnyxStudio-android.apk",
                    "size": 123456
                },
                {
                    "name": "OnyxStudio.exe",
                    "browser_download_url": "https://example.com/OnyxStudio.exe",
                    "size": 9876543
                },
                {
                    "name": "SHA256SUMS.txt",
                    "browser_download_url": "https://example.com/SHA256SUMS.txt",
                    "size": 420
                }
            ]
        });

        let rel = parse_github_release_json(&payload).unwrap();
        assert_eq!(rel.tag, "v1.1.0");
        assert_eq!(rel.exe_name, "OnyxStudio.exe");
        assert_eq!(rel.exe_size, 9_876_543);
        assert_eq!(
            rel.sums_url.as_deref(),
            Some("https://example.com/SHA256SUMS.txt")
        );

        let sums = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  OnyxStudio-android.apk\n\
                    BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB *OnyxStudio.exe\n";
        assert_eq!(
            extract_sha256_for_asset(sums, &rel.exe_name).as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn updater_prefs_persist_across_reloads() {
        let tmp = tempfile::tempdir().unwrap();
        let up1 = AppUpdater::new(tmp.path().to_path_buf());
        assert!(up1.is_auto_update_enabled());
        let st = up1.set_auto_update(false);
        assert!(!st.auto_update);

        let up2 = AppUpdater::new(tmp.path().to_path_buf());
        assert!(!up2.is_auto_update_enabled());
    }
}
