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

#[test]
fn mass_delete_waits_for_confirmation() {
    use obsidian_sync_core::MassDeletePending;
    let e = env();
    let mut ea = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 0..30 {
        write(&e.a.join(format!("n/{i}.md")), "x");
    }
    assert_eq!(ea.sync_once(&noop).unwrap().uploaded, 30);
    for i in 0..25 {
        fs::remove_file(e.a.join(format!("n/{i}.md"))).unwrap();
    }
    // 확인 전: 멈추고 원격은 그대로
    let err = ea.sync_once(&noop).unwrap_err();
    assert_eq!(
        err.downcast_ref::<MassDeletePending>(),
        Some(&MassDeletePending { count: 25 })
    );
    assert!(e.remote.join("n/0.md").exists());
    // 확인 후: 한 번 진행
    ea.allow_deletions_once();
    let r = ea.sync_once(&noop).unwrap();
    assert_eq!(r.deleted_remote, 25, "{r:?}");
    assert!(!e.remote.join("n/0.md").exists());
    assert!(e.remote.join("n/29.md").exists());
}

#[test]
fn cancel_stops_sync_and_resumes_later() {
    use std::sync::atomic::Ordering;
    let e = env();
    let mut ea = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 0..40 {
        write(&e.a.join(format!("{i}.md")), "x");
    }
    let cancel = ea.cancel_flag();
    cancel.store(true, Ordering::Relaxed);
    let r = ea.sync_once(&noop).unwrap();
    assert_eq!(r.uploaded, 0);
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(ea.sync_once(&noop).unwrap().uploaded, 40);
}

#[test]
fn remote_deletes_are_batched() {
    use obsidian_sync_core::backend::{self, Backend, RemoteEntry};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct Counting(Box<dyn Backend>, Arc<AtomicUsize>, Arc<AtomicUsize>);
    impl Backend for Counting {
        fn id(&self) -> String {
            self.0.id()
        }
        fn list(&mut self) -> anyhow::Result<Vec<RemoteEntry>> {
            self.0.list()
        }
        fn download(&mut self, p: &str) -> anyhow::Result<Vec<u8>> {
            self.0.download(p)
        }
        fn upload(&mut self, p: &str, d: &[u8]) -> anyhow::Result<RemoteEntry> {
            self.0.upload(p, d)
        }
        fn delete(&mut self, p: &str) -> anyhow::Result<()> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.delete(p)
        }
        fn delete_many(&mut self, paths: &[String]) -> anyhow::Result<()> {
            self.2.fetch_add(1, Ordering::Relaxed);
            for p in paths {
                self.0.delete(p)?;
            }
            Ok(())
        }
    }

    let e = env();
    let p = profile(&e.a, &e.remote);
    let singles = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));
    let inner = backend::create_backend(&p.backend, &e.data_a).unwrap();
    let b = Box::new(Counting(inner, singles.clone(), batches.clone()));
    let mut ea = SyncEngine::with_backend(p, "PC", b, &e.data_a).unwrap();
    for i in 0..250 {
        write(&e.a.join(format!("{i}.md")), "x");
    }
    ea.sync_once(&noop).unwrap();
    for i in 0..240 {
        fs::remove_file(e.a.join(format!("{i}.md"))).unwrap();
    }
    ea.allow_deletions_once();
    let r = ea.sync_once(&noop).unwrap();
    assert_eq!(r.deleted_remote, 240);
    assert_eq!(singles.load(Ordering::Relaxed), 0);
    assert_eq!(batches.load(Ordering::Relaxed), 2); // 200 + 40
}

