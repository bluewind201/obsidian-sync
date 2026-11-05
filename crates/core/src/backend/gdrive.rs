//! Google Drive (Drive API v3, `drive.file` 범위)
//!
//! `drive.file` 범위는 이 앱이 만든 파일만 볼 수 있습니다.
//! 따라서 Windows/Android 모두 같은 OAuth Client ID를 사용해야 서로의 파일이 보입니다.
use super::{check_path, Backend, RemoteEntry};
use crate::auth::TokenManager;
use crate::config::BackendConfig;
use crate::http::{self, status_of};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

const API: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const SIMPLE_LIMIT: usize = 5 * 1024 * 1024;
const FIELDS: &str = "id,name,mimeType,md5Checksum,size,modifiedTime";

pub struct GDriveBackend {
    folder: String,
    tokens: TokenManager,
    agent: ureq::Agent,
    /// 상대 폴더 경로 → 폴더 ID ("" = 동기화 루트)
    folders: HashMap<String, String>,
    /// 상대 파일 경로 → 파일 ID
    files: HashMap<String, String>,
}

fn q_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn entry_from(v: &Value, rel: &str) -> RemoteEntry {
    let rev = v
        .get("md5Checksum")
        .or_else(|| v.get("modifiedTime"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    RemoteEntry {
        path: rel.to_string(),
        size: v
            .get("size")
            .and_then(|x| x.as_str().and_then(|s| s.parse().ok()).or(x.as_u64()))
            .unwrap_or(0),
        rev,
    }
}

impl GDriveBackend {
    pub fn new(cfg: &BackendConfig, folder: &str, data_dir: &Path) -> Result<Self> {
        Ok(Self {
            folder: folder.trim().trim_matches('/').to_string(),
            tokens: TokenManager::new(data_dir, cfg)?,
            agent: http::agent(),
            folders: HashMap::new(),
            files: HashMap::new(),
        })
    }

    fn send(
        &mut self,
        make: &dyn Fn(&ureq::Agent) -> ureq::Request,
        body: Option<&[u8]>,
    ) -> Result<ureq::Response> {
        for attempt in 0..2 {
            let tok = self.tokens.access_token()?;
            let req = make(&self.agent).set("Authorization", &format!("Bearer {tok}"));
            let r = match body {
                Some(b) => req.send_bytes(b),
                None => req.call(),
            };
            match http::check(r) {
                Err(e) if attempt == 0 && status_of(&e) == Some(401) => {
                    self.tokens.invalidate();
                    continue;
                }
                other => return other,
            }
        }
        unreachable!()
    }

    /// 부모 폴더 안의 항목 전부 조회
    fn children(&mut self, parent_id: &str, extra_q: &str) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        let q = format!(
            "'{}' in parents and trashed = false{}",
            q_escape(parent_id),
            extra_q
        );
        let fields = format!("nextPageToken,files({FIELDS})");
        loop {
            let pt = page.clone();
            let resp = self.send(
                &|a| {
                    let mut r = a
                        .get(API)
                        .query("q", &q)
                        .query("fields", &fields)
                        .query("pageSize", "1000")
                        .query("spaces", "drive");
                    if let Some(p) = &pt {
                        r = r.query("pageToken", p);
                    }
                    r
                },
                None,
            )?;
            let v = http::read_json(resp)?;
            if let Some(arr) = v.get("files").and_then(|x| x.as_array()) {
                out.extend(arr.iter().cloned());
            }
            page = v
                .get("nextPageToken")
                .and_then(|x| x.as_str())
                .map(String::from);
            if page.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn create_folder(&mut self, name: &str, parent_id: &str) -> Result<String> {
        let body =
            json!({"name": name, "mimeType": FOLDER_MIME, "parents": [parent_id]}).to_string();
        let resp = self.send(
            &|a| {
                a.post(API)
                    .query("fields", "id")
                    .set("Content-Type", "application/json; charset=UTF-8")
            },
            Some(body.as_bytes()),
        )?;
        let v = http::read_json(resp)?;
        v.get("id")
            .and_then(|x| x.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow!("폴더 생성 실패: {name}"))
    }

    /// 동기화 루트 폴더 ID (없으면 생성)
    fn root_id(&mut self) -> Result<String> {
        if let Some(id) = self.folders.get("") {
            return Ok(id.clone());
        }
        let mut parent = "root".to_string();
        let segs: Vec<String> = self.folder.split('/').map(String::from).collect();
        for seg in segs {
            let q = format!(
                " and name = '{}' and mimeType = '{}'",
                q_escape(&seg),
                FOLDER_MIME
            );
            let found = self.children(&parent, &q)?;
            parent = match found
                .first()
                .and_then(|v| v.get("id"))
                .and_then(|x| x.as_str())
            {
                Some(id) => id.to_string(),
                None => self.create_folder(&seg, &parent)?,
            };
        }
        self.folders.insert(String::new(), parent.clone());
        Ok(parent)
    }

    fn ensure_dir(&mut self, rel_dir: &str) -> Result<String> {
        if let Some(id) = self.folders.get(rel_dir) {
            return Ok(id.clone());
        }
        let (parent_rel, name) = match rel_dir.rsplit_once('/') {
            Some((p, n)) => (p.to_string(), n.to_string()),
            None => (String::new(), rel_dir.to_string()),
        };
        let parent_id = if parent_rel.is_empty() {
            self.root_id()?
        } else {
            self.ensure_dir(&parent_rel)?
        };
        let id = self.create_folder(&name, &parent_id)?;
        self.folders.insert(rel_dir.to_string(), id.clone());
        Ok(id)
    }

    fn upload_resumable(
        &mut self,
        init_url: String,
        method: &str,
        meta: String,
        data: &[u8],
    ) -> Result<Value> {
        let m = method.to_string();
        let resp = self.send(
            &|a| {
                a.request(&m, &init_url)
                    .set("Content-Type", "application/json; charset=UTF-8")
                    .set("X-Upload-Content-Type", "application/octet-stream")
            },
            Some(meta.as_bytes()),
        )?;
        let session = resp
            .header("Location")
            .ok_or_else(|| anyhow!("업로드 세션 생성 실패"))?
            .to_string();
        let resp = self.send(&|a| a.put(&session), Some(data))?;
        http::read_json(resp)
    }
}

impl Backend for GDriveBackend {
    fn id(&self) -> String {
        format!("gdrive:{}", self.folder)
    }

    fn list(&mut self) -> Result<Vec<RemoteEntry>> {
        self.folders.clear();
        self.files.clear();
        let root = self.root_id()?;
        let mut out = Vec::new();
        let mut queue = vec![(String::new(), root)];
        while let Some((rel, id)) = queue.pop() {
            for item in self.children(&id, "")? {
                let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
                let item_id = item.get("id").and_then(|x| x.as_str()).unwrap_or("");
                if name.is_empty() || item_id.is_empty() || name.contains('/') {
                    continue;
                }
                let child = if rel.is_empty() {
                    name.to_string()
                } else {
                    format!("{rel}/{name}")
                };
                let mime = item.get("mimeType").and_then(|x| x.as_str()).unwrap_or("");
                if mime == FOLDER_MIME {
                    if !self.folders.contains_key(&child) {
                        self.folders.insert(child.clone(), item_id.to_string());
                        queue.push((child, item_id.to_string()));
                    }
                } else if mime.starts_with("application/vnd.google-apps.") {
                    // 구글 문서 등 내려받을 수 없는 형식은 건너뜀
                    continue;
                } else if !self.files.contains_key(&child) {
                    self.files.insert(child.clone(), item_id.to_string());
                    out.push(entry_from(&item, &child));
                }
            }
        }
        Ok(out)
    }

    fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        check_path(path)?;
        let id = self
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow!("원격 파일을 찾을 수 없습니다: {path}"))?;
        let url = format!("{API}/{id}");
        let resp = self.send(&|a| a.get(&url).query("alt", "media"), None)?;
        http::read_bytes(resp)
    }

    fn upload(&mut self, path: &str, data: &[u8]) -> Result<RemoteEntry> {
        check_path(path)?;
        let v = if let Some(id) = self.files.get(path).cloned() {
            if data.len() <= SIMPLE_LIMIT {
                let url = format!("{UPLOAD}/{id}");
                let resp = self.send(
                    &|a| {
                        a.request("PATCH", &url)
                            .query("uploadType", "media")
                            .query("fields", FIELDS)
                            .set("Content-Type", "application/octet-stream")
                    },
                    Some(data),
                )?;
                http::read_json(resp)?
            } else {
                let url = format!("{UPLOAD}/{id}?uploadType=resumable&fields={FIELDS}");
                self.upload_resumable(url, "PATCH", "{}".into(), data)?
            }
        } else {
            let (dir, name) = match path.rsplit_once('/') {
                Some((d, n)) => (d.to_string(), n.to_string()),
                None => (String::new(), path.to_string()),
            };
            let parent = if dir.is_empty() {
                self.root_id()?
            } else {
                self.ensure_dir(&dir)?
            };
            let meta = json!({"name": name, "parents": [parent]}).to_string();
            if data.len() <= SIMPLE_LIMIT {
                let boundary = format!(
                    "obsync{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                );
                let mut body = Vec::with_capacity(data.len() + meta.len() + 256);
                body.extend_from_slice(
                    format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(data);
                body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
                let ct = format!("multipart/related; boundary={boundary}");
                let resp = self.send(
                    &|a| {
                        a.post(UPLOAD)
                            .query("uploadType", "multipart")
                            .query("fields", FIELDS)
                            .set("Content-Type", &ct)
                    },
                    Some(&body),
                )?;
                http::read_json(resp)?
            } else {
                let url = format!("{UPLOAD}?uploadType=resumable&fields={FIELDS}");
                self.upload_resumable(url, "POST", meta, data)?
            }
        };
        if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
            self.files.insert(path.to_string(), id.to_string());
        }
        Ok(entry_from(&v, path))
    }

    fn delete(&mut self, path: &str) -> Result<()> {
        check_path(path)?;
        let Some(id) = self.files.get(path).cloned() else {
            return Ok(());
        };
        // 영구 삭제 대신 휴지통으로 이동 (복구 가능)
        let url = format!("{API}/{id}");
        let body = br#"{"trashed":true}"#;
        match self.send(
            &|a| {
                a.request("PATCH", &url)
                    .set("Content-Type", "application/json")
            },
            Some(body),
        ) {
            Ok(_) => {}
            Err(e) if status_of(&e) == Some(404) => {}
            Err(e) => return Err(e),
        }
        self.files.remove(path);
        Ok(())
    }
}
