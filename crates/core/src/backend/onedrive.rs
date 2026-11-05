//! Microsoft OneDrive (Graph API)
use super::{check_path, encode_path, Backend, RemoteEntry};
use crate::auth::TokenManager;
use crate::config::BackendConfig;
use crate::http::{self, status_of};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::Path;

const GRAPH: &str = "https://graph.microsoft.com/v1.0/me/drive";
const SIMPLE_LIMIT: usize = 4 * 1024 * 1024;
const CHUNK: usize = 320 * 1024 * 32; // 10 MiB (320KiB 배수)

pub struct OneDriveBackend {
    folder: String,
    tokens: TokenManager,
    agent: ureq::Agent,
}

fn entry_from(item: &Value, rel: &str) -> RemoteEntry {
    let rev = item
        .get("cTag")
        .or_else(|| item.get("eTag"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    RemoteEntry {
        path: rel.to_string(),
        size: item.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
        rev,
    }
}

impl OneDriveBackend {
    pub fn new(cfg: &BackendConfig, folder: &str, data_dir: &Path) -> Result<Self> {
        Ok(Self {
            folder: folder.trim().trim_matches('/').to_string(),
            tokens: TokenManager::new(data_dir, cfg)?,
            agent: http::agent(),
        })
    }

    fn item_url(&self, rel: &str) -> String {
        let full = if rel.is_empty() {
            self.folder.clone()
        } else {
            format!("{}/{}", self.folder, rel)
        };
        format!("{GRAPH}/root:/{}:", encode_path(&full))
    }

    /// 인증 헤더를 붙여 요청 실행. 401이면 토큰 갱신 후 한 번 재시도
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

    fn list_dir(&mut self, rel: &str, out: &mut Vec<RemoteEntry>) -> Result<()> {
        let mut next = Some(format!(
            "{}/children?$top=1000&$select=name,size,file,folder,cTag,eTag",
            self.item_url(rel)
        ));
        let mut subdirs = Vec::new();
        while let Some(url) = next.take() {
            let resp = self.send(&|a| a.get(&url), None)?;
            let v = http::read_json(resp)?;
            for item in v.get("value").and_then(|x| x.as_array()).into_iter().flatten() {
                let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let child = if rel.is_empty() {
                    name.to_string()
                } else {
                    format!("{rel}/{name}")
                };
                if item.get("folder").is_some() {
                    subdirs.push(child);
                } else if item.get("file").is_some() {
                    out.push(entry_from(item, &child));
                }
            }
            next = v
                .get("@odata.nextLink")
                .and_then(|x| x.as_str())
                .map(String::from);
        }
        for d in subdirs {
            self.list_dir(&d, out)?;
        }
        Ok(())
    }
}

impl Backend for OneDriveBackend {
    fn id(&self) -> String {
        format!("onedrive:{}", self.folder)
    }

    fn list(&mut self) -> Result<Vec<RemoteEntry>> {
        let mut out = Vec::new();
        match self.list_dir("", &mut out) {
            Ok(()) => Ok(out),
            // 폴더가 아직 없음
            Err(e) if status_of(&e) == Some(404) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        check_path(path)?;
        let url = format!("{}/content", self.item_url(path));
        let resp = self.send(&|a| a.get(&url), None)?;
        http::read_bytes(resp)
    }

    fn upload(&mut self, path: &str, data: &[u8]) -> Result<RemoteEntry> {
        check_path(path)?;
        if data.len() <= SIMPLE_LIMIT {
            // 경로 기반 PUT은 기본이 덮어쓰기이며, 상위 폴더도 자동 생성됨
            let url = format!("{}/content", self.item_url(path));
            let resp = self.send(
                &|a| a.put(&url).set("Content-Type", "application/octet-stream"),
                Some(data),
            )?;
            let v = http::read_json(resp)?;
            return Ok(entry_from(&v, path));
        }
        // 대용량: 업로드 세션
        let url = format!("{}/createUploadSession", self.item_url(path));
        let body = br#"{"item":{"@microsoft.graph.conflictBehavior":"replace"}}"#;
        let resp = self.send(
            &|a| a.post(&url).set("Content-Type", "application/json"),
            Some(body),
        )?;
        let v = http::read_json(resp)?;
        let upload_url = v
            .get("uploadUrl")
            .and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("업로드 세션 생성 실패"))?
            .to_string();
        let total = data.len();
        let mut start = 0;
        let mut last = Value::Null;
        while start < total {
            let end = (start + CHUNK).min(total);
            // 업로드 URL은 사전 인증되어 있으므로 Authorization 헤더를 붙이지 않음
            let resp = http::check(
                self.agent
                    .put(&upload_url)
                    .set(
                        "Content-Range",
                        &format!("bytes {}-{}/{}", start, end - 1, total),
                    )
                    .send_bytes(&data[start..end]),
            )?;
            if end == total {
                last = http::read_json(resp)?;
            }
            start = end;
        }
        Ok(entry_from(&last, path))
    }

    fn delete(&mut self, path: &str) -> Result<()> {
        check_path(path)?;
        let url = self.item_url(path);
        match self.send(&|a| a.delete(&url), None) {
            Ok(_) => Ok(()),
            Err(e) if status_of(&e) == Some(404) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
