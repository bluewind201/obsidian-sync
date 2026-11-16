// 릴리즈 빌드에서는 콘솔 창을 띄우지 않음
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use obsidian_sync_desktop::{api, get_web_asset, platform, AppUserEvent};

#[cfg(windows)]
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
fn main() {
    if !platform::single_instance() {
        return;
    }

    let hide = std::env::args().any(|a| a == "--minimized");
    let event_loop = tao::event_loop::EventLoopBuilder::<AppUserEvent>::with_user_event().build();
    let event_proxy = event_loop.create_proxy();
    let trigger: platform::SharedTrigger = Arc::new(Mutex::new(None));
    let backend_state = Arc::new(Mutex::new(api::BackendState::new_with_proxy(
        trigger.clone(),
        Some(event_proxy),
    )));
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
        let _keep_alive = (&window, &web_context, &tray);
        *control_flow = tao::event_loop::ControlFlow::Wait;

        match event {
            tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                if has_tray {
                    platform::hide_window(hwnd);
                } else {
                    *control_flow = tao::event_loop::ControlFlow::Exit;
                }
            }
            tao::event::Event::UserEvent(AppUserEvent::ExportPdf {
                output_path,
                landscape,
                margin_top_cm,
                margin_bottom_cm,
                margin_left_cm,
                margin_right_cm,
                reply,
            }) => {
                export_webview_to_pdf(
                    &webview,
                    output_path,
                    landscape,
                    margin_top_cm,
                    margin_bottom_cm,
                    margin_left_cm,
                    margin_right_cm,
                    reply,
                );
            }
            _ => {}
        }
    });
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn export_webview_to_pdf(
    webview: &wry::WebView,
    output_path: std::path::PathBuf,
    landscape: bool,
    margin_top_cm: f64,
    margin_bottom_cm: f64,
    margin_left_cm: f64,
    margin_right_cm: f64,
    reply: std::sync::mpsc::Sender<Result<(), String>>,
) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Environment6, ICoreWebView2_7, COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE,
        COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
    };
    use webview2_com::PrintToPdfCompletedHandler;
    use windows::core::{Interface, HSTRING};
    use wry::WebViewExtWindows;

    const CM_TO_INCHES: f64 = 1.0 / 2.54;

    let setup = || -> Result<
        (
            ICoreWebView2_7,
            webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2PrintSettings,
        ),
        String,
    > {
        let controller = webview.controller();
        let core = unsafe { controller.CoreWebView2() }
            .map_err(|e| format!("CoreWebView2 접근 실패: {e}"))?;
        let core7: ICoreWebView2_7 = core
            .cast()
            .map_err(|e| format!("ICoreWebView2_7 지원 안됨: {e}"))?;
        let env = unsafe { core7.Environment() }
            .map_err(|e| format!("WebView2 Environment 조회 실패: {e}"))?;
        let env6: ICoreWebView2Environment6 = env
            .cast()
            .map_err(|e| format!("ICoreWebView2Environment6 지원 안됨: {e}"))?;
        let settings = unsafe { env6.CreatePrintSettings() }
            .map_err(|e| format!("PrintSettings 생성 실패: {e}"))?;

        let orientation = if landscape {
            COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE
        } else {
            COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT
        };
        let (page_w, page_h) = if landscape {
            (29.7 * CM_TO_INCHES, 21.0 * CM_TO_INCHES)
        } else {
            (21.0 * CM_TO_INCHES, 29.7 * CM_TO_INCHES)
        };
        unsafe {
            let _ = settings.SetOrientation(orientation);
            let _ = settings.SetPageWidth(page_w);
            let _ = settings.SetPageHeight(page_h);
            let _ = settings.SetMarginTop((margin_top_cm * CM_TO_INCHES).max(0.0));
            let _ = settings.SetMarginBottom((margin_bottom_cm * CM_TO_INCHES).max(0.0));
            let _ = settings.SetMarginLeft((margin_left_cm * CM_TO_INCHES).max(0.0));
            let _ = settings.SetMarginRight((margin_right_cm * CM_TO_INCHES).max(0.0));
            let _ = settings.SetShouldPrintBackgrounds(true);
            let _ = settings.SetShouldPrintHeaderAndFooter(false);
            let _ = settings.SetShouldPrintSelectionOnly(false);
            let _ = settings.SetScaleFactor(1.0);
        }
        Ok((core7, settings))
    };

    let (core7, settings) = match setup() {
        Ok(v) => v,
        Err(msg) => {
            let _ = reply.send(Err(msg));
            return;
        }
    };

    if let Some(parent) = output_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            let _ = reply.send(Err(format!("PDF 대상 폴더 생성 실패: {e}")));
            return;
        }
    }

    let reply_cb = reply.clone();
    let handler = PrintToPdfCompletedHandler::create(Box::new(move |hr, is_successful| {
        let ok: bool = is_successful.into();
        let res = if hr.is_ok() && ok {
            Ok(())
        } else {
            Err(format!("WebView2 PDF 저장 실패 (hr={hr:?}, ok={ok})"))
        };
        let _ = reply_cb.send(res);
        Ok(())
    }));
    let wide_path = HSTRING::from(output_path.as_os_str());
    if let Err(e) = unsafe { core7.PrintToPdf(&wide_path, &settings, &handler) } {
        let _ = reply.send(Err(format!("PrintToPdf 호출 실패: {e}")));
    }
}

#[cfg(not(windows))]
fn main() {
    println!(
        "Obsidian Sync Desktop v{} (Windows WebView2 전용 실행 바이너리)",
        obsidian_sync_core::VERSION
    );
}
