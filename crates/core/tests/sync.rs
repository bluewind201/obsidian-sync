use std::{fs, path::Path};
use tempfile::TempDir;
use vault_sync_core::{sync, validate_path, Config};

fn setup(vault: &Path, remote: &Path) -> Config {
    fs::create_dir_all(vault).unwrap();
    fs::create_dir_all(remote).unwrap();
    Config {
        vault: vault.into(),
        options: [("root".into(), remote.to_string_lossy().into())].into(),
        ..Default::default()
    }
}
#[tokio::test]
async fn roundtrip_updates_and_binary() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let a = setup(&d.join("a"), &d.join("remote"));
    let b = setup(&d.join("b"), &d.join("remote"));
    fs::create_dir(a.vault.join("한글")).unwrap();
    fs::write(a.vault.join("한글/노트.md"), "hello").unwrap();
    fs::write(a.vault.join("image.png"), [0, 255, 10, 32]).unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    sync(&b, &d.join("sb")).await.unwrap();
    assert_eq!(
        fs::read(b.vault.join("image.png")).unwrap(),
        [0, 255, 10, 32]
    );
    fs::write(b.vault.join("한글/노트.md"), "changed").unwrap();
    sync(&b, &d.join("sb")).await.unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    assert_eq!(
        fs::read_to_string(a.vault.join("한글/노트.md")).unwrap(),
        "changed"
    );
    assert_eq!(sync(&a, &d.join("sa")).await.unwrap().uploaded, 0);
}
#[tokio::test]
async fn concurrent_changes_preserve_both_and_converge() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let a = setup(&d.join("a"), &d.join("remote"));
    let b = setup(&d.join("b"), &d.join("remote"));
    fs::write(a.vault.join("note.md"), "base").unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    sync(&b, &d.join("sb")).await.unwrap();
    fs::write(a.vault.join("note.md"), "A").unwrap();
    fs::write(b.vault.join("note.md"), "B").unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    assert_eq!(sync(&b, &d.join("sb")).await.unwrap().conflicts, 1);
    sync(&a, &d.join("sa")).await.unwrap();
    assert_eq!(
        fs::read(a.vault.join("note.md")).unwrap(),
        fs::read(b.vault.join("note.md")).unwrap()
    );
    let copies: Vec<_> = fs::read_dir(b.vault.join(".sync-conflicts"))
        .unwrap()
        .map(|e| fs::read(e.unwrap().path()).unwrap())
        .collect();
    assert!(copies.contains(&b"A".to_vec()) && copies.contains(&b"B".to_vec()));
    fs::write(b.vault.join("note.md"), "merged").unwrap();
    sync(&b, &d.join("sb")).await.unwrap();
    assert_eq!(sync(&a, &d.join("sa")).await.unwrap().conflicts, 0);
}
#[tokio::test]
async fn deletes_default_to_restore_and_opt_in_to_backup() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let mut a = setup(&d.join("a"), &d.join("remote"));
    let mut b = setup(&d.join("b"), &d.join("remote"));
    fs::write(a.vault.join("note.md"), "keep").unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    sync(&b, &d.join("sb")).await.unwrap();
    fs::remove_file(a.vault.join("note.md")).unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    assert!(a.vault.join("note.md").exists());
    a.propagate_deletes = true;
    b.propagate_deletes = true;
    fs::remove_file(a.vault.join("note.md")).unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    sync(&b, &d.join("sb")).await.unwrap();
    assert!(!b.vault.join("note.md").exists());
    assert!(b.vault.join(".sync-backups").is_dir());
}
#[tokio::test]
async fn missing_vault_and_remote_history_fail_closed() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let a = setup(&d.join("a"), &d.join("remote"));
    fs::write(a.vault.join("note.md"), "keep").unwrap();
    sync(&a, &d.join("state")).await.unwrap();
    fs::remove_dir_all(d.join("remote/vault-sync-v1/events")).unwrap();
    assert!(sync(&a, &d.join("state")).await.is_err());
    assert_eq!(fs::read(a.vault.join("note.md")).unwrap(), b"keep");
    fs::remove_dir_all(&a.vault).unwrap();
    assert!(sync(&a, &d.join("state")).await.is_err());
}
#[tokio::test]
async fn mass_delete_is_blocked() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let mut a = setup(&d.join("a"), &d.join("remote"));
    a.propagate_deletes = true;
    for i in 0..12 {
        fs::write(a.vault.join(format!("{i}.md")), "keep").unwrap();
    }
    sync(&a, &d.join("state")).await.unwrap();
    for i in 0..12 {
        fs::remove_file(a.vault.join(format!("{i}.md"))).unwrap();
    }
    assert!(sync(&a, &d.join("state")).await.is_err());
}
#[test]
fn rejects_traversal_and_windows_aliases() {
    for path in [
        "../x",
        "/x",
        "a//b",
        "a\\b",
        "C:/x",
        "x:ads",
        "CON.md",
        "a. ",
        ".sync-backups/x",
    ] {
        assert!(validate_path(path).is_err(), "{path}");
    }
    assert!(validate_path("한글/notes.md").is_ok());
}
#[tokio::test]
async fn corrupt_blob_does_not_replace_local_file() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let a = setup(&d.join("a"), &d.join("remote"));
    let b = setup(&d.join("b"), &d.join("remote"));
    fs::write(a.vault.join("x.md"), "good").unwrap();
    sync(&a, &d.join("sa")).await.unwrap();
    for item in fs::read_dir(d.join("remote/vault-sync-v1/blobs")).unwrap() {
        fs::write(item.unwrap().path(), "corrupt").unwrap();
    }
    assert!(sync(&b, &d.join("sb")).await.is_err());
    assert!(!b.vault.join("x.md").exists());
}
#[tokio::test]
async fn nested_remote_is_rejected() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let a = setup(&d.join("a"), &d.join("a/remote"));
    assert!(sync(&a, &d.join("state")).await.is_err());
}
#[cfg(unix)]
#[tokio::test]
async fn symlinks_cannot_escape_vault() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    let a = setup(&d.join("a"), &d.join("remote"));
    fs::write(d.join("outside"), "private").unwrap();
    std::os::unix::fs::symlink(d.join("outside"), a.vault.join("link")).unwrap();
    assert!(sync(&a, &d.join("state")).await.is_err());
}
