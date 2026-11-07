//! Synology NAS 저장소 (DSM File Station Web API)
//!
//! - 로그인: `SYNO.API.Auth` (계정/비밀번호, 2단계 인증 OTP, 기기 토큰으로 OTP 생략)
//! - 목록/정보: `SYNO.FileStation.List`
//! - 업로드: `SYNO.FileStation.Upload` (multipart, 파일을 메모리에 올리지 않고 스트리밍)
//! - 다운로드: `SYNO.FileStation.Download` (스트리밍)
//! - 삭제: `SYNO.FileStation.Delete` (공유 폴더에 휴지통이 켜져 있으면 휴지통으로 이동)
//!
//! 변경 감지는 주기적으로 목록을 비교하는 방식(폴링)입니다. 리비전은 `크기-수정시각` 입니다.

use super::{check_path, Backend, RemoteEntry};
use crate::config::BackendConfig;
use crate::http;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 목록 조회 시 들어가지 않는 Synology 시스템 폴더
const SKIP_DIRS: &[&str] = &[
    "@eaDir",
    "#recycle",
    "#snapshot",
    ".SynologyWorkingDirectory",
];
const PAGE: usize = 1000;
const DEVICE_NAME: &str = "Obsidian Sync";

/// 사용하는 API와 원하는 버전
const APIS: &[(&str, u32)] = &[
    ("SYNO.API.Auth", 6),
    ("SYNO.FileStation.List", 2),
    ("SYNO.FileStation.Upload", 2),
    ("SYNO.FileStation.Download", 2),
    ("SYNO.FileStation.Delete", 2),
];

/// Synology API 오류
#[derive(Debug)]
pub struct SynoError {
    pub code: i64,
    pub api: String,
    /// 하위 오류 코드 (삭제 실패 등)
    pub sub_codes: Vec<i64>,
}

impl SynoError {
    fn is_auth(&self) -> bool {
        self.api == "SYNO.API.Auth"
    }
    /// 2단계 인증 코드(OTP)가 필요하거나 틀린 경우
    pub fn needs_otp(&self) -> bool {
        self.is_auth() && matches!(self.code, 403 | 404)
    }
    fn session_expired(&self) -> bool {
        matches!(self.code, 106 | 107 | 119)
    }
    fn not_found(&self) -> bool {
        !self.is_auth()
            && (self.code == 408
                || (self.code == 900
                    && !self.sub_codes.is_empty()
                    && self.sub_codes.iter().all(|c| *c == 408)))
    }
    fn message(&self) -> &'static str {
        if self.is_auth() {
            return match self.code {
                400 => "아이디 또는 비밀번호가 틀립니다",
                401 => "비활성화된 계정입니다",
                402 => "이 계정은 File Station을 사용할 권한이 없습니다",
                403 => "2단계 인증 코드(OTP)가 필요합니다",
                404 => "2단계 인증 코드(OTP)가 틀렸습니다. 새 코드로 다시 입력해 주세요",
                406 => "DSM에서 2단계 인증을 먼저 설정해야 합니다",
                407 => "로그인 실패가 많아 이 IP가 차단되었습니다 (DSM 보안 설정 확인)",
                408..=410 => {
                    "비밀번호가 만료되었거나 변경이 필요합니다. DSM에 직접 로그인해 바꿔 주세요"
                }
                _ => Self::common(self.code),
            };
        }
        match self.code {
            400 => "잘못된 파일 작업 요청입니다",
            401 => "알 수 없는 파일 작업 오류입니다",
            402 => "NAS가 바빠서 처리하지 못했습니다",
            403..=407 => "이 폴더에 접근할 권한이 없습니다",
            408 => "폴더나 파일을 찾을 수 없습니다",
            411 => "읽기 전용 폴더입니다",
            412 | 418 | 419 | 420 => "파일/폴더 이름이 NAS에서 허용되지 않습니다",
            413 => "암호화된 공유 폴더가 잠겨 있습니다",
            414 => "같은 이름의 파일이 이미 있습니다",
            415 => "디스크 할당량을 초과했습니다",
            416 => "NAS 저장 공간이 부족합니다",
            417 => "NAS 입출력 오류입니다",
            900 => "삭제하지 못했습니다",
            1800 | 1801 | 1803 => "업로드가 중간에 끊겼습니다",
            1804 => "파일이 너무 커서 이 폴더에 올릴 수 없습니다",
            1805 => "기존 파일을 덮어쓸 수 없습니다",
            _ => Self::common(self.code),
        }
    }
    fn common(code: i64) -> &'static str {
        match code {
            101..=104 => "이 DSM 버전에서 지원하지 않는 요청입니다 (DSM 6.2 이상 필요)",
            105 => "권한이 없습니다",
            106 => "로그인 세션이 만료되었습니다",
            107 => "다른 곳에서 로그인해 세션이 끊겼습니다",
            119 => "로그인 세션을 찾을 수 없습니다",
            _ => "알 수 없는 오류",
        }
    }
}

