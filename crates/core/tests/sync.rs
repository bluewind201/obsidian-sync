//! 로컬 폴더 백엔드를 원격 삼아 두 기기(A, B) 간 동기화를 검증
use obsidian_sync_core::{
    AppConfig, BackendConfig, ConflictPolicy, Profile, ProfileKind, SyncEngine, SyncMode,
};
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

fn profile(local: &Path, remote: &Path) -> Profile {
    Profile {
        local_path: local.to_string_lossy().into(),
        backend: BackendConfig::LocalFolder {
            path: remote.to_string_lossy().into(),
        },
        ..Default::default()
    }
}

fn engine(vault: &Path, remote: &Path, data: &Path, device: &str) -> SyncEngine {
    SyncEngine::new(profile(vault, remote), device, data).unwrap()
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

#[test]
fn backup_mode_never_deletes_remote() {
    let e = env();
    let mut p = profile(&e.a, &e.remote);
    p.kind = ProfileKind::Folder;
    p.mode = SyncMode::UploadOnly;
    let mut ea = SyncEngine::new(p.clone(), "Phone", &e.data_a).unwrap();

    // 이미 원격에 같은 크기로 올라가 있는 사진은 다시 올리지 않음
    write(&e.a.join("DCIM/old.jpg"), "old-photo");
    write(&e.remote.join("DCIM/old.jpg"), "old-photo");
    // 원격에만 있는 파일은 내려받지 않음
    write(&e.remote.join("other.jpg"), "pc-only");
    write(&e.a.join("DCIM/new.jpg"), "new-photo");
    let r = ea.sync_once(&noop).unwrap();
    assert_eq!(r.uploaded, 1, "{r:?}");
    assert_eq!(r.downloaded, 0);
    assert!(!e.a.join("other.jpg").exists());
    assert!(e.remote.join("DCIM/new.jpg").exists());

    // 휴대폰에서 지워도 원격에는 남음
    fs::remove_file(e.a.join("DCIM/new.jpg")).unwrap();
    let r = ea.sync_once(&noop).unwrap();
    assert_eq!(r.deleted_remote, 0);
    assert!(e.remote.join("DCIM/new.jpg").exists());
    assert_eq!(ea.sync_once(&noop).unwrap().changes(), 0);

    // 휴대폰에서 바꾸면 다시 올림
    std::thread::sleep(std::time::Duration::from_millis(20));
    write(&e.a.join("DCIM/old.jpg"), "edited-photo");
    assert_eq!(ea.sync_once(&noop).unwrap().uploaded, 1);
    assert_eq!(
        fs::read_to_string(e.remote.join("DCIM/old.jpg")).unwrap(),
        "edited-photo"
    );
}

#[test]
fn multiple_profiles_sync_independently() {
    let e = env();
    let notes = e.a.join("notes");
    let photos = e.a.join("photos");
    let r_notes = e.remote.join("notes");
    let r_photos = e.remote.join("photos");
    for d in [&notes, &photos] {
        fs::create_dir_all(d).unwrap();
    }
    write(&notes.join("a.md"), "note");
    write(&photos.join("p.jpg"), "photo");
    let mut photo_profile = profile(&photos, &r_photos);
    photo_profile.kind = ProfileKind::Folder;
    photo_profile.name = "사진".into();
    let cfg = AppConfig {
        profiles: vec![profile(&notes, &r_notes), photo_profile],
        ..Default::default()
    };
    cfg.validate().unwrap();
    let rep = obsidian_sync_core::service::sync_once(cfg.clone(), &e.data_a, &noop).unwrap();
    assert_eq!(rep.uploaded, 2, "{rep:?}");
    assert!(r_notes.join("a.md").exists());
    assert!(r_photos.join("p.jpg").exists());

    // 폴더가 겹치는 프로필은 거부
    let mut bad = cfg.clone();
    let mut overlap = profile(&photos.join("sub"), &e.remote.join("x"));
    overlap.id = "other".into();
    bad.profiles.push(overlap);
    assert!(bad.validate().is_err());
}

#[test]
fn large_file_streams() {
    let e = env();
    let mut ea = engine(&e.a, &e.remote, &e.data_a, "PC");
    let mut eb = engine(&e.b, &e.remote, &e.data_b, "Phone");
    let big: Vec<u8> = (0..5_000_000u32).map(|i| (i % 253) as u8).collect();
    fs::create_dir_all(e.a.join("video")).unwrap();
    fs::write(e.a.join("video/clip.mp4"), &big).unwrap();
    assert_eq!(ea.sync_once(&noop).unwrap().uploaded, 1);
    assert_eq!(eb.sync_once(&noop).unwrap().downloaded, 1);
    assert_eq!(fs::read(e.b.join("video/clip.mp4")).unwrap(), big);
    // 임시파일이 남지 않음
    let leftovers: Vec<_> = fs::read_dir(e.b.join("video"))
        .unwrap()
        .filter_map(|d| d.ok())
        .filter(|d| d.file_name().to_string_lossy().starts_with(".obsync-tmp-"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn upload_only_can_mirror_deletes() {
    let e = env();
    let mut p = profile(&e.a, &e.remote);
    p.mode = SyncMode::UploadOnly;
    p.delete_in_target = true;
    let mut ea = SyncEngine::new(p, "PC", &e.data_a).unwrap();
    write(&e.a.join("x.jpg"), "x");
    ea.sync_once(&noop).unwrap();
    fs::remove_file(e.a.join("x.jpg")).unwrap();
    assert_eq!(ea.sync_once(&noop).unwrap().deleted_remote, 1);
    assert!(!e.remote.join("x.jpg").exists());
}

#[test]
fn download_only_mode() {
    let e = env();
    let mut p = profile(&e.b, &e.remote);
    p.mode = SyncMode::DownloadOnly;
    let mut eb = SyncEngine::new(p.clone(), "Phone", &e.data_b).unwrap();

    write(&e.remote.join("doc/a.md"), "from nas");
    write(&e.remote.join("same.md"), "same");
    write(&e.b.join("same.md"), "same");
    write(&e.b.join("diff.md"), "local version!!");
    write(&e.remote.join("diff.md"), "nas version");
    write(&e.b.join("local-only.md"), "mine");
    let r = eb.sync_once(&noop).unwrap();
    assert_eq!(r.uploaded, 0, "{r:?}");
    assert_eq!(
        fs::read_to_string(e.b.join("doc/a.md")).unwrap(),
        "from nas"
    );
    // 처음 연결 때 내용이 다르면 저장소 기준, 내 기기 파일은 .trash 보관
    assert_eq!(
        fs::read_to_string(e.b.join("diff.md")).unwrap(),
        "nas version"
    );
    assert_eq!(
        fs::read_to_string(e.b.join(".trash/diff.md")).unwrap(),
        "local version!!"
    );
    // 내 기기에만 있는 파일은 올리지 않음
    assert!(!e.remote.join("local-only.md").exists());
    assert_eq!(eb.sync_once(&noop).unwrap().changes(), 0);

    // 내 기기에서 고쳐도 올리지 않음
    std::thread::sleep(std::time::Duration::from_millis(20));
    write(&e.b.join("doc/a.md"), "edited on phone");
    assert_eq!(eb.sync_once(&noop).unwrap().uploaded, 0);
    assert_eq!(
        fs::read_to_string(e.remote.join("doc/a.md")).unwrap(),
        "from nas"
    );

    // 원격에서 지워도 기본값은 내 기기에 남김
    fs::remove_file(e.remote.join("same.md")).unwrap();
    eb.sync_once(&noop).unwrap();
    assert!(e.b.join("same.md").exists());

    // 옵션을 켜면 원격 삭제를 따라감 (.trash로 이동)
    p.delete_in_target = true;
    let mut eb = SyncEngine::new(p, "Phone", &e.data_b).unwrap();
    write(&e.remote.join("gone.md"), "g");
    eb.sync_once(&noop).unwrap();
    fs::remove_file(e.remote.join("gone.md")).unwrap();
    assert_eq!(eb.sync_once(&noop).unwrap().deleted_local, 1);
    assert!(e.b.join(".trash/gone.md").exists());
}

#[test]
fn conflict_policies() {
    for policy in [ConflictPolicy::PreferLocal, ConflictPolicy::PreferRemote] {
        let e = env();
        let mut pa = profile(&e.a, &e.remote);
        pa.conflict_policy = policy;
        let mut pb = profile(&e.b, &e.remote);
        pb.conflict_policy = policy;
        let mut ea = SyncEngine::new(pa, "PC", &e.data_a).unwrap();
        let mut eb = SyncEngine::new(pb, "Phone", &e.data_b).unwrap();
        write(&e.a.join("x.md"), "base");
        ea.sync_once(&noop).unwrap();
        eb.sync_once(&noop).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&e.a.join("x.md"), "A edit");
        write(&e.b.join("x.md"), "B edit");
        ea.sync_once(&noop).unwrap();
        let r = eb.sync_once(&noop).unwrap();
        assert_eq!(r.conflicts, 0, "{policy:?} {r:?}");
        let expected = match policy {
            ConflictPolicy::PreferLocal => "B edit",
            _ => "A edit",
        };
        assert_eq!(fs::read_to_string(e.b.join("x.md")).unwrap(), expected);
        assert_eq!(fs::read_to_string(e.remote.join("x.md")).unwrap(), expected);
        if policy == ConflictPolicy::PreferRemote {
            assert_eq!(
                fs::read_to_string(e.b.join(".trash/x.md")).unwrap(),
                "B edit"
            );
        }
        // 충돌 사본은 만들지 않음
        assert!(!fs::read_dir(&e.b)
            .unwrap()
            .filter_map(|d| d.ok())
            .any(|d| d.file_name().to_string_lossy().contains("충돌")));
    }
}
