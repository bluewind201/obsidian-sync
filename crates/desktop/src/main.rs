// 릴리즈 빌드에서는 콘솔 창을 띄우지 않음
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_os = "android"))]
use obsidian_sync_desktop::{api, platform, studio_ui::StudioApp};
#[cfg(not(target_os = "android"))]
use std::sync::{Arc, Mutex};
#[cfg(not(target_os = "android"))]
use std::time::Duration;

#[cfg(not(target_os = "android"))]
fn main() {
    if !platform::single_instance() {
        return;
    }

    // 이전 업데이트 후 남은 .old / .new.tmp 정리 및 자동 실행 레지스트리 경로 최신화
    platform::ensure_autostart_current_exe();
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(1500));
        obsidian_sync_desktop::updater::cleanup_old_exe_on_startup();
    });

    let hide = std::env::args().any(|a| a == "--minimized");

    let trigger: platform::SharedTrigger = Arc::new(Mutex::new(None));
    let backend_state = Arc::new(Mutex::new(api::BackendState::new(trigger.clone())));

    // 창이 시스템 트레이에 숨겨져 있는 동안에도 서비스 중지/재시작/초기화 상태 머신이 멈추지 않도록 백그라운드 폴링
    {
        let st = backend_state.clone();
        let _ = std::thread::Builder::new()
            .name("obsidian-sync-lifecycle".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(200));
                if let Ok(mut guard) = st.lock() {
                    guard.sync.poll_lifecycle();
                }
            });
    }

    // 백그라운드 GitHub 릴리즈 자동 업데이트 확인 및 다운로드 (창이 트레이에 숨겨져 있으면 조용히 자동 적용)
    {
        let st = backend_state.clone();
        let updater = st.lock().unwrap().updater.clone();
        let _ = std::thread::Builder::new()
            .name("onyx-auto-updater".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(6));
                loop {
                    let auto = updater.is_auto_update_enabled();
                    let _ = updater.check_and_maybe_download_sync(auto);
                    let status = updater.status();
                    if status.auto_update && status.ready_to_install && !status.applying {
                        let h = updater.main_hwnd();
                        let hidden_in_tray = h != 0 && !platform::is_window_visible_hwnd(h);
                        if hidden_in_tray {
                            let is_syncing = st
                                .lock()
                                .ok()
                                .and_then(|mut g| g.sync.snapshot().status.map(|s| s.syncing))
                                .unwrap_or(false);
                            if !is_syncing {
                                if let Ok(staged) = updater.prepare_staged_for_apply() {
                                    let stop_join = {
                                        let mut g = st.lock().unwrap();
                                        g.sync.stop_service();
                                        g.sync.stopping.take()
                                    };
                                    if let Some(j) = stop_join {
                                        let _ = j.join();
                                    }
                                    if updater.swap_and_restart(&staged).is_ok() {
                                        std::thread::sleep(Duration::from_millis(120));
                                        std::process::exit(0);
                                    }
                                }
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_secs(3 * 60 * 60));
                }
            });
    }

    let icon_data = eframe::egui::IconData {
        rgba: platform::icon_rgba(64),
        width: 64,
        height: 64,
    };

    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Onyx Studio")
        .with_app_id("com.obsidiansync.onyxstudio")
        .with_inner_size([1380.0, 880.0])
        .with_min_inner_size([900.0, 600.0])
        .with_visible(!hide)
        .with_icon(Arc::new(icon_data));

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Onyx Studio",
        native_options,
        Box::new(move |cc| Ok(Box::new(StudioApp::new(cc, backend_state, trigger, hide)))),
    ) {
        platform::show_error_box(
            "Onyx Studio 시작 오류",
            &format!("네이티브 그래픽 창(OpenGL/Glow)을 초기화할 수 없습니다:\n{e}"),
        );
    }
}

#[cfg(target_os = "android")]
fn main() {}