impl std::fmt::Display for SynoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Synology: {} (오류 {}", self.message(), self.code)?;
        if !self.sub_codes.is_empty() {
            write!(f, ", {:?}", self.sub_codes)?;
        }
        write!(f, ")")
    }
}
impl std::error::Error for SynoError {}

/// 오류가 2단계 인증 코드 입력이 필요한 상황인지
pub fn needs_otp(e: &anyhow::Error) -> bool {
    e.chain()
        .any(|c| c.downcast_ref::<SynoError>().is_some_and(|s| s.needs_otp()))
}

fn syno_code(e: &anyhow::Error) -> Option<&SynoError> {
    e.chain().find_map(|c| c.downcast_ref::<SynoError>())
}

#[derive(Clone)]
struct ApiInfo {
    path: String,
    version: u32,
}

pub struct SynologyBackend {
    base: String,
    user: String,
    pass: String,
    /// NAS 안의 동기화 폴더 (예: `/home/Obsidian`, 끝 `/` 없음)
    root: String,
    token_path: PathBuf,
    agent: ureq::Agent,
    sid: Option<String>,
    apis: Option<HashMap<String, ApiInfo>>,
}

fn base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn norm_root(folder: &str) -> String {
    let parts: Vec<&str> = folder.trim().split('/').filter(|s| !s.is_empty()).collect();
    format!("/{}", parts.join("/"))
}

/// 기기 토큰 파일 경로 (NAS 주소 + 계정별). 2단계 인증을 한 번 통과하면 저장됩니다.
pub fn device_token_path(data_dir: &Path, cfg: &BackendConfig) -> Option<PathBuf> {
    let BackendConfig::Synology { url, username, .. } = cfg else {
        return None;
    };
    let key = crate::engine::hash_bytes(
        format!("{}\n{}", base_url(url).to_lowercase(), username.trim()).as_bytes(),
    );
    Some(data_dir.join(format!("synology-{}.json", &key[..16])))
}

fn load_device_id(path: &Path) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    v.get("device_id")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn save_device_id(path: &Path, id: &str) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json!({ "device_id": id }).to_string())?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// 저장된 기기 토큰 삭제 (다음 로그인 때 다시 OTP를 묻습니다)
pub fn forget_device(data_dir: &Path, cfg: &BackendConfig) {
    if let Some(p) = device_token_path(data_dir, cfg) {
        let _ = std::fs::remove_file(p);
    }
}

/// 2단계 인증을 통과해 기기 토큰이 저장되어 있는지
pub fn has_device_token(data_dir: &Path, cfg: &BackendConfig) -> bool {
    device_token_path(data_dir, cfg)
        .and_then(|p| load_device_id(&p))
        .is_some()
}

/// 로그인 (OTP가 있으면 함께 보내고 기기 토큰을 저장). 성공하면 세션을 바로 닫습니다.
pub fn login(cfg: &BackendConfig, data_dir: &Path, otp: Option<&str>) -> Result<()> {
    cfg.validate()?;
    let mut b = SynologyBackend::new(cfg, data_dir)?;
    b.login(otp.map(str::trim).filter(|s| !s.is_empty()))?;
    b.logout();
    Ok(())
}

fn api_err(api: &str, v: &Value) -> SynoError {
    let err = v.get("error");
    let code = err
        .and_then(|e| e.get("code"))
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    let sub_codes = err
        .and_then(|e| e.get("errors"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("code").and_then(Value::as_i64))
                .collect()
        })
        .unwrap_or_default();
    SynoError {
        code,
        api: api.into(),
        sub_codes,
    }
}

/// API 응답 JSON 확인. 실패면 [`SynoError`]
fn check_json(api: &str, v: Value) -> Result<Value> {
    if v.get("success").and_then(Value::as_bool) == Some(true) {
        Ok(v.get("data").cloned().unwrap_or(Value::Null))
    } else {
        Err(api_err(api, &v).into())
    }
}

