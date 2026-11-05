use anyhow::{anyhow, Result};
use std::io::Read;
use std::time::Duration;

/// HTTP 오류 (상태 코드 확인용)
#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub url: String,
    pub body: String,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body: String = self.body.chars().take(300).collect();
        write!(f, "HTTP {} ({}) {}", self.status, self.url, body)
    }
}
impl std::error::Error for HttpError {}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(300))
        .timeout_write(Duration::from_secs(300))
        .user_agent(concat!("obsidian-sync/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// ureq 결과를 anyhow 결과로 변환 (상태코드 오류는 [`HttpError`])
pub fn check(r: std::result::Result<ureq::Response, ureq::Error>) -> Result<ureq::Response> {
    match r {
        Ok(resp) => Ok(resp),
        Err(ureq::Error::Status(status, resp)) => {
            let url = resp.get_url().to_string();
            let body = resp.into_string().unwrap_or_default();
            Err(HttpError { status, url, body }.into())
        }
        Err(e) => Err(anyhow!("네트워크 오류: {e}")),
    }
}

/// 오류가 특정 HTTP 상태코드인지 확인
pub fn status_of(e: &anyhow::Error) -> Option<u16> {
    e.downcast_ref::<HttpError>().map(|h| h.status)
}

pub fn read_bytes(resp: ureq::Response) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn read_json(resp: ureq::Response) -> Result<serde_json::Value> {
    let bytes = read_bytes(resp)?;
    Ok(serde_json::from_slice(&bytes)?)
}
