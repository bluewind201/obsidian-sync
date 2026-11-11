use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Instant;

fn headless(cfg: AppConfig, data_dir: PathBuf) -> App {
    App {
        config_path: data_dir.join("config.json"),
        data_dir,
        saved: cfg.clone(),
        cfg,
        sel: 0,
        ignore_texts: HashMap::new(),
        backend_cache: HashMap::new(),
        service: None,
        stopping: None,
        trigger: Arc::new(Mutex::new(None)),
        pending_start: false,
        pending_reset: None,
        resetting: None,
        login: None,
        test: None,
        otp: String::new(),
        browser: None,
        confirm_delete: false,
        message: None,
        autostart: false,
        show_secret: false,
        page: Page::Home,
        hwnd: 0,
        hide_on_start: false,
        _tray: None,
        has_tray: false,
    }
}

// Real Synology HTTP request from a real SyncService worker. The server holds its
// response until released; no synthetic worker or sleep-only stop implementation.
fn slow_service() -> (
    App,
    tempfile::TempDir,
    mpsc::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let vault = tmp.path().join("vault");
    std::fs::create_dir(&vault).unwrap();
    std::fs::write(vault.join("keep.md"), "keep this data").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 8192];
        let n = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..n]).contains("/webapi/query.cgi"));
        entered_tx.send(()).unwrap();
        // Timeout also releases the original blocking callback for red evidence.
        let _ = release_rx.recv_timeout(Duration::from_millis(800));
        // Cache a valid discovery response. Unsupported APIs then fail locally,
        // avoiding unrelated connection-refused retries after the gate opens.
        let body = r#"{"success":true,"data":{}}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let p = Profile {
        local_path: vault.to_string_lossy().into(),
        backend: BackendConfig::Synology {
            url,
            username: "fixture".into(),
            password: "fixture".into(),
            folder: "/vault".into(),
        },
        ..Default::default()
    };
    let cfg = AppConfig {
        profiles: vec![p],
        watch_local: false,
        ..Default::default()
    };
    let mut app = headless(cfg, tmp.path().join("data"));
    app.start_service();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    (app, tmp, release_tx, server)
}

#[test]
fn restart_callback_does_not_wait_for_real_worker() {
    let (mut app, tmp, release, server) = slow_service();
    next_service_uses_local_fixture(&mut app, &tmp);
    app.stop_service();
    let start = Instant::now();
    app.start_service();
    let elapsed = start.elapsed();
    println!("restart callback returned in {elapsed:?}");
    app.start_service();
    app.poll_lifecycle();
    assert!(
        app.service.is_none(),
        "no new worker before the old one exits"
    );
    assert!(app.pending_start);
    let _ = release.send(());
    server.join().unwrap();
    finish_transition(&mut app);
    assert!(app.service.is_some());
    app.stop_service();
    finish_transition(&mut app);
    assert!(
        elapsed < Duration::from_millis(400),
        "restart callback blocked {elapsed:?}"
    );
}

#[test]
fn reset_callback_does_not_wait_for_real_worker() {
    let (mut app, tmp, release, server) = slow_service();
    let id = app.cfg.profiles[0].id.clone();
    let backend =
        obsidian_sync_core::backend::create_backend(&app.cfg.profiles[0].backend, &app.data_dir)
            .unwrap();
    let key = obsidian_sync_core::engine::hash_bytes(
        format!("{}\n{}", app.cfg.profiles[0].local_path, backend.id()).as_bytes(),
    );
    let state_path = app.data_dir.join(format!("state-{}.json", &key[..16]));
    std::fs::create_dir_all(&app.data_dir).unwrap();
    let state = obsidian_sync_core::state::SyncState {
        backend_id: backend.id(),
        ..Default::default()
    };
    state.save(&state_path).unwrap();
    let saved = std::fs::read(&state_path).unwrap();
    let start = Instant::now();
    app.reset_profile(id);
    let elapsed = start.elapsed();
    println!("reset callback returned in {elapsed:?}");
    next_service_uses_local_fixture(&mut app, &tmp);
    app.reset_profile(app.cfg.profiles[0].id.clone());
    app.start_service();
    app.poll_lifecycle();
    assert!(app.service.is_none());
    assert!(
        app.resetting.is_none(),
        "reset must wait for the old worker"
    );
    assert!(app.pending_reset.is_some());
    assert_eq!(
        std::fs::read(&state_path).unwrap(),
        saved,
        "no reset before stop completion"
    );
    let _ = release.send(());
    server.join().unwrap();
    finish_transition(&mut app);
    assert!(app.service.is_some());
    assert!(
        !state_path.exists(),
        "queued reset ran after stop completion"
    );
    app.stop_service();
    finish_transition(&mut app);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("vault/keep.md")).unwrap(),
        "keep this data"
    );
    assert!(
        elapsed < Duration::from_millis(400),
        "reset callback blocked {elapsed:?}"
    );
}

fn finish_transition(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.transitioning() || app.pending_start {
        assert!(Instant::now() < deadline, "lifecycle did not finish");
        app.poll_lifecycle();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn next_service_uses_local_fixture(app: &mut App, tmp: &tempfile::TempDir) {
    let remote = tmp.path().join("next-remote");
    std::fs::create_dir(&remote).unwrap();
    // Only the queued next service changes. The running service retains the
    // Synology backend and remains blocked at the server barrier.
    app.cfg.profiles[0].backend = BackendConfig::LocalFolder {
        path: remote.to_string_lossy().into(),
    };
}

#[test]
fn stop_cancels_queued_restart() {
    let (mut app, _tmp, release, server) = slow_service();
    app.start_service();
    app.stop_service();
    app.poll_lifecycle();
    assert!(!app.pending_start);
    assert!(app.service.is_none());
    release.send(()).unwrap();
    server.join().unwrap();
    finish_transition(&mut app);
    assert!(app.service.is_none());
}
