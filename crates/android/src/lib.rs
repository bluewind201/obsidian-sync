//! Android(Java)에서 호출하는 JNI 함수들.
//! 모든 함수는 JSON 문자열을 주고받으며, 패닉이 JVM으로 전파되지 않도록 막습니다.
//! Java 클래스: `io.bluewind.obsidiansync.Native` (static native 메서드)

use anyhow::{anyhow, Result};
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jstring};
use jni::JNIEnv;
use obsidian_sync_core::auth::{self, DeviceCode, OAuthProvider};
use obsidian_sync_core::backend::synology;
use obsidian_sync_core::{service, AppConfig, BackendConfig, SyncService};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static SERVICE: Mutex<Option<SyncService>> = Mutex::new(None);
static PENDING: Mutex<Option<DeviceCode>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);

fn parse(config: &str) -> Result<AppConfig> {
    AppConfig::from_json(config).map_err(|e| anyhow!("설정 형식 오류: {e:#}"))
}

/// 저장소 설정 JSON (프로필의 `backend` 항목)
fn parse_backend(backend: &str) -> Result<BackendConfig> {
    serde_json::from_str(backend).map_err(|e| anyhow!("저장소 설정 형식 오류: {e}"))
}

fn err(e: anyhow::Error) -> Value {
    json!({"ok": false, "error": format!("{e:#}"), "need_otp": synology::needs_otp(&e)})
}

// ── 순수 Rust 함수 (테스트 가능) ──────────────────────────────

pub fn start(config: &str, data_dir: &str) -> Value {
    let r = (|| {
        let cfg = parse(config)?;
        // 이전 서비스는 잠금을 놓은 뒤에 멈춰야 상태 조회(화면)가 멈추지 않습니다
        let old = SERVICE.lock().unwrap().take();
        if let Some(old) = old {
            old.stop();
        }
        let new = SyncService::start(cfg, Path::new(data_dir))?;
        if let Some(raced) = SERVICE.lock().unwrap().replace(new) {
            raced.stop();
        }
        Ok::<_, anyhow::Error>(())
    })();
    match r {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(e),
    }
}

pub fn stop() {
    // 잠금을 잡은 채로 멈추면 그동안 status()/sync_now()가 모두 대기합니다
    let s = SERVICE.lock().unwrap().take();
    if let Some(s) = s {
        s.stop();
    }
}

/// 대량 삭제 확인 대기 중인 프로필의 삭제를 한 번 허용 (빈 id = 대기 중인 전체)
pub fn allow_deletes(profile_id: &str) -> bool {
    match SERVICE.lock().unwrap().as_ref() {
        Some(s) => {
            s.allow_deletes(profile_id);
            true
        }
        None => false,
    }
}

pub fn sync_now() -> bool {
    match SERVICE.lock().unwrap().as_ref() {
        Some(s) => {
            s.sync_now();
            true
        }
        None => false,
    }
}

pub fn status() -> Value {
    match SERVICE.lock().unwrap().as_ref() {
        Some(s) => serde_json::to_value(s.status()).unwrap_or(Value::Null),
        None => json!({"running": false, "syncing": false, "logs": []}),
    }
}

pub fn sync_once(config: &str, data_dir: &str) -> Value {
    // 서비스가 실행 중이면 중복 실행하지 않고 서비스에 요청
    if sync_now() {
        return json!({"ok": true, "delegated": true});
    }
    match parse(config).and_then(|c| service::sync_once(c, Path::new(data_dir), &|_| {})) {
        Ok(rep) => json!({"ok": true, "report": rep, "summary": rep.summary()}),
        Err(e) => err(e),
    }
}

/// 연결 테스트 (`backend`: 저장소 설정 JSON)
pub fn test_connection(backend: &str, data_dir: &str) -> Value {
    match parse_backend(backend).and_then(|b| service::test_connection(&b, Path::new(data_dir))) {
        Ok(n) => json!({"ok": true, "count": n}),
        Err(e) => err(e),
    }
}

/// Synology 로그인 (2단계 인증 코드가 있으면 함께 보내고 기기를 기억)
pub fn synology_login(backend: &str, data_dir: &str, otp: &str) -> Value {
    let otp = otp.trim();
    match parse_backend(backend)
        .and_then(|b| synology::login(&b, Path::new(data_dir), (!otp.is_empty()).then_some(otp)))
    {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(e),
    }
}

/// NAS 폴더 목록 (`path`가 비면 공유 폴더). {"ok","folders":[{name,path}]}
pub fn synology_folders(backend: &str, data_dir: &str, path: &str) -> Value {
    match parse_backend(backend).and_then(|b| synology::list_folders(&b, Path::new(data_dir), path))
    {
        Ok(f) => json!({"ok": true, "folders": f}),
        Err(e) => err(e),
    }
}

