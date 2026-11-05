//! Android(Java)에서 호출하는 JNI 함수들.
//! 모든 함수는 JSON 문자열을 주고받으며, 패닉이 JVM으로 전파되지 않도록 막습니다.
//! Java 클래스: `io.bluewind.obsidiansync.Native` (static native 메서드)

use anyhow::{anyhow, Result};
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jstring};
use jni::JNIEnv;
use obsidian_sync_core::auth::{self, DeviceCode, OAuthProvider};
use obsidian_sync_core::{service, AppConfig, SyncService};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static SERVICE: Mutex<Option<SyncService>> = Mutex::new(None);
static PENDING: Mutex<Option<DeviceCode>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);

fn parse(config: &str) -> Result<AppConfig> {
    serde_json::from_str(config).map_err(|e| anyhow!("설정 형식 오류: {e}"))
}

fn err(e: anyhow::Error) -> Value {
    json!({"ok": false, "error": format!("{e:#}")})
}

// ── 순수 Rust 함수 (테스트 가능) ──────────────────────────────

pub fn start(config: &str, data_dir: &str) -> Value {
    let r = (|| {
        let cfg = parse(config)?;
        let mut guard = SERVICE.lock().unwrap();
        if let Some(old) = guard.take() {
            old.stop();
        }
        *guard = Some(SyncService::start(cfg, Path::new(data_dir))?);
        Ok::<_, anyhow::Error>(())
    })();
    match r {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(e),
    }
}

pub fn stop() {
    if let Some(s) = SERVICE.lock().unwrap().take() {
        s.stop();
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

pub fn test_connection(config: &str, data_dir: &str) -> Value {
    match parse(config).and_then(|c| service::test_connection(&c.backend, Path::new(data_dir))) {
        Ok(n) => json!({"ok": true, "count": n}),
        Err(e) => err(e),
    }
}

pub fn login_start(config: &str) -> Value {
    let r = (|| {
        let cfg = parse(config)?;
        cfg.backend.validate()?;
        let p = OAuthProvider::for_backend(&cfg.backend)
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
pub fn login_wait(config: &str, data_dir: &str) -> Value {
    let r = (|| {
        let cfg = parse(config)?;
        let dc = PENDING
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("먼저 로그인을 시작하세요"))?;
        let p =
            OAuthProvider::for_backend(&cfg.backend).ok_or_else(|| anyhow!("OAuth 저장소 아님"))?;
        let token = p.poll_device(&dc, &CANCEL)?;
        let path: PathBuf = auth::token_path(Path::new(data_dir), &cfg.backend).unwrap();
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

pub fn is_logged_in(config: &str, data_dir: &str) -> bool {
    parse(config)
        .map(|c| !c.backend.needs_oauth() || auth::is_logged_in(Path::new(data_dir), &c.backend))
        .unwrap_or(false)
}

pub fn logout(config: &str, data_dir: &str) {
    if let Ok(c) = parse(config) {
        auth::logout(Path::new(data_dir), &c.backend);
    }
}

pub fn reset_state(config: &str, data_dir: &str) -> Value {
    match parse(config).and_then(|c| service::reset_state(c, Path::new(data_dir))) {
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
jni_json2!(Java_io_bluewind_obsidiansync_Native_resetState, reset_state);

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
    }
}