fn path_array(p: &str) -> String {
    Value::Array(vec![Value::String(p.into())]).to_string()
}

fn multipart_text(boundary: &str, name: &str, value: &str) -> String {
    format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    crate::scan::mtime_ms(meta)
}

impl SynologyBackend {
    pub fn new(cfg: &BackendConfig, data_dir: &Path) -> Result<Self> {
        let BackendConfig::Synology {
            url,
            username,
            password,
            folder,
        } = cfg
        else {
            bail!("Synology 설정이 아닙니다");
        };
        Ok(Self {
            base: base_url(url),
            user: username.trim().to_string(),
            pass: password.clone(),
            root: norm_root(folder),
            token_path: device_token_path(data_dir, cfg).unwrap(),
            agent: http::agent(),
            sid: None,
            apis: None,
        })
    }

    /// 상대경로 → NAS 절대경로
    fn abs(&self, rel: &str) -> String {
        if rel.is_empty() {
            self.root.clone()
        } else {
            format!("{}/{}", self.root, rel)
        }
    }

    fn share_path(&self) -> String {
        let share = self
            .root
            .trim_start_matches('/')
            .split('/')
            .next()
            .unwrap_or("");
        format!("/{share}")
    }

    /// 사용 가능한 API 경로/버전 조회 (DSM 6/7 모두 지원)
    fn api(&mut self, name: &str) -> Result<ApiInfo> {
        if self.apis.is_none() {
            let query = APIS.iter().map(|a| a.0).collect::<Vec<_>>().join(",");
            let url = format!("{}/webapi/query.cgi", self.base);
            let resp = http::check(
                self.agent
                    .get(&url)
                    .query("api", "SYNO.API.Info")
                    .query("version", "1")
                    .query("method", "query")
                    .query("query", &query)
                    .call(),
            )
            .context("NAS에 연결할 수 없습니다. 주소와 포트(보통 5001)를 확인하세요")?;
            let v = http::read_json(resp)
                .context("Synology DSM 응답이 아닙니다. 주소와 포트(보통 5001)를 확인하세요")?;
            let data = check_json("SYNO.API.Info", v)?;
            let mut map = HashMap::new();
            for (api, want) in APIS {
                let Some(info) = data.get(*api) else { continue };
                let path = info
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("entry.cgi");
                let max = info.get("maxVersion").and_then(Value::as_u64).unwrap_or(1) as u32;
                let min = info.get("minVersion").and_then(Value::as_u64).unwrap_or(1) as u32;
                let version = (*want).min(max).max(min);
                map.insert(
                    api.to_string(),
                    ApiInfo {
                        path: path.to_string(),
                        version,
                    },
                );
            }
            self.apis = Some(map);
        }
        self.apis
            .as_ref()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "이 NAS에서 {name} API를 사용할 수 없습니다 (DSM 6.2 이상, File Station 필요)"
                )
            })
    }

    fn login(&mut self, otp: Option<&str>) -> Result<()> {
        let info = self.api("SYNO.API.Auth")?;
        let device_id = load_device_id(&self.token_path);
        let version = info.version.to_string();
        let mut form: Vec<(&str, &str)> = vec![
            ("api", "SYNO.API.Auth"),
            ("version", &version),
            ("method", "login"),
            ("account", &self.user),
            ("passwd", &self.pass),
            ("session", "FileStation"),
            ("format", "sid"),
        ];
        if let Some(code) = otp {
            form.push(("otp_code", code));
            if info.version >= 6 {
                form.push(("enable_device_token", "yes"));
                form.push(("device_name", DEVICE_NAME));
            }
        } else if let Some(id) = device_id.as_deref() {
            if info.version >= 6 {
                form.push(("device_id", id));
                form.push(("device_name", DEVICE_NAME));
            }
        }
        let url = format!("{}/webapi/{}", self.base, info.path);
        let resp = http::check(self.agent.post(&url).send_form(&form))?;
        let data = check_json("SYNO.API.Auth", http::read_json(resp)?)?;
        let sid = data
            .get("sid")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Synology 로그인 응답에 세션이 없습니다"))?;
        self.sid = Some(sid.to_string());
        if otp.is_some() {
            if let Some(did) = data.get("did").and_then(Value::as_str) {
                save_device_id(&self.token_path, did)?;
            }
        }
        Ok(())
    }

    /// 세션 종료 (실패해도 무시)
    fn logout(&mut self) {
        let Some(sid) = self.sid.take() else { return };
        if let Ok(info) = self.api("SYNO.API.Auth") {
            let url = format!("{}/webapi/{}", self.base, info.path);
            let v = info.version.to_string();
            let _ = self.agent.post(&url).send_form(&[
                ("api", "SYNO.API.Auth"),
                ("version", &v),
                ("method", "logout"),
                ("session", "FileStation"),
                ("_sid", &sid),
            ]);
        }
    }

    fn sid(&mut self) -> Result<String> {
        if self.sid.is_none() {
            self.login(None)?;
        }
        Ok(self.sid.clone().unwrap())
    }

    /// API 호출 (세션이 만료되면 한 번 다시 로그인)
    fn call(&mut self, api: &str, method: &str, params: &[(&str, &str)]) -> Result<Value> {
        match self.call_once(api, method, params) {
            Err(e) if syno_code(&e).is_some_and(|s| s.session_expired()) => {
                self.sid = None;
                self.call_once(api, method, params)
            }
            r => r,
        }
    }

    fn call_once(&mut self, api: &str, method: &str, params: &[(&str, &str)]) -> Result<Value> {
        let info = self.api(api)?;
        let sid = self.sid()?;
        let version = info.version.to_string();
        let mut form: Vec<(&str, &str)> = vec![
            ("api", api),
            ("version", &version),
            ("method", method),
            ("_sid", &sid),
        ];
        form.extend_from_slice(params);
        let url = format!("{}/webapi/{}", self.base, info.path);
        let resp = http::check(self.agent.post(&url).send_form(&form))?;
        check_json(api, http::read_json(resp)?)
    }

    fn entry_rev(f: &Value) -> (u64, String) {
        let add = f.get("additional");
        let size = add
            .and_then(|a| a.get("size"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let mtime = add
            .and_then(|a| a.get("time"))
            .and_then(|t| t.get("mtime"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        (size, format!("{size}-{mtime}"))
    }

    /// 폴더 하나의 목록 (페이지 단위로 모두 가져옴)
    fn list_dir(&mut self, abs: &str) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        loop {
            let off = offset.to_string();
            let lim = PAGE.to_string();
            let data = self.call(
                "SYNO.FileStation.List",
                "list",
                &[
                    ("folder_path", abs),
                    ("additional", r#"["size","time"]"#),
                    ("offset", &off),
                    ("limit", &lim),
                ],
            )?;
            let files = data
                .get("files")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let total = data.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
            let n = files.len();
            out.extend(files);
            offset += n;
            if n == 0 || offset >= total {
                break;
            }
        }
        Ok(out)
    }

    /// 파일 정보 (없으면 None)
    fn info(&mut self, abs: &str) -> Result<Option<Value>> {
        let data = match self.call(
            "SYNO.FileStation.List",
            "getinfo",
            &[
                ("path", &path_array(abs)),
                ("additional", r#"["size","time"]"#),
            ],
        ) {
            Ok(d) => d,
            Err(e) if syno_code(&e).is_some_and(|s| s.not_found()) => return Ok(None),
            Err(e) => return Err(e),
        };
        let f = data
            .get("files")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .cloned();
        Ok(f.filter(|f| f.get("code").is_none()))
    }

    fn upload_once(&mut self, rel: &str, src: &Path) -> Result<()> {
        let info = self.api("SYNO.FileStation.Upload")?;
        let sid = self.sid()?;
        let abs = self.abs(rel);
        let (parent, name) = abs.rsplit_once('/').unwrap();
        if name.contains(['"', '\r', '\n']) {
            bail!("NAS로 올릴 수 없는 파일 이름입니다 (\" 문자 포함): {rel}");
        }
        let file = std::fs::File::open(src)
            .with_context(|| format!("파일을 열 수 없습니다: {}", src.display()))?;
        let meta = file.metadata()?;
        let len = meta.len();

        let boundary = format!(
            "----obsync{}",
            &crate::engine::hash_bytes(format!("{abs}{len}{}", mtime_ms(&meta)).as_bytes())[..24]
        );
        let version = info.version.to_string();
        let mut head = String::new();
        head += &multipart_text(&boundary, "api", "SYNO.FileStation.Upload");
        head += &multipart_text(&boundary, "version", &version);
        head += &multipart_text(&boundary, "method", "upload");
        head += &multipart_text(&boundary, "path", parent);
        head += &multipart_text(&boundary, "create_parents", "true");
        head += &multipart_text(&boundary, "overwrite", "true");
        head += &multipart_text(&boundary, "mtime", &mtime_ms(&meta).to_string());
        head += &format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        );
        let tail = format!("\r\n--{boundary}--\r\n");
        let total = head.len() as u64 + len + tail.len() as u64;
        let body = std::io::Cursor::new(head.into_bytes())
            .chain(file.take(len))
            .chain(std::io::Cursor::new(tail.into_bytes()));

        let url = format!("{}/webapi/{}", self.base, info.path);
        let resp = http::check(
            self.agent
                .post(&url)
                .query("api", "SYNO.FileStation.Upload")
                .query("version", &version)
                .query("method", "upload")
                .query("_sid", &sid)
                .set(
                    "Content-Type",
                    &format!("multipart/form-data; boundary={boundary}"),
                )
                .set("Content-Length", &total.to_string())
                .send(body),
        )?;
        check_json("SYNO.FileStation.Upload", http::read_json(resp)?)?;
        Ok(())
    }

    fn download_once(&mut self, rel: &str, dest: &mut dyn Write) -> Result<()> {
        let info = self.api("SYNO.FileStation.Download")?;
        let sid = self.sid()?;
        let url = format!("{}/webapi/{}", self.base, info.path);
        let resp = http::check(
            self.agent
                .get(&url)
                .query("api", "SYNO.FileStation.Download")
                .query("version", &info.version.to_string())
                .query("method", "download")
                .query("mode", "download")
                .query("path", &path_array(&self.abs(rel)))
                .query("_sid", &sid)
                .call(),
        )?;
        // 오류는 파일 대신 JSON으로 옴
        if resp.content_type().starts_with("application/json") {
            let bytes = http::read_bytes(resp)?;
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if v.get("success").and_then(Value::as_bool) == Some(false) {
                    return Err(api_err("SYNO.FileStation.Download", &v).into());
                }
            }
            dest.write_all(&bytes)?;
            return Ok(());
        }
        std::io::copy(&mut resp.into_reader(), dest)?;
        Ok(())
    }
}

impl Backend for SynologyBackend {
    fn id(&self) -> String {
        format!("synology:{}@{}{}", self.user, self.base, self.root)
    }

    fn check(&mut self) -> Result<()> {
        let share = self.share_path();
        if self.info(&share)?.is_none() {
            bail!(
                "NAS에 공유 폴더 '{}'이(가) 없거나 접근 권한이 없습니다. NAS 폴더 경로를 확인하세요",
                share.trim_start_matches('/')
            );
        }
        Ok(())
    }

    fn list(&mut self) -> Result<Vec<RemoteEntry>> {
        let mut files = Vec::new();
        let mut queue = vec![String::new()];
        while let Some(dir) = queue.pop() {
            let items = match self.list_dir(&self.abs(&dir)) {
                Ok(i) => i,
                // 동기화 폴더가 아직 없음 → 비어 있는 것으로 간주 (첫 업로드 때 생성)
                Err(e) if dir.is_empty() && syno_code(&e).is_some_and(|s| s.not_found()) => {
                    return Ok(files)
                }
                Err(e) => return Err(e),
            };
            for f in items {
                let Some(name) = f.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let rel = if dir.is_empty() {
                    name.to_string()
                } else {
                    format!("{dir}/{name}")
                };
                if f.get("isdir").and_then(Value::as_bool).unwrap_or(false) {
                    if !SKIP_DIRS.contains(&name) {
                        queue.push(rel);
                    }
                } else {
                    let (size, rev) = Self::entry_rev(&f);
                    files.push(RemoteEntry {
                        path: rel,
                        size,
                        rev,
                    });
                }
            }
        }
        Ok(files)
    }

    fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.download_to(path, &mut buf)?;
        Ok(buf)
    }

    fn download_to(&mut self, path: &str, dest: &mut dyn Write) -> Result<()> {
        check_path(path)?;
        match self.download_once(path, dest) {
            Err(e) if syno_code(&e).is_some_and(|s| s.session_expired()) => {
                self.sid = None;
                self.download_once(path, dest)
            }
            r => r,
        }
    }

    fn upload(&mut self, path: &str, data: &[u8]) -> Result<RemoteEntry> {
        // 메모리 데이터는 임시 파일로 저장해 같은 경로로 올림
        let tmp = std::env::temp_dir().join(format!(
            "obsync-up-{}",
            &crate::engine::hash_bytes(path.as_bytes())[..16]
        ));
        std::fs::write(&tmp, data)?;
        let r = self.upload_file(path, &tmp);
        let _ = std::fs::remove_file(&tmp);
        r
    }

    fn upload_file(&mut self, path: &str, src: &Path) -> Result<RemoteEntry> {
        check_path(path)?;
        match self.upload_once(path, src) {
            Err(e) if syno_code(&e).is_some_and(|s| s.session_expired()) => {
                self.sid = None;
                self.upload_once(path, src)?;
            }
            r => r?,
        }
        let abs = self.abs(path);
        let f = self
            .info(&abs)?
            .ok_or_else(|| anyhow!("업로드 확인 실패: {path}"))?;
        let (size, rev) = Self::entry_rev(&f);
        Ok(RemoteEntry {
            path: path.into(),
            size,
            rev,
        })
    }

    fn delete(&mut self, path: &str) -> Result<()> {
        check_path(path)?;
        let abs = self.abs(path);
        match self.call(
            "SYNO.FileStation.Delete",
            "delete",
            &[("path", &path_array(&abs)), ("recursive", "false")],
        ) {
            Ok(_) => Ok(()),
            Err(e) if syno_code(&e).is_some_and(|s| s.not_found()) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn cfg(url: &str, folder: &str) -> BackendConfig {
        BackendConfig::Synology {
            url: url.into(),
            username: "me".into(),
            password: "pw".into(),
            folder: folder.into(),
        }
    }

    #[test]
    fn paths() {
        let dir = tempfile::tempdir().unwrap();
        let b =
            SynologyBackend::new(&cfg("https://nas:5001/", "home/Obsidian/"), dir.path()).unwrap();
        assert_eq!(b.base, "https://nas:5001");
        assert_eq!(b.root, "/home/Obsidian");
        assert_eq!(b.abs("a/b.md"), "/home/Obsidian/a/b.md");
        assert_eq!(b.share_path(), "/home");
        assert_eq!(path_array("/a,b/\"c\""), r#"["/a,b/\"c\""]"#);
    }

    #[test]
    fn errors() {
        let e: anyhow::Error = SynoError {
            code: 403,
            api: "SYNO.API.Auth".into(),
            sub_codes: vec![],
        }
        .into();
        assert!(needs_otp(&e.context("x")));
        let nf = SynoError {
            code: 900,
            api: "SYNO.FileStation.Delete".into(),
            sub_codes: vec![408],
        };
        assert!(nf.not_found());
        assert!(nf.to_string().contains("900"));
    }

    // ── 가짜 DSM 서버로 전체 흐름 검증 ─────────────────────────

    #[derive(Default)]
    struct Fake {
        files: HashMap<String, (Vec<u8>, i64)>,
        logins: Vec<String>,
        tick: i64,
    }

    struct Req {
        method: String,
        target: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    fn read_req(stream: &mut std::net::TcpStream) -> Option<Req> {
        let mut r = std::io::BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();
        r.read_line(&mut line).ok()?;
        let mut it = line.split_whitespace();
        let method = it.next()?.to_string();
        let target = it.next()?.to_string();
        let mut headers = HashMap::new();
        loop {
            let mut h = String::new();
            r.read_line(&mut h).ok()?;
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }
        let len: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; len];
        r.read_exact(&mut body).ok()?;
        Some(Req {
            method,
            target,
            headers,
            body,
        })
    }

    fn params(s: &str) -> HashMap<String, String> {
        s.split('&')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| {
                let dec = |x: &str| {
                    percent_encoding::percent_decode_str(&x.replace('+', " "))
                        .decode_utf8_lossy()
                        .to_string()
                };
                (dec(k), dec(v))
            })
            .collect()
    }

    fn json_path(p: &str) -> String {
        serde_json::from_str::<Vec<String>>(p)
            .map(|v| v[0].clone())
            .unwrap_or_else(|_| p.to_string())
    }

    fn handle(fake: &Mutex<Fake>, req: Req) -> (String, Vec<u8>) {
        let (path, query) = req.target.split_once('?').unwrap_or((&req.target, ""));
        let mut p = params(query);
        let ctype = req.headers.get("content-type").cloned().unwrap_or_default();
        if ctype.starts_with("application/x-www-form-urlencoded") {
            p.extend(params(&String::from_utf8_lossy(&req.body)));
        }
        let ok = |d: Value| json!({"success": true, "data": d}).to_string().into_bytes();
        let fail = |c: i64| {
            json!({"success": false, "error": {"code": c}})
                .to_string()
                .into_bytes()
        };
        let j = "application/json".to_string();
        let mut f = fake.lock().unwrap();
        if path == "/webapi/query.cgi" {
            return (
                j,
                ok(json!({
                    "SYNO.API.Auth": {"path": "entry.cgi", "minVersion": 1, "maxVersion": 7},
                    "SYNO.FileStation.List": {"path": "entry.cgi", "minVersion": 1, "maxVersion": 2},
                    "SYNO.FileStation.Upload": {"path": "entry.cgi", "minVersion": 1, "maxVersion": 3},
                    "SYNO.FileStation.Download": {"path": "entry.cgi", "minVersion": 1, "maxVersion": 2},
                    "SYNO.FileStation.Delete": {"path": "entry.cgi", "minVersion": 1, "maxVersion": 2},
                })),
            );
        }
        let api = p.get("api").cloned().unwrap_or_default();
        if api != "SYNO.API.Auth" && p.get("_sid").map(String::as_str) != Some("SID") {
            return (j, fail(119));
        }
        match (api.as_str(), p.get("method").map(String::as_str)) {
            ("SYNO.API.Auth", Some("login")) => {
                if p.get("passwd").map(String::as_str) != Some("pw") {
                    return (j, fail(400));
                }
                let otp = p.get("otp_code").cloned();
                let did = p.get("device_id").cloned();
                f.logins.push(format!("{otp:?}/{did:?}"));
                match (otp.as_deref(), did.as_deref()) {
                    (Some("123456"), _) => (j, ok(json!({"sid": "SID", "did": "DEV1"}))),
                    (Some(_), _) => (j, fail(404)),
                    (None, Some("DEV1")) => (j, ok(json!({"sid": "SID"}))),
                    _ => (j, fail(403)),
                }
            }
            ("SYNO.API.Auth", Some("logout")) => (j, ok(Value::Null)),
            ("SYNO.FileStation.List", Some("list")) => {
                let dir = p["folder_path"].clone();
                let prefix = format!("{dir}/");
                if !f.files.keys().any(|k| k.starts_with(&prefix)) {
                    return (j, fail(408));
                }
                let mut entries: HashMap<String, Value> = HashMap::new();
                for (k, (data, mtime)) in &f.files {
                    let Some(rest) = k.strip_prefix(&prefix) else {
                        continue;
                    };
                    match rest.split_once('/') {
                        Some((d, _)) => {
                            entries.insert(d.into(), json!({"name": d, "isdir": true}));
                        }
                        None => {
                            entries.insert(
                                rest.into(),
                                json!({"name": rest, "isdir": false,
                                    "additional": {"size": data.len(), "time": {"mtime": mtime}}}),
                            );
                        }
                    }
                }
                let mut all: Vec<Value> = entries.into_values().collect();
                all.sort_by_key(|v| v["name"].as_str().unwrap().to_string());
                let off: usize = p["offset"].parse().unwrap();
                let lim: usize = 1; // 페이지 처리 검증용으로 1개씩
                let total = all.len();
                let page: Vec<Value> = all.into_iter().skip(off).take(lim).collect();
                (j, ok(json!({"files": page, "total": total, "offset": off})))
            }
            ("SYNO.FileStation.List", Some("getinfo")) => {
                let path = json_path(&p["path"]);
                let is_share = !path.trim_start_matches('/').contains('/');
                let file = if is_share {
                    json!({"name": path, "isdir": true})
                } else {
                    match f.files.get(&path) {
                        Some((d, m)) => json!({"name": path, "isdir": false,
                            "additional": {"size": d.len(), "time": {"mtime": m}}}),
                        None => json!({"code": 408, "path": path}),
                    }
                };
                (j, ok(json!({"files": [file]})))
            }
            ("SYNO.FileStation.Upload", Some("upload")) => {
                let boundary = ctype.split("boundary=").nth(1).unwrap().to_string();
                let body = req.body;
                let sep = format!("--{boundary}").into_bytes();
                let mut fields = HashMap::new();
                let mut file = Vec::new();
                let mut fname = String::new();
                let mut i = 0;
                let find = |hay: &[u8], needle: &[u8], from: usize| {
                    hay[from..]
                        .windows(needle.len())
                        .position(|w| w == needle)
                        .map(|x| x + from)
                };
                while let Some(start) = find(&body, &sep, i) {
                    let hs = start + sep.len() + 2;
                    let Some(he) = find(&body, b"\r\n\r\n", hs) else {
                        break;
                    };
                    let header = String::from_utf8_lossy(&body[hs..he]).to_string();
                    let ds = he + 4;
                    let de = find(&body, &[b"\r\n".as_slice(), &sep].concat(), ds).unwrap();
                    let name = header
                        .split("name=\"")
                        .nth(1)
                        .unwrap()
                        .split('"')
                        .next()
                        .unwrap();
                    if name == "file" {
                        fname = header
                            .split("filename=\"")
                            .nth(1)
                            .unwrap()
                            .split('"')
                            .next()
                            .unwrap()
                            .to_string();
                        file = body[ds..de].to_vec();
                    } else {
                        fields.insert(
                            name.to_string(),
                            String::from_utf8_lossy(&body[ds..de]).to_string(),
                        );
                    }
                    i = de + 2;
                }
                assert_eq!(fields["overwrite"], "true");
                assert_eq!(fields["create_parents"], "true");
                f.tick += 1;
                let t = f.tick;
                f.files
                    .insert(format!("{}/{fname}", fields["path"]), (file, t));
                (j, ok(Value::Null))
            }
            ("SYNO.FileStation.Download", Some("download")) => {
                let path = json_path(&p["path"]);
                match f.files.get(&path) {
                    Some((d, _)) => ("application/octet-stream".into(), d.clone()),
                    None => (j, fail(408)),
                }
            }
            ("SYNO.FileStation.Delete", Some("delete")) => {
                let path = json_path(&p["path"]);
                if f.files.remove(&path).is_some() {
                    (j, ok(Value::Null))
                } else {
                    (j, json!({"success": false, "error": {"code": 900, "errors": [{"code": 408, "path": path}]}}).to_string().into_bytes())
                }
            }
            _ => (j, fail(103)),
        }
    }

    fn serve() -> (String, Arc<Mutex<Fake>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let fake = Arc::new(Mutex::new(Fake::default()));
        let f2 = fake.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { continue };
                let f3 = f2.clone();
                std::thread::spawn(move || {
                    while let Some(req) = read_req(&mut s) {
                        let _ = req.method;
                        let (ct, body) = handle(&f3, req);
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {}\r\n\r\n",
                            body.len()
                        );
                        if s.write_all(head.as_bytes()).is_err() || s.write_all(&body).is_err() {
                            break;
                        }
                    }
                });
            }
        });
        (url, fake)
    }

    #[test]
    fn full_flow_with_otp_and_device_token() {
        let (url, fake) = serve();
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(&url, "/home/Obsidian");

        // OTP 없이 로그인 → OTP 필요 오류
        let e = login(&c, dir.path(), None).unwrap_err();
        assert!(needs_otp(&e), "{e:#}");
        // 틀린 OTP
        assert!(needs_otp(
            &login(&c, dir.path(), Some("000000")).unwrap_err()
        ));
        // 맞는 OTP → 기기 토큰 저장
        login(&c, dir.path(), Some("123456")).unwrap();
        assert!(has_device_token(dir.path(), &c));

        // 이후에는 기기 토큰으로 OTP 없이 동작
        let mut b = SynologyBackend::new(&c, dir.path()).unwrap();
        b.check().unwrap();
        assert!(b.list().unwrap().is_empty());

        let src = dir.path().join("src.bin");
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&src, &big).unwrap();
        let e1 = b.upload_file("사진/2024/a,b x.jpg", &src).unwrap();
        assert!(b.upload_file("q\"x\".jpg", &src).is_err());
        assert_eq!(e1.size, big.len() as u64);
        b.upload("노트.md", b"hello").unwrap();
        b.upload("z.md", b"z").unwrap();

        let mut list = b.list().unwrap();
        list.sort_by(|a, b| a.path.cmp(&b.path));
        let names: Vec<&str> = list.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(names, vec!["z.md", "노트.md", "사진/2024/a,b x.jpg"]);
        assert_eq!(list[2].rev, e1.rev);

        let mut out = Vec::new();
        b.download_to("사진/2024/a,b x.jpg", &mut out).unwrap();
        assert_eq!(out, big);

        // 세션 만료 → 자동 재로그인
        b.sid = Some("EXPIRED".into());
        assert_eq!(b.download("노트.md").unwrap(), b"hello");

        b.delete("노트.md").unwrap();
        b.delete("노트.md").unwrap(); // 이미 없음 → 성공
        assert_eq!(b.list().unwrap().len(), 2);
        drop(b);

        let logins = fake.lock().unwrap().logins.clone();
        assert!(logins.iter().any(|l| l.contains("DEV1")), "{logins:?}");
    }
}
