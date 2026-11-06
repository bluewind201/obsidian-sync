//! S3 호환 오브젝트 스토리지 (AWS Signature V4, path-style)
//!
//! - 네이버 클라우드 Object Storage: endpoint `https://kr.object.ncloudstorage.com`, region `kr-standard`
//! - Google Cloud Storage (HMAC 키): endpoint `https://storage.googleapis.com`, region `auto`
//! - Synology C2 Object Storage, AWS S3, MinIO 등
use super::{check_path, encode_path, Backend, RemoteEntry};
use crate::http::{self, status_of};
use crate::xml;
use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub struct S3Backend {
    endpoint: String,
    host: String,
    region: String,
    bucket: String,
    prefix: String,
    access_key: String,
    secret_key: String,
    agent: ureq::Agent,
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key).expect("hmac key");
    m.update(data.as_bytes());
    m.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn enc(s: &str) -> String {
    encode_path(s).replace('/', "%2F")
}

impl S3Backend {
    pub fn new(
        endpoint: &str,
        region: &str,
        bucket: &str,
        prefix: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        let endpoint = endpoint.trim().trim_end_matches('/').to_string();
        let endpoint = if endpoint.contains("://") {
            endpoint
        } else {
            format!("https://{endpoint}")
        };
        let host = endpoint
            .split_once("://")
            .map(|x| x.1)
            .unwrap_or(&endpoint)
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        let mut prefix = prefix.trim().trim_matches('/').to_string();
        if !prefix.is_empty() {
            prefix.push('/');
        }
        Self {
            endpoint,
            host,
            region: region.trim().to_string(),
            bucket: bucket.trim().to_string(),
            prefix,
            access_key: access_key.trim().to_string(),
            secret_key: secret_key.trim().to_string(),
            agent: http::agent(),
        }
    }

    /// 서명된 요청 생성. `key`는 버킷 내 전체 키 (빈 문자열이면 버킷 자체)
    fn request(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, String)],
        payload: &[u8],
    ) -> ureq::Request {
        self.request_at(method, key, query, payload, chrono::Utc::now())
    }

    fn request_at(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, String)],
        payload: &[u8],
        now: chrono::DateTime<chrono::Utc>,
    ) -> ureq::Request {
        let (url, headers) = self.sign(method, key, query, payload, now);
        let mut req = self.agent.request(method, &url);
        for (k, v) in headers {
            req = req.set(&k, &v);
        }
        req
    }

    fn sign(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, String)],
        payload: &[u8],
        now: chrono::DateTime<chrono::Utc>,
    ) -> (String, Vec<(String, String)>) {
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let mut uri = format!("/{}", encode_path(&self.bucket));
        if !key.is_empty() {
            uri.push('/');
            uri.push_str(&encode_path(key));
        }
        let mut q: Vec<(String, String)> = query.iter().map(|(k, v)| (enc(k), enc(v))).collect();
        q.sort();
        let canonical_query = q
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let payload_hash = sha256_hex(payload);
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "{method}\n{uri}\n{canonical_query}\nhost:{}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n\n{signed_headers}\n{payload_hash}",
            self.host
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let k_date = hmac(format!("AWS4{}", self.secret_key).as_bytes(), &date);
        let k_region = hmac(&k_date, &self.region);
        let k_service = hmac(&k_region, "s3");
        let k_signing = hmac(&k_service, "aws4_request");
        let signature = hex::encode(hmac(&k_signing, &string_to_sign));
        let auth = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key
        );
        let mut url = format!("{}{}", self.endpoint, uri);
        if !canonical_query.is_empty() {
            url.push('?');
            url.push_str(&canonical_query);
        }
        (
            url,
            vec![
                ("x-amz-date".into(), amz_date),
                ("x-amz-content-sha256".into(), payload_hash),
                ("Authorization".into(), auth),
            ],
        )
    }

    fn key(&self, rel: &str) -> String {
        format!("{}{}", self.prefix, rel)
    }
}

