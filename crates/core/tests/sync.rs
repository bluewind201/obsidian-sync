//! 로컬 폴더 백엔드를 원격 삼아 두 기기(A, B) 간 동기화를 검증
use obsidian_sync_core::{AppConfig, BackendConfig, SyncEngine};
use std::fs;
use std::path::Path;

struct Env {
    _tmp: tempfile::TempDir,
    a: std::path::PathBuf,
    b: std::path::PathBuf,
    remote: std::path::PathBuf,
    data_a: std::path::PathBuf,
    data_b: std::path::PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let r = tmp.path();
    let e = Env {
        a: r.join("a"),
        b: r.join("b"),
        remote: r.join("remote"),
        data_a: r.join("data_a"),
        data_b: r.join("data_b"),
        _tmp: tmp,
    };
    for d in [&e.a, &e.b, &e.remote] {
        fs::create_dir_all(d).unwrap();
    }
    e
}

fn engine(vault: &Path, remote: &Path, data: &Path, device: &str) -> SyncEngine {
    let cfg = AppConfig {
        vault_path: vault.to_string_lossy().into(),
        backend: BackendConfig::LocalFolder {
            path: remote.to_string_lossy().into(),
        },
        device_name: device.into(),
        ..Default::default()
    };
    SyncEngine::new(cfg, data).unwrap()
}

fn write(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

fn noop(_: &str) {}

#[test]
fn two_way_sync() {
    let e = env();
    let mut ea = engine(&e.a, &e.remote, &e.data_a, "PC");
    let mut eb = engine(&e.b, &e.remote, &e.data_b, "Phone");

    write(&e.a.join("노트/첫 노트.md"), "hello");
    write(&e.a.join(".obsidian/app.json"), "{}");
    write(&e.a.join(".obsidian/workspace.json"), "ignored");
    let r = ea.sync_once(&noop).unwrap();
    assert_eq!(r.uploaded, 2, "{r:?}");
    assert!(!e.remote.join(".obsidian/workspace.json").exists());

    let r = eb.sync_once(&noop).unwrap();
    assert_eq!(r.downloaded, 2);
    assert_eq!(
        fs::read_to_string(e.b.join("노트/첫 노트.md")).unwrap(),
        "hello"
    );

    // 변경 없음
    assert_eq!(ea.sync_once(&noop).unwrap().changes(), 0);
    assert_eq!(eb.sync_once(&noop).unwrap().changes(), 0);

    // B에서 수정 → A로 전파
    std::thread::sleep(std::time::Duration::from_millis(20));
    write(&e.b.join("노트/첫 노트.md"), "hello from B");
    assert_eq!(eb.sync_once(&noop).unwrap().uploaded, 1);
    assert_eq!(ea.sync_once(&noop).unwrap().downloaded, 1);
    assert_eq!(
        fs::read_to_string(e.a.join("노트/첫 노트.md")).unwrap(),
        "hello from B"
    );

    // A에서 삭제 → 원격 삭제 → B는 .trash로 이동
    fs::remove_file(e.a.join("노트/첫 노트.md")).unwrap();
    assert_eq!(ea.sync_once(&noop).unwrap().deleted_remote, 1);
    assert!(!e.remote.join("노트/첫 노트.md").exists());
    assert_eq!(eb.sync_once(&noop).unwrap().deleted_local, 1);
    assert!(!e.b.join("노트/첫 노트.md").exists());
    assert!(e.b.join(".trash/노트/첫 노트.md").exists());
}

#[test]
fn conflict_keeps_both() {
    let e = env();
    let mut ea = engine(&e.a, &e.remote, &e.data_a, "PC");
    let mut eb = engine(&e.b, &e.remote, &e.data_b, "Phone");
    write(&e.a.join("x.md"), "base");
    ea.sync_once(&noop).unwrap();
    eb.sync_once(&noop).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    write(&e.a.join("x.md"), "A edit");
    write(&e.b.join("x.md"), "B edit");
    ea.sync_once(&noop).unwrap();
    let r = eb.sync_once(&noop).unwrap();
    assert_eq!(r.conflicts, 1, "{r:?}");
    // 원격(A) 버전이 원래 이름, B 버전은 충돌 사본
    assert_eq!(fs::read_to_string(e.b.join("x.md")).unwrap(), "A edit");
    let copies: Vec<_> = fs::read_dir(&e.b)
        .unwrap()
        .filter_map(|d| d.ok())
        .map(|d| d.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("충돌"))
        .collect();
    assert_eq!(copies.len(), 1);
    assert_eq!(fs::read_to_string(e.b.join(&copies[0])).unwrap(), "B edit");
    // 충돌 사본도 A로 전달
    assert_eq!(ea.sync_once(&noop).unwrap().downloaded, 1);
    assert!(e.a.join(&copies[0]).exists());
}

#[test]
fn identical_files_first_sync_no_conflict() {
    let e = env();
    write(&e.a.join("same.md"), "same");
    write(&e.remote.join("same.md"), "same");
    let mut ea = engine(&e.a, &e.remote, &e.data_a, "PC");
    let r = ea.sync_once(&noop).unwrap();
    assert_eq!(r.conflicts, 0);
    assert_eq!(r.changes(), 0);
}

#[test]
fn empty_remote_safety() {
    let e = env();
    let mut ea = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 0..6 {
        write(&e.a.join(format!("{i}.md")), "x");
    }
    ea.sync_once(&noop).unwrap();
    // 원격이 통째로 사라짐 → 로컬을 지우지 않고 중단
    fs::remove_dir_all(&e.remote).unwrap();
    fs::create_dir_all(&e.remote).unwrap();
    assert!(ea.sync_once(&noop).is_err());
    assert!(e.a.join("0.md").exists());
    // 기록 초기화 후에는 다시 업로드
    ea.reset_state().unwrap();
    assert_eq!(ea.sync_once(&noop).unwrap().uploaded, 6);
}
