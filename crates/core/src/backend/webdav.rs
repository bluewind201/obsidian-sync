//! WebDAV 저장소 (Synology DSM의 WebDAV Server 패키지, Nextcloud, 기타)
use super::{check_path, encode_path, Backend, RemoteEntry};
use crate::http::{self, status_of};
use crate::xml;
use anyhow::{anyhow, Result};
use base64::Engine;
use percent_encoding::percent_decode_str;
use std::collections::HashSet;

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/><d:getlastmodified/><d:getetag/></d:prop></d:propfind>"#;

pub struct WebDavBackend {
    base: String,
    base_path: String,
    auth: String,
    agent: ureq::Agent,
    known_dirs: HashSet<String>,
    base_ready: bool,
}

struct DavItem {
    rel: String,
    is_dir: bool,
    size: u64,
    rev: String,
}

fn url_path(url: &str) -> String {
    // scheme://host[:port]/path → /path
    let after = url.split_once("://").map(|x| x.1).unwrap_or(url);
    match after.find('/') {
        Some(i) => after[i..].to_string(),
        None => "/".into(),
    }
}

fn norm_etag(s: &str) -> String {
    s.trim()
        .trim_start_matches("W/")
        .trim_matches('"')
        .to_string()
}

impl WebDavBackend {
    pub fn new(url: &str, user: &str, pass: &str) -> Self {
        let base = url.trim().trim_end_matches('/').to_string();
        let base_path = percent_decode_str(&url_path(&base))
            .decode_utf8_lossy()
            .trim_end_matches('/')
            .to_string();
        let auth = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
        );
        Self {
            base,
            base_path,
            auth,
            agent: http::agent(),
            known_dirs: HashSet::new(),
            base_ready: false,
        }
    }

    fn url(&self, rel: &str) -> String {
        if rel.is_empty() {
            format!("{}/", self.base)
        } else {
            format!("{}/{}", self.base, encode_path(rel))
        }
    }

    fn rel_from_href(&self, href: &str) -> Option<String> {
        let path = if href.starts_with("http://") || href.starts_with("https://") {
            url_path(href)
        } else {
            href.to_string()
        };
        let decoded = percent_decode_str(&path).decode_utf8_lossy().to_string();
        let rest = decoded.strip_prefix(&self.base_path)?;
        Some(rest.trim_matches('/').to_string())
    }

    fn propfind(&self, rel: &str, depth: &str) -> Result<Vec<DavItem>> {
        let mut url = self.url(rel);
        if !rel.is_empty() && depth == "1" {
            url.push('/');
        }
        let resp = http::check(
            self.agent
                .request("PROPFIND", &url)
                .set("Authorization", &self.auth)
                .set("Depth", depth)
                .set("Content-Type", "application/xml; charset=utf-8")
                .send_string(PROPFIND_BODY),
        )?;
        let text = resp.into_string()?;
        let root = xml::parse(&text)?;
        let ms = root
            .child("multistatus")
            .ok_or_else(|| anyhow!("WebDAV 응답 형식 오류"))?;
        let mut out = Vec::new();
        for r in ms.children_named("response") {
            let Some(href) = r.text_at(&["href"]) else {
                continue;
            };
            let Some(rel) = self.rel_from_href(href) else {
                continue;
            };
            // 200 OK propstat 우선 사용
            let prop = r
                .children_named("propstat")
                .find(|ps| {
                    ps.text_at(&["status"])
                        .map(|s| s.contains(" 200"))
                        .unwrap_or(true)
                })
                .and_then(|ps| ps.child("prop"));
            let Some(prop) = prop else { continue };
            let is_dir = prop
                .child("resourcetype")
                .map(|rt| rt.child("collection").is_some())
                .unwrap_or(false);
            let size = prop
                .text_at(&["getcontentlength"])
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            let etag = prop
                .text_at(&["getetag"])
                .map(norm_etag)
                .unwrap_or_default();
            let rev = if etag.is_empty() {
                format!(
                    "{}-{}",
                    size,
                    prop.text_at(&["getlastmodified"]).unwrap_or("").trim()
                )
            } else {
                etag
            };
            out.push(DavItem {
                rel,
                is_dir,
                size,
                rev,
            });
        }
        Ok(out)
    }

    fn mkcol(&self, url: &str) -> Result<()> {
        match http::check(
            self.agent
                .request("MKCOL", url)
                .set("Authorization", &self.auth)
                .call(),
        ) {
            Ok(_) => Ok(()),
            // 405: 이미 존재
            Err(e) if status_of(&e) == Some(405) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// 동기화 루트 폴더가 없으면 상위부터 차례로 생성
    fn ensure_base(&mut self) -> Result<()> {
        if self.base_ready {
            return Ok(());
        }
        match self.propfind("", "0") {
            Ok(_) => {}
            Err(e) if status_of(&e) == Some(404) => {
                let root = &self.base[..self.base.len() - url_path(&self.base).len()];
                let segs: Vec<String> = url_path(&self.base)
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
                let mut cur = root.to_string();
                for seg in segs {
                    cur.push('/');
                    cur.push_str(&seg);
                    match self.mkcol(&format!("{cur}/")) {
                        Ok(()) => {}
                        // 이미 있는 공유 폴더 등은 권한상 만들 수 없다는 응답이 올 수 있음
                        Err(e) if matches!(status_of(&e), Some(403) | Some(401)) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Err(e) => return Err(e),
        }
        self.base_ready = true;
        Ok(())
    }

    fn ensure_dirs(&mut self, rel_file: &str) -> Result<()> {
        self.ensure_base()?;
        let segs: Vec<&str> = rel_file.split('/').collect();
        let mut cur = String::new();
        for seg in &segs[..segs.len().saturating_sub(1)] {
            if !cur.is_empty() {
                cur.push('/');
            }
            cur.push_str(seg);
            if self.known_dirs.contains(&cur) {
                continue;
            }
            let url = format!("{}/", self.url(&cur));
            self.mkcol(&url)?;
            self.known_dirs.insert(cur.clone());
        }
        Ok(())
    }
}

impl Backend for WebDavBackend {
    fn id(&self) -> String {
        format!("webdav:{}", self.base)
    }

    fn list(&mut self) -> Result<Vec<RemoteEntry>> {
        let mut files = Vec::new();
        let mut queue = vec![String::new()];
        self.known_dirs.clear();
        while let Some(dir) = queue.pop() {
            let items = match self.propfind(&dir, "1") {
                Ok(i) => i,
                Err(e) if dir.is_empty() && status_of(&e) == Some(404) => {
                    // 원격 폴더가 아직 없음 → 비어 있는 것으로 간주
                    self.base_ready = false;
                    return Ok(files);
                }
                Err(e) => return Err(e),
            };
            if dir.is_empty() {
                self.base_ready = true;
            }
            for it in items {
                if it.rel == dir || it.rel.is_empty() {
                    continue;
                }
                if it.is_dir {
                    self.known_dirs.insert(it.rel.clone());
                    queue.push(it.rel);
                } else {
                    files.push(RemoteEntry {
                        path: it.rel,
                        size: it.size,
                        rev: it.rev,
                    });
                }
            }
        }
        Ok(files)
    }

    fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        check_path(path)?;
        let resp = http::check(
            self.agent
                .get(&self.url(path))
                .set("Authorization", &self.auth)
                .call(),
        )?;
        http::read_bytes(resp)
    }

    fn upload(&mut self, path: &str, data: &[u8]) -> Result<RemoteEntry> {
        check_path(path)?;
        self.ensure_dirs(path)?;
        http::check(
            self.agent
                .put(&self.url(path))
                .set("Authorization", &self.auth)
                .set("Content-Type", "application/octet-stream")
                .send_bytes(data),
        )?;
        let items = self.propfind(path, "0")?;
        let it = items
            .into_iter()
            .find(|i| !i.is_dir)
            .ok_or_else(|| anyhow!("업로드 확인 실패: {path}"))?;
        Ok(RemoteEntry {
            path: path.into(),
            size: it.size,
            rev: it.rev,
        })
    }

    fn delete(&mut self, path: &str) -> Result<()> {
        check_path(path)?;
        match http::check(
            self.agent
                .delete(&self.url(path))
                .set("Authorization", &self.auth)
                .call(),
        ) {
            Ok(_) => Ok(()),
            Err(e) if status_of(&e) == Some(404) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn href_to_rel() {
        let b = WebDavBackend::new("https://nas.local:5006/home/Obsidian%20Vault/", "u", "p");
        assert_eq!(b.base_path, "/home/Obsidian Vault");
        assert_eq!(
            b.rel_from_href("/home/Obsidian%20Vault/notes/%ED%95%9C.md")
                .as_deref(),
            Some("notes/한.md")
        );
        assert_eq!(
            b.rel_from_href("https://nas.local:5006/home/Obsidian%20Vault/")
                .as_deref(),
            Some("")
        );
        assert_eq!(
            b.url("a b/c.md"),
            "https://nas.local:5006/home/Obsidian%20Vault/a%20b/c.md"
        );
    }
}