#[test]
fn change_marker_skips_full_listing_and_signals_other_devices() {
    use obsidian_sync_core::backend::{self, Backend, RemoteEntry};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// 전체 목록 조회 횟수를 세는 래퍼 (변경 신호는 그대로 전달)
    struct Lists(Box<dyn Backend>, Arc<AtomicUsize>);
    impl Backend for Lists {
        fn id(&self) -> String {
            self.0.id()
        }
        fn list(&mut self) -> anyhow::Result<Vec<RemoteEntry>> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.list()
        }
        fn download(&mut self, p: &str) -> anyhow::Result<Vec<u8>> {
            self.0.download(p)
        }
        fn upload(&mut self, p: &str, d: &[u8]) -> anyhow::Result<RemoteEntry> {
            self.0.upload(p, d)
        }
        fn delete(&mut self, p: &str) -> anyhow::Result<()> {
            self.0.delete(p)
        }
        fn change_marker(&mut self) -> anyhow::Result<Option<String>> {
            self.0.change_marker()
        }
        fn touch_change_marker(&mut self, t: &str) -> anyhow::Result<()> {
            self.0.touch_change_marker(t)
        }
    }

    let e = env();
    let lists = Arc::new(AtomicUsize::new(0));
    let mk = |vault: &Path, data: &Path, dev: &str| {
        let p = profile(vault, &e.remote);
        let inner = backend::create_backend(&p.backend, data).unwrap();
        SyncEngine::with_backend(p, dev, Box::new(Lists(inner, lists.clone())), data).unwrap()
    };
    let mut a = mk(&e.a, &e.data_a, "PC");
    let mut b = mk(&e.b, &e.data_b, "Phone");

    write(&e.a.join("note.md"), "v1");
    a.sync_once(&noop).unwrap();
    b.sync_once(&noop).unwrap();
    assert_eq!(fs::read_to_string(e.b.join("note.md")).unwrap(), "v1");
    // 변경 신호 파일은 동기화 대상이 아님
    assert!(!e.b.join(".obsync").exists());
    assert!(e.remote.join(".obsync/changed").exists());

    // 아무도 바꾸지 않았으면 다른 기기 빠른 확인은 "변경 없음"
    assert!(!b.remote_changed());

    // 내 기기에서 저장 → 전체 목록 조회 없이 업로드
    let before = lists.load(Ordering::Relaxed);
    // 크기를 바꿔 씀: 같은 크기·같은 수정시각(파일시스템 시간 해상도 안)이면 변경으로 보지 않음
    write(&e.a.join("note.md"), "v2 edited");
    write(&e.a.join("new.md"), "n");
    let rep = a.sync_once(&noop).unwrap();
    assert_eq!(rep.uploaded, 2);
    assert_eq!(
        lists.load(Ordering::Relaxed),
        before,
        "목록 조회를 건너뛰어야 함"
    );
    assert_eq!(
        fs::read_to_string(e.remote.join("note.md")).unwrap(),
        "v2 edited"
    );

    // 다른 기기는 변경 신호로 바로 알아채고 받아옴
    assert!(b.remote_changed());
    b.sync_once(&noop).unwrap();
    assert_eq!(
        fs::read_to_string(e.b.join("note.md")).unwrap(),
        "v2 edited"
    );
    assert_eq!(fs::read_to_string(e.b.join("new.md")).unwrap(), "n");
    assert!(!a.remote_changed() || a.sync_once(&noop).unwrap().changes() == 0);

    // 다른 프로그램이 저장소를 직접 바꾸면 신호가 없으므로 주기적 전체 확인에서 잡힘
    write(&e.remote.join("outside.md"), "o");
    assert_eq!(a.sync_once(&noop).unwrap().downloaded, 0);
    a.force_full_list();
    assert_eq!(a.sync_once(&noop).unwrap().downloaded, 1);
    assert!(e.a.join("outside.md").exists());

    // 내 기기에서 지운 것도 목록 조회 없이 반영
    let before = lists.load(Ordering::Relaxed);
    fs::remove_file(e.a.join("new.md")).unwrap();
    assert_eq!(a.sync_once(&noop).unwrap().deleted_remote, 1);
    assert_eq!(lists.load(Ordering::Relaxed), before);
    assert!(!e.remote.join("new.md").exists());
}

#[test]
fn pending_mass_deletes_do_not_block_followup_edits() {
    let e = env();
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 0..50 {
        write(&e.a.join(format!("{i}.md")), "base");
    }
    a.sync_once(&noop).unwrap();
    for i in 0..40 {
        fs::remove_file(e.a.join(format!("{i}.md"))).unwrap();
    }
    let mut b = engine(&e.b, &e.remote, &e.data_b, "Phone");
    b.sync_once(&noop).unwrap();
    assert!(a.sync_once(&noop).is_err());
    write(&e.a.join("49.md"), "followup edit");
    assert!(a.sync_once(&noop).is_err());
    assert_eq!(
        fs::read_to_string(e.remote.join("49.md")).unwrap(),
        "followup edit"
    );
    assert!(
        b.remote_changed(),
        "partial progress must notify other devices"
    );
    b.sync_once(&noop).unwrap();
    assert_eq!(
        fs::read_to_string(e.b.join("49.md")).unwrap(),
        "followup edit"
    );
    assert!(
        e.remote.join("0.md").exists(),
        "unapproved deletion must stay protected"
    );
    drop(a);
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 50..90 {
        write(&e.a.join(format!("{i}.md")), "new file");
    }
    assert!(a.sync_once(&noop).is_err());
    drop(a);
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    assert!(
        a.sync_once(&noop).is_err(),
        "restart must preserve deletion protection"
    );
    a.allow_deletions_once();
    assert_eq!(a.sync_once(&noop).unwrap().deleted_remote, 40);
    assert_eq!(a.sync_once(&noop).unwrap().changes(), 0);
}

