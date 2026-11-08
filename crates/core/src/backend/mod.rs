//! 원격 저장소 플러그인 구조.
//!
//! 새 저장소를 추가하려면 [`Backend`] 트레이트를 구현하고
//! [`BackendConfig`]에 항목을 추가한 뒤 [`create_backend`]에 연결하면 됩니다.

pub mod gdrive;
pub mod local;
pub mod onedrive;
pub mod s3;
pub mod synology;

use crate::config::BackendConfig;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

/// 원격 파일 정보. `path`는 볼트 기준 상대경로이며 구분자는 항상 `/` 입니다.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RemoteEntry {
    pub path: String,
    pub size: u64,
    /// 내용이 바뀌면 함께 바뀌는 값 (ETag, md5, cTag 등)
    pub rev: String,
}

pub trait Backend: Send {
    /// 저장소 고유 식별자 (동기화 상태 파일 구분용)
    fn id(&self) -> String;
    /// 모든 파일 목록 (폴더 제외)
    fn list(&mut self) -> Result<Vec<RemoteEntry>>;
    fn download(&mut self, path: &str) -> Result<Vec<u8>>;
    /// 업로드 후 새 리비전 정보 반환 (`list`와 같은 형식의 rev 여야 함)
    fn upload(&mut self, path: &str, data: &[u8]) -> Result<RemoteEntry>;
    /// 삭제. 이미 없으면 성공으로 처리
    fn delete(&mut self, path: &str) -> Result<()>;

    /// 여러 파일 한꺼번에 삭제. 한 번의 요청으로 지울 수 있는 저장소는 재정의합니다.
    /// 실패하면 엔진이 파일별 `delete`로 다시 시도합니다.
    fn delete_many(&mut self, paths: &[String]) -> Result<()> {
        for p in paths {
            self.delete(p)?;
        }
        Ok(())
    }

    /// 로컬 파일을 업로드. 대용량 파일을 메모리에 올리지 않도록 저장소별로 재정의할 수 있음
    fn upload_file(&mut self, path: &str, src: &Path) -> Result<RemoteEntry> {
        let data = std::fs::read(src)?;
        self.upload(path, &data)
    }

    /// 원격 파일을 `dest`로 내려받음. 대용량 파일은 저장소별로 스트리밍 구현
    fn download_to(&mut self, path: &str, dest: &mut dyn Write) -> Result<()> {
        let data = self.download(path)?;
        dest.write_all(&data)?;
        Ok(())
    }

    /// 연결 확인용 추가 검사 (예: 공유 폴더 존재 여부)
    fn check(&mut self) -> Result<()> {
        Ok(())
    }

    /// 변경 신호 파일([`CHANGE_MARKER`])의 현재 값. 이 앱이 저장소를 바꿀 때마다 새 값을 씁니다.
    /// 다른 기기는 이 작은 파일 하나만 자주 확인해 변경을 빨리 알아챕니다.
    /// `None`이면 지원하지 않는 저장소 (전체 목록 주기 확인만 사용). 파일이 없으면 `Some("")`.
    fn change_marker(&mut self) -> Result<Option<String>> {
        Ok(None)
    }

    /// 변경 신호 파일에 새 값을 씀 (지원하지 않는 저장소는 아무것도 하지 않음)
    fn touch_change_marker(&mut self, _token: &str) -> Result<()> {
        Ok(())
    }
}

/// 변경 신호 파일 경로 (동기화 대상에서 제외됨)
pub const CHANGE_MARKER: &str = ".obsync/changed";

pub fn create_backend(cfg: &BackendConfig, data_dir: &Path) -> Result<Box<dyn Backend>> {
    cfg.validate()?;
    Ok(match cfg {
        BackendConfig::Synology { .. } => Box::new(synology::SynologyBackend::new(cfg, data_dir)?),
        BackendConfig::LocalFolder { path } => Box::new(local::LocalBackend::new(path)?),
        BackendConfig::S3 {
            endpoint,
            region,
            bucket,
            prefix,
            access_key,
            secret_key,
        } => Box::new(s3::S3Backend::new(
            endpoint, region, bucket, prefix, access_key, secret_key,
        )),
        BackendConfig::OneDrive { folder, .. } => {
            Box::new(onedrive::OneDriveBackend::new(cfg, folder, data_dir)?)
        }
        BackendConfig::GoogleDrive { folder, .. } => {
            Box::new(gdrive::GDriveBackend::new(cfg, folder, data_dir)?)
        }
    })
}

/// 원격에서 받은 상대경로가 안전한지 검사 (상위 폴더 탈출 방지)
pub fn is_safe_rel_path(p: &str) -> bool {
    if p.is_empty() || p.starts_with('/') || p.contains('\\') || p.contains('\0') {
        return false;
    }
    !p.split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == ".." || seg.contains(':'))
}

pub fn check_path(p: &str) -> Result<()> {
    if !is_safe_rel_path(p) {
        bail!("허용되지 않는 경로: {p}");
    }
    Ok(())
}

/// 경로를 `/`로 나눠 각 부분을 퍼센트 인코딩
pub fn encode_path(p: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const SET: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    p.split('/')
        .map(|s| utf8_percent_encode(s, SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_paths() {
        assert!(is_safe_rel_path("a/b.md"));
        assert!(is_safe_rel_path("한글 노트.md"));
        assert!(!is_safe_rel_path("../x"));
        assert!(!is_safe_rel_path("/x"));
        assert!(!is_safe_rel_path("a//b"));
        assert!(!is_safe_rel_path("C:/x"));
        assert_eq!(encode_path("a b/c+d.md"), "a%20b/c%2Bd.md");
    }
}