/// NAS에 새 폴더 만들기. {"ok","path"}
pub fn synology_create_folder(backend: &str, data_dir: &str, parent: &str, name: &str) -> Value {
    match parse_backend(backend)
        .and_then(|b| synology::create_folder(&b, Path::new(data_dir), parent, name))
    {
        Ok(p) => json!({"ok": true, "path": p}),
        Err(e) => err(e),
    }
}

/// Synology 2단계 인증을 통과해 이 기기가 기억되어 있는지
pub fn synology_remembered(backend: &str, data_dir: &str) -> bool {
    parse_backend(backend)
        .map(|b| synology::has_device_token(Path::new(data_dir), &b))
        .unwrap_or(false)
}

/// 저장된 Synology 기기 인증을 지움
pub fn synology_forget(backend: &str, data_dir: &str) {
    if let Ok(b) = parse_backend(backend) {
        synology::forget_device(Path::new(data_dir), &b);
    }
}

pub fn login_start(backend: &str) -> Value {
    let r = (|| {
        let backend = parse_backend(backend)?;
        backend.validate()?;
        let p = OAuthProvider::for_backend(&backend)
            .ok_or_else(|| anyhow!("로그인이 필요 없는 저장소입니다"))?;
        let dc = p.start_device()?;
        CANCEL.store(false, Ordering::Relaxed);
        *PENDING.lock().unwrap() = Some(dc.clone());
        Ok::<_, anyhow::Error>(dc)
    })();
    match r {
        Ok(dc) => {
            json!({"ok": true, "user_code": dc.user_code, "verification_uri": dc.verification_uri})
        }
        Err(e) => err(e),
    }
}

/// 사용자가 승인할 때까지 대기 (블로킹, 백그라운드 스레드에서 호출)
pub fn login_wait(backend: &str, data_dir: &str) -> Value {
    let r = (|| {
        let backend = parse_backend(backend)?;
        let dc = PENDING
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("먼저 로그인을 시작하세요"))?;
        let p = OAuthProvider::for_backend(&backend).ok_or_else(|| anyhow!("OAuth 저장소 아님"))?;
        let token = p.poll_device(&dc, &CANCEL)?;
        let path: PathBuf = auth::token_path(Path::new(data_dir), &backend).unwrap();
        auth::save_token(&path, &token)?;
        *PENDING.lock().unwrap() = None;
        Ok::<_, anyhow::Error>(())
    })();
    match r {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(e),
    }
}

pub fn login_cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// OneDrive/Google Drive 로그인 여부 (그 외 저장소는 항상 true)
pub fn is_logged_in(backend: &str, data_dir: &str) -> bool {
    parse_backend(backend)
        .map(|b| !b.needs_oauth() || auth::is_logged_in(Path::new(data_dir), &b))
        .unwrap_or(false)
}

pub fn logout(backend: &str, data_dir: &str) {
    if let Ok(b) = parse_backend(backend) {
        auth::logout(Path::new(data_dir), &b);
    }
}

/// 프로필 하나의 동기화 기록 초기화
pub fn reset_state(config: &str, data_dir: &str, profile_id: &str) -> Value {
    match parse(config).and_then(|c| service::reset_state(&c, profile_id, Path::new(data_dir))) {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(e),
    }
}

pub fn validate(config: &str) -> Value {
    match parse(config).and_then(|c| c.validate()) {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(e),
    }
}

/// 설정을 현재 형식으로 정리해 돌려줌 (이전 버전 설정 자동 변환, 새 프로필 ID 부여)
pub fn normalize(config: &str) -> Value {
    let cfg = if config.trim().is_empty() {
        Ok(AppConfig::default())
    } else {
        parse(config)
    };
    match cfg {
        Ok(c) => json!({"ok": true, "config": c}),
        Err(e) => err(e),
    }
}

/// 새 프로필 ID
pub fn new_profile_id() -> String {
    obsidian_sync_core::config::new_profile_id()
}

// ── JNI 바인딩 ──────────────────────────────────────────────

fn s(env: &mut JNIEnv, js: &JString) -> String {
    env.get_string(js).map(|x| x.into()).unwrap_or_default()
}

fn out(env: &mut JNIEnv, v: String) -> jstring {
    env.new_string(v)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
}

const PANIC: &str = r#"{"ok":false,"error":"내부 오류가 발생했습니다"}"#;

macro_rules! jni_json2 {
    ($name:ident, $f:path) => {
        #[no_mangle]
        pub extern "system" fn $name<'l>(
            mut env: JNIEnv<'l>,
            _c: JClass<'l>,
            a: JString<'l>,
            b: JString<'l>,
        ) -> jstring {
            let (a, b) = (s(&mut env, &a), s(&mut env, &b));
            let v = guard(PANIC.to_string(), || $f(&a, &b).to_string());
            out(&mut env, v)
        }
    };
}

