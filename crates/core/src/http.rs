use anyhow::Result;
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

/// 연결 자체가 실패한 오류 (시간 초과, 끊김 등). 다시 시도하면 될 수 있음
#[derive(Debug)]
pub struct NetworkError(pub String);

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "네트워크 오류: {}", self.0)
    }
}
impl std::error::Error for NetworkError {}

/// 연결 문제로 실패했는지 (다시 시도할 만한 오류)
pub fn is_network(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<NetworkError>().is_some()
            || c.downcast_ref::<std::io::Error>().is_some_and(|io| {
                use std::io::ErrorKind::*;
                matches!(
                    io.kind(),
                    TimedOut | ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof
                )
            })
    })
}

/// API 호출(목록, 로그인 등) 한 번에 허용하는 시간. 파일 전송은 에이전트 기본값을 씀
pub const API_TIMEOUT: Duration = Duration::from_secs(60);

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
        Err(e) => Err(NetworkError(e.to_string()).into()),
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