#[test]
fn bulk_edits_resume_after_mid_batch_cancel_without_reset() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let e = env();
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 0..100 {
        write(&e.a.join(format!("{i}.md")), "base");
    }
    a.sync_once(&noop).unwrap();
    for i in 0..100 {
        write(&e.a.join(format!("{i}.md")), "bulk modified");
    }
    let cancel = a.cancel_flag();
    let progress = AtomicUsize::new(0);
    let r = a
        .sync_once(&|line| {
            if line.starts_with('↑') && progress.fetch_add(1, Ordering::Relaxed) == 9 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();
    assert_eq!(r.uploaded, 10);
    drop(a);
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    assert_eq!(a.sync_once(&noop).unwrap().uploaded, 90);
    write(&e.a.join("99.md"), "followup after restart");
    assert_eq!(a.sync_once(&noop).unwrap().uploaded, 1);
    assert_eq!(a.sync_once(&noop).unwrap().changes(), 0);
    for i in 0..99 {
        assert_eq!(
            fs::read_to_string(e.remote.join(format!("{i}.md"))).unwrap(),
            "bulk modified"
        );
    }
}

#[test]
fn pending_delete_latch_clears_when_files_return_and_rearms_after_approval() {
    let e = env();
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 0..50 {
        write(&e.a.join(format!("{i}.md")), "base");
    }
    a.sync_once(&noop).unwrap();
    for i in 0..40 {
        fs::remove_file(e.a.join(format!("{i}.md"))).unwrap();
    }
    assert!(a.sync_once(&noop).is_err());
    // A canceled scan must not clear the persisted hold.
    a.cancel_flag()
        .store(true, std::sync::atomic::Ordering::Relaxed);
    a.sync_once(&noop).unwrap();
    drop(a);
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    assert!(a.sync_once(&noop).is_err());
    assert!(e.remote.join("0.md").exists());
    // Reappearance of all files resolves the held plan without approval/reset.
    for i in 0..40 {
        write(&e.a.join(format!("{i}.md")), "base");
    }
    a.sync_once(&noop).unwrap();
    drop(a);
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    fs::remove_file(e.a.join("0.md")).unwrap();
    assert_eq!(a.sync_once(&noop).unwrap().deleted_remote, 1);
    for i in 1..40 {
        fs::remove_file(e.a.join(format!("{i}.md"))).unwrap();
    }
    assert!(a.sync_once(&noop).is_err());
    a.allow_deletions_once();
    assert_eq!(a.sync_once(&noop).unwrap().deleted_remote, 39);
    // Approval is not a permanent waiver for later mass deletions.
    for i in 50..90 {
        write(&e.a.join(format!("{i}.md")), "later");
    }
    a.sync_once(&noop).unwrap();
    for i in 50..90 {
        fs::remove_file(e.a.join(format!("{i}.md"))).unwrap();
    }
    assert!(a.sync_once(&noop).is_err());
    assert!(e.remote.join("50.md").exists());
}

#[cfg(windows)]
#[test]
fn unreadable_files_cannot_clear_pending_deletion_protection() {
    use std::os::windows::fs::OpenOptionsExt;
    let e = env();
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    for i in 0..50 {
        write(&e.a.join(format!("{i}.md")), "base");
    }
    a.sync_once(&noop).unwrap();
    for i in 0..40 {
        fs::remove_file(e.remote.join(format!("{i}.md"))).unwrap();
    }
    a.force_full_list();
    assert!(a.sync_once(&noop).is_err());
    for i in 50..90 {
        write(&e.a.join(format!("{i}.md")), "new");
    }
    assert!(a.sync_once(&noop).is_err());
    let mut locks = Vec::new();
    for i in 0..40 {
        let p = e.a.join(format!("{i}.md"));
        write(&p, "temporarily unreadable");
        locks.push(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(p)
                .unwrap(),
        );
    }
    let unreadable = a.sync_once(&noop).unwrap();
    assert_eq!(unreadable.errors.len(), 40);
    assert_eq!(unreadable.deleted_local, 0);
    drop(locks);
    for i in 0..40 {
        write(&e.a.join(format!("{i}.md")), "base");
    }
    drop(a);
    let mut a = engine(&e.a, &e.remote, &e.data_a, "PC");
    assert!(
        a.sync_once(&noop).is_err(),
        "incomplete plan must preserve hold"
    );
    assert!(e.a.join("0.md").exists());
}