jni_json2!(Java_io_bluewind_obsidiansync_Native_start, start);
jni_json2!(Java_io_bluewind_obsidiansync_Native_syncOnce, sync_once);
jni_json2!(
    Java_io_bluewind_obsidiansync_Native_testConnection,
    test_connection
);
jni_json2!(Java_io_bluewind_obsidiansync_Native_loginWait, login_wait);
macro_rules! jni_json3 {
    ($name:ident, $f:path) => {
        #[no_mangle]
        pub extern "system" fn $name<'l>(
            mut env: JNIEnv<'l>,
            _c: JClass<'l>,
            a: JString<'l>,
            b: JString<'l>,
            c: JString<'l>,
        ) -> jstring {
            let (a, b, c) = (s(&mut env, &a), s(&mut env, &b), s(&mut env, &c));
            let v = guard(PANIC.to_string(), || $f(&a, &b, &c).to_string());
            out(&mut env, v)
        }
    };
}

jni_json3!(Java_io_bluewind_obsidiansync_Native_resetState, reset_state);
jni_json3!(
    Java_io_bluewind_obsidiansync_Native_synologyFolders,
    synology_folders
);

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_synologyCreateFolder<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
    b: JString<'l>,
    c: JString<'l>,
    d: JString<'l>,
) -> jstring {
    let (a, b) = (s(&mut env, &a), s(&mut env, &b));
    let (c, d) = (s(&mut env, &c), s(&mut env, &d));
    let v = guard(PANIC.to_string(), || {
        synology_create_folder(&a, &b, &c, &d).to_string()
    });
    out(&mut env, v)
}
jni_json3!(
    Java_io_bluewind_obsidiansync_Native_synologyLogin,
    synology_login
);

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_synologyForget<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
    b: JString<'l>,
) {
    let (a, b) = (s(&mut env, &a), s(&mut env, &b));
    guard((), || synology_forget(&a, &b));
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_synologyRemembered<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
    b: JString<'l>,
) -> jboolean {
    let (a, b) = (s(&mut env, &a), s(&mut env, &b));
    guard(false, || synology_remembered(&a, &b)) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_normalize<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
) -> jstring {
    let a = s(&mut env, &a);
    let v = guard(PANIC.to_string(), || normalize(&a).to_string());
    out(&mut env, v)
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_newProfileId<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jstring {
    let v = guard("p0".to_string(), new_profile_id);
    out(&mut env, v)
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_loginStart<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
) -> jstring {
    let a = s(&mut env, &a);
    let v = guard(PANIC.to_string(), || login_start(&a).to_string());
    out(&mut env, v)
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_validate<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
) -> jstring {
    let a = s(&mut env, &a);
    let v = guard(PANIC.to_string(), || validate(&a).to_string());
    out(&mut env, v)
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_status<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jstring {
    let v = guard("{}".to_string(), || status().to_string());
    out(&mut env, v)
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_stop<'l>(
    _env: JNIEnv<'l>,
    _c: JClass<'l>,
) {
    guard((), stop);
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_allowDeletes<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    profile_id: JString<'l>,
) -> jboolean {
    let id = s(&mut env, &profile_id);
    guard(false, || allow_deletes(&id)) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_syncNow<'l>(
    _env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jboolean {
    guard(false, sync_now) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_loginCancel<'l>(
    _env: JNIEnv<'l>,
    _c: JClass<'l>,
) {
    guard((), login_cancel);
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_isLoggedIn<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
    b: JString<'l>,
) -> jboolean {
    let (a, b) = (s(&mut env, &a), s(&mut env, &b));
    guard(false, || is_logged_in(&a, &b)) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_bluewind_obsidiansync_Native_logout<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    a: JString<'l>,
    b: JString<'l>,
) {
    let (a, b) = (s(&mut env, &a), s(&mut env, &b));
    guard((), || logout(&a, &b));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_config_is_error_not_panic() {
        assert_eq!(sync_once("not json", "/tmp")["ok"], false);
        assert_eq!(validate("{}")["ok"], false);
        assert_eq!(status()["running"], false);
        assert_eq!(test_connection("{}", "/tmp")["ok"], false);
    }

    #[test]
    fn normalize_migrates_legacy() {
        let v = normalize(
            r#"{"vault_path":"/sdcard/Vault","backend":{"type":"local_folder","path":"/x"}}"#,
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["config"]["profiles"][0]["local_path"], "/sdcard/Vault");
        assert_eq!(
            normalize("")["config"]["profiles"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
