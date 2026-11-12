// 릴리즈 빌드에서는 콘솔 창을 띄우지 않음
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

pub mod api;
pub mod platform;
pub mod sync_controller;
pub mod vault;

#[allow(dead_code)]
mod web_assets {
    include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));
}
pub use web_assets::get_web_asset;

pub use sync_controller::App;

#[cfg(test)]
use obsidian_sync_core::{AppConfig, BackendConfig, Profile};
#[cfg(test)]
use std::path::PathBuf;
#[cfg(any(windows, test))]
use std::sync::{Arc, Mutex};
#[cfg(any(windows, test))]
use std::time::Duration;

#[cfg(windows)]
fn main() {
    if !platform::single_instance() {
        return;
    }

    let hide = std::env::args().any(|a| a == "--minimized");
    let trigger: platform::SharedTrigger = Arc::new(Mutex::new(None));
    let backend_state = Arc::new(Mutex::new(api::BackendState::new(trigger.clone())));
    let data_dir = backend_state.lock().unwrap().sync.data_dir.clone();

    // 창이 트레이에 숨겨진 동안에도 서비스 중지/재시작/초기화 상태 머신이 멈추지 않도록 주기 폴링
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

    let event_loop = tao::event_loop::EventLoop::new();
    let window_icon = tao::window::Icon::from_rgba(platform::icon_rgba(64), 64, 64).ok();

    let window = tao::window::WindowBuilder::new()
        .with_title("Obsidian Sync")
        .with_inner_size(tao::dpi::LogicalSize::new(1400.0, 900.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(900.0, 600.0))
        .with_window_icon(window_icon)
        .with_visible(!hide)
        .build(&event_loop)
        .expect("애플리케이션 창을 생성할 수 없습니다");

    let hwnd = tao::platform::windows::WindowExtWindows::hwnd(&window) as isize;
    let tray = platform::create_tray(hwnd, trigger);
    let has_tray = tray.is_some();
    if hide && !has_tray {
        window.set_visible(true);
    }

    let mut web_context = wry::WebContext::new(Some(data_dir.join("webview")));
    let api_state = backend_state.clone();

    let webview = wry::WebViewBuilder::new_with_web_context(&mut web_context)
        .with_asynchronous_custom_protocol("app".into(), move |_id, request, responder| {
            let raw_path = request.uri().path().to_string();
            let path = percent_encoding::percent_decode_str(&raw_path)
                .decode_utf8_lossy()
                .to_string();

            if path.starts_with("/api/") {
                let st = api_state.clone();
                let body = request.into_body();
                std::thread::spawn(move || {
                    let json_bytes = api::handle_api(&st, &path, &body);
                    let resp = wry::http::Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json; charset=utf-8")
                        .header("Cache-Control", "no-store")
                        .header("Access-Control-Allow-Origin", "*")
                        .body(std::borrow::Cow::Owned(json_bytes))
                        .unwrap();
                    responder.respond(resp);
                });
                return;
            }

            if path == "/vault-media" {
                let query = request.uri().query().map(str::to_string);
                std::thread::spawn(move || {
                    let resp = match api::handle_vault_media(query.as_deref()) {
                        Some((mime, bytes)) => wry::http::Response::builder()
                            .status(200)
                            .header("Content-Type", mime)
                            .header("Cache-Control", "private, max-age=60")
                            .body(std::borrow::Cow::Owned(bytes))
                            .unwrap(),
                        None => wry::http::Response::builder()
                            .status(404)
                            .header("Content-Type", "text/plain; charset=utf-8")
                            .body(std::borrow::Cow::Borrowed(b"Not Found" as &[u8]))
                            .unwrap(),
                    };
                    responder.respond(resp);
                });
                return;
            }

            let asset = get_web_asset(&path).or_else(|| {
                if !path.contains('.') {
                    get_web_asset("/index.html")
                } else {
                    None
                }
            });

            let resp = match asset {
                Some((mime, bytes)) => wry::http::Response::builder()
                    .status(200)
                    .header("Content-Type", mime)
                    .body(std::borrow::Cow::Borrowed(bytes))
                    .unwrap(),
                None => {
                    let fallback = b"<!doctype html><html><head><meta charset=\"utf-8\"><title>Obsidian Sync</title></head><body>Web UI bundle not found.</body></html>";
                    wry::http::Response::builder()
                        .status(404)
                        .header("Content-Type", "text/html; charset=utf-8")
                        .body(std::borrow::Cow::Borrowed(fallback as &[u8]))
                        .unwrap()
                }
            };
            responder.respond(resp);
        })
        .with_new_window_req_handler(|url, _features| {
            if url.starts_with("http://") || url.starts_with("https://") {
                if !url.contains("app.localhost") {
                    let _ = open::that(&url);
                    return wry::NewWindowResponse::Deny;
                }
            }
            wry::NewWindowResponse::Allow
        })
        .with_url("app://localhost/")
        .build(&window)
        .expect("WebView2를 초기화할 수 없습니다");

    event_loop.run(move |event, _, control_flow| {
        let _keep_alive = (&window, &webview, &web_context, &tray);
        *control_flow = tao::event_loop::ControlFlow::Wait;

        if let tao::event::Event::WindowEvent {
            event: tao::event::WindowEvent::CloseRequested,
            ..
        } = event
        {
            if has_tray {
                platform::hide_window(hwnd);
            } else {
                *control_flow = tao::event_loop::ControlFlow::Exit;
            }
        }
    });
}

#[cfg(not(windows))]
fn main() {
    println!(
        "Obsidian Sync Desktop v{} (Windows WebView2 전용 실행 바이너리)",
        obsidian_sync_core::VERSION
    );
}

#[cfg(test)]
mod lifecycle_tests;