impl Backend for S3Backend {
    fn id(&self) -> String {
        format!("s3:{}/{}/{}", self.endpoint, self.bucket, self.prefix)
    }

    fn list(&mut self) -> Result<Vec<RemoteEntry>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut q: Vec<(&str, String)> = vec![("list-type", "2".into())];
            if !self.prefix.is_empty() {
                q.push(("prefix", self.prefix.clone()));
            }
            if let Some(t) = &token {
                q.push(("continuation-token", t.clone()));
            }
            let resp = http::check(self.request("GET", "", &q, b"").call())?;
            let text = resp.into_string()?;
            let root = xml::parse(&text)?;
            let Some(res) = root.child("ListBucketResult") else {
                anyhow::bail!("S3 목록 응답 형식 오류");
            };
            for c in res.children_named("Contents") {
                let key = c.text_at(&["Key"]).unwrap_or("");
                if key.ends_with('/') {
                    continue;
                }
                let Some(rel) = key.strip_prefix(&self.prefix) else {
                    continue;
                };
                if rel.is_empty() {
                    continue;
                }
                out.push(RemoteEntry {
                    path: rel.to_string(),
                    size: c
                        .text_at(&["Size"])
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    rev: c
                        .text_at(&["ETag"])
                        .unwrap_or("")
                        .trim_matches('"')
                        .to_string(),
                });
            }
            let truncated = res
                .text_at(&["IsTruncated"])
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            token = res
                .text_at(&["NextContinuationToken"])
                .map(String::from)
                .filter(|s| !s.is_empty());
            if !truncated || token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        check_path(path)?;
        let resp = http::check(self.request("GET", &self.key(path), &[], b"").call())?;
        http::read_bytes(resp)
    }

    fn upload(&mut self, path: &str, data: &[u8]) -> Result<RemoteEntry> {
        check_path(path)?;
        let resp = http::check(
            self.request("PUT", &self.key(path), &[], data)
                .set("Content-Type", "application/octet-stream")
                .send_bytes(data),
        )?;
        let etag = resp
            .header("ETag")
            .map(|s| s.trim_matches('"').to_string())
            .unwrap_or_else(|| format!("{:x}", md5_fallback(data)));
        Ok(RemoteEntry {
            path: path.into(),
            size: data.len() as u64,
            rev: etag,
        })
    }

    fn delete(&mut self, path: &str) -> Result<()> {
        check_path(path)?;
        match http::check(self.request("DELETE", &self.key(path), &[], b"").call()) {
            Ok(_) => Ok(()),
            Err(e) if status_of(&e) == Some(404) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// ETag 헤더가 없을 때를 위한 대체값 (다음 목록 조회 시 한 번 재확인됨)
fn md5_fallback(data: &[u8]) -> u128 {
    let h = Sha256::digest(data);
    u128::from_be_bytes(h[..16].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// AWS 공식 문서(GET Bucket List Objects 예제)의 서명값으로 검증
    #[test]
    fn sigv4_matches_aws_example() {
        let b = S3Backend::new(
            "https://examplebucket.s3.amazonaws.com",
            "us-east-1",
            "",
            "",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        let now = chrono::Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap();
        let (_, headers) = b.sign(
            "GET",
            "",
            &[("max-keys", "2".into()), ("prefix", "J".into())],
            b"",
            now,
        );
        let auth = &headers.iter().find(|h| h.0 == "Authorization").unwrap().1;
        assert!(
            auth.ends_with(
                "Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
            ),
            "{auth}"
        );
    }

    #[test]
    fn key_encoding() {
        let b = S3Backend::new(
            "kr.object.ncloudstorage.com",
            "kr-standard",
            "bk",
            "/vault/",
            "a",
            "s",
        );
        assert_eq!(b.host, "kr.object.ncloudstorage.com");
        assert_eq!(b.key("노트 1.md"), "vault/노트 1.md");
        let now = chrono::Utc::now();
        let (url, _) = b.sign("GET", &b.key("a b.md"), &[], b"", now);
        assert_eq!(url, "https://kr.object.ncloudstorage.com/bk/vault/a%20b.md");
    }
}
