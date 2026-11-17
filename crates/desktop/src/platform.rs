//! Windows 전용 기능 (트레이 아이콘, 창 숨기기, 중복 실행 방지, 시작 프로그램 등록, 파일/폴더 선택)
//! 다른 OS에서는 빈 구현으로 대체되어 개발/테스트 빌드가 가능합니다.

use obsidian_sync_core::service::Trigger;
use std::sync::{Arc, Mutex};

pub type SharedTrigger = Arc<Mutex<Option<Trigger>>>;

/// Onyx Studio 앱 아이콘 RGBA 생성 (딥 오닉스 스퀘클 배경 + 4면 패싯 크리스탈 젬)
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((size * size * 4) as usize);
    let half = (size as f32 - 1.0) * 0.5;
    let max_d = size.max(1) as f32;
    let lerp = |a: f32, b: f32, t: f32| -> u8 { (a + (b - a) * t).round().clamp(0.0, 255.0) as u8 };
    let blend = |bg: [u8; 4], fg: [u8; 3], alpha: f32| -> [u8; 4] {
        let a = alpha.clamp(0.0, 1.0);
        [
            lerp(bg[0] as f32, fg[0] as f32, a),
            lerp(bg[1] as f32, fg[1] as f32, a),
            lerp(bg[2] as f32, fg[2] as f32, a),
            255,
        ]
    };

    for y in 0..size {
        for x in 0..size {
            let nx = if half > 0.0 {
                (x as f32 - half) / half
            } else {
                0.0
            };
            let ny = if half > 0.0 {
                (y as f32 - half) / half
            } else {
                0.0
            };

            // Rounded-square SDF (corner radius 0.28 in normalized -1..1 space)
            let qx = (nx.abs() - 0.68).max(0.0);
            let qy = (ny.abs() - 0.68).max(0.0);
            let sdf = (qx * qx + qy * qy).sqrt() - 0.28;
            let px_edge = 2.0 / max_d;
            let bg_alpha = (0.5 - sdf / px_edge).clamp(0.0, 1.0);

            if bg_alpha <= 0.0 {
                v.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }

            // Deep obsidian slate gradient
            let t = ((ny + 1.0) * 0.5).clamp(0.0, 1.0);
            let mut px = [
                lerp(15.0, 30.0, t),
                lerp(23.0, 41.0, t),
                lerp(42.0, 59.0, t),
                (bg_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
            ];

            // Subtle inner sky-indigo rim along squircle border
            let rim = 1.0 - ((sdf + 0.04).abs() / 0.04).clamp(0.0, 1.0);
            if rim > 0.0 && bg_alpha > 0.9 {
                px = blend(px, [56, 189, 248], rim * 0.35);
            }

            // Outer crystal gem silhouette (elongated diamond / hex-cut)
            let gem_d = (nx.abs() * 1.24 + ny.abs()) - 0.66;
            let gem_cov = (0.5 - gem_d / (px_edge * 1.2)).clamp(0.0, 1.0);
            if gem_cov > 0.0 && bg_alpha > 0.9 {
                let facet = match (nx >= 0.0, ny >= 0.0) {
                    (false, false) => [56, 189, 248], // top-left bright cyan (#38BDF8)
                    (true, false) => [129, 140, 248], // top-right light indigo (#818CF8)
                    (false, true) => [2, 132, 199],   // bottom-left ocean blue (#0284C7)
                    (true, true) => [99, 102, 241],   // bottom-right indigo (#6366F1)
                };
                px = blend(px, facet, gem_cov);

                // Inner crisp specular diamond facet
                let core_d = (nx.abs() * 1.24 + ny.abs()) - 0.25;
                let core_cov = (0.5 - core_d / (px_edge * 1.2)).clamp(0.0, 1.0);
                if core_cov > 0.0 {
                    px = blend(px, [248, 250, 252], core_cov * 0.92);
                }
            }

            v.extend_from_slice(&px);
        }
    }
    v
}

#[cfg(windows)]
mod imp {
    use super::SharedTrigger;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::time::Duration;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, OpenEventW, OpenProcess, QueryFullProcessImageNameW, SetEvent,
        TerminateProcess, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, BringWindowToTop, FindWindowExW, GetWindowThreadProcessId,
        IsIconic, MessageBoxW, SendMessageTimeoutW, SetForegroundWindow, ShowWindow, MB_ICONERROR,
        MB_OK, SMTO_ABORTIFHUNG, SW_HIDE, SW_RESTORE, SW_SHOW, WM_NULL,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "OnyxStudio";
    const SINGLE_INSTANCE_MUTEX_NAME: &str = "Local\\OnyxStudioSingleInstance";
    const ACTIVATE_EVENT_NAME: &str = "Local\\OnyxStudioActivateEvent";

    static INSTANCE_MUTEX_HANDLE: AtomicIsize = AtomicIsize::new(0);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn pid_file_path() -> PathBuf {
        std::env::temp_dir().join("onyx-studio-instance.pid")
    }

    fn write_current_pid_file() {
        let _ = std::fs::write(pid_file_path(), std::process::id().to_string());
    }

    fn read_saved_pid_file() -> Option<u32> {
        let raw = std::fs::read_to_string(pid_file_path()).ok()?;
        raw.trim().parse::<u32>().ok().filter(|&p| p != 0)
    }

    fn norm_exe_key(p: &Path) -> String {
        let resolved = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let mut s = resolved.to_string_lossy().replace('/', "\\");
        if let Some(stripped) = s.strip_prefix("\\\\?\\") {
            s = stripped.to_string();
        }
        s.to_lowercase()
    }

    unsafe fn query_process_exe(pid: u32) -> Option<PathBuf> {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok != 0 && len > 0 {
            let s = String::from_utf16_lossy(&buf[..len as usize]);
            Some(PathBuf::from(s))
        } else {
            None
        }
    }

    unsafe fn is_onyx_process(pid: u32) -> bool {
        let Some(exe) = query_process_exe(pid) else {
            return false;
        };
        let lower = exe
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        lower.contains("onyxstudio") || lower.contains("obsidiansync")
    }

    unsafe fn is_same_running_binary(pid: u32) -> bool {
        let Some(other_exe) = query_process_exe(pid) else {
            return false;
        };
        let Ok(self_exe) = std::env::current_exe() else {
            return false;
        };
        norm_exe_key(&other_exe) == norm_exe_key(&self_exe)
    }

    unsafe fn is_window_responsive(hwnd: HWND) -> bool {
        let mut res: usize = 0;
        SendMessageTimeoutW(hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 1500, &mut res) != 0
    }

    unsafe fn terminate_stale_process(pid: u32) {
        if pid == 0 || pid == std::process::id() {
            return;
        }
        let h = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return;
        }
        let _ = TerminateProcess(h, 1);
        let _ = WaitForSingleObject(h, 1500);
        CloseHandle(h);
    }

    unsafe fn find_onyx_window() -> Option<(HWND, u32)> {
        let title = wide("Onyx Studio");
        let mut prev: HWND = std::ptr::null_mut();
        loop {
            let h = FindWindowExW(std::ptr::null_mut(), prev, std::ptr::null(), title.as_ptr());
            if h.is_null() {
                return None;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(h, &mut pid);
            if pid != 0 && pid != std::process::id() && is_onyx_process(pid) {
                return Some((h, pid));
            }
            prev = h;
        }
    }

    pub fn present_hwnd(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as HWND;
            ShowWindow(h, SW_SHOW);
            if IsIconic(h) != 0 {
                ShowWindow(h, SW_RESTORE);
            }
            BringWindowToTop(h);
            SetForegroundWindow(h);
        }
    }

    pub fn signal_activate_event() {
        let name = wide(ACTIVATE_EVENT_NAME);
        unsafe {
            let h = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
            if !h.is_null() {
                SetEvent(h);
                CloseHandle(h);
            }
        }
    }

    pub fn show_window(hwnd: isize) {
        present_hwnd(hwnd);
        signal_activate_event();
    }

    pub fn hide_window(hwnd: isize) -> bool {
        if hwnd == 0 {
            return false;
        }
        unsafe {
            ShowWindow(hwnd as HWND, SW_HIDE);
        }
        true
    }

    pub fn show_error_box(title: &str, message: &str) {
        let t = wide(title);
        let m = wide(message);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                m.as_ptr(),
                t.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    pub fn spawn_activation_listener(
        event_proxy: tao::event_loop::EventLoopProxy<crate::AppUserEvent>,
    ) {
        let name = wide(ACTIVATE_EVENT_NAME);
        let h_raw: isize = unsafe {
            let h: HANDLE = CreateEventW(std::ptr::null(), 0, 0, name.as_ptr());
            if h.is_null() {
                return;
            }
            h as isize
        };
        let _ = std::thread::Builder::new()
            .name("onyx-activate-listener".into())
            .spawn(move || unsafe {
                let h = h_raw as HANDLE;
                loop {
                    let wait = WaitForSingleObject(h, INFINITE);
                    if wait != WAIT_OBJECT_0 {
                        break;
                    }
                    if event_proxy
                        .send_event(crate::AppUserEvent::ShowMainWindow)
                        .is_err()
                    {
                        break;
                    }
                }
                CloseHandle(h);
            });
    }

    /// 이미 실행 중이면 기존 창을 앞으로 가져오고 `false` 반환.
    /// 구버전/다른 경로 바이너리거나 응답 없는 좀비 인스턴스면 정리 후 `true` 반환.
    pub fn single_instance() -> bool {
        let name = wide(SINGLE_INSTANCE_MUTEX_NAME);
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if !h.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
                let mut existing = find_onyx_window();
                if existing.is_none() {
                    std::thread::sleep(Duration::from_millis(250));
                    existing = find_onyx_window();
                }
                if let Some((hwnd, pid)) = existing {
                    if is_same_running_binary(pid) && is_window_responsive(hwnd) {
                        AllowSetForegroundWindow(pid);
                        signal_activate_event();
                        present_hwnd(hwnd as isize);
                        CloseHandle(h);
                        return false;
                    }
                    // 다른 경로(구버전) 바이너리이거나 응답 없는 창이면 기존 프로세스를 종료하고 새로 시작
                    terminate_stale_process(pid);
                    std::thread::sleep(Duration::from_millis(120));
                } else if let Some(stale_pid) = read_saved_pid_file() {
                    if stale_pid != std::process::id() && is_onyx_process(stale_pid) {
                        terminate_stale_process(stale_pid);
                        std::thread::sleep(Duration::from_millis(120));
                    }
                }
            }
            if !h.is_null() {
                INSTANCE_MUTEX_HANDLE.store(h as isize, Ordering::SeqCst);
            }
            write_current_pid_file();
            true
        }
    }

    pub fn create_tray(hwnd: isize, trigger: SharedTrigger) -> Option<TrayIcon> {
        let menu = Menu::new();
        let open = MenuItem::new("열기", true, None);
        let sync = MenuItem::new("지금 동기화", true, None);
        let quit = MenuItem::new("종료", true, None);
        menu.append_items(&[&open, &sync, &PredefinedMenuItem::separator(), &quit])
            .ok()?;
        let (open_id, sync_id, quit_id) = (open.id().clone(), sync.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == open_id {
                show_window(hwnd);
            } else if e.id == sync_id {
                if let Some(t) = trigger.lock().unwrap().as_ref() {
                    t.sync_now();
                }
            } else if e.id == quit_id {
                std::process::exit(0);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            let show = matches!(
                e,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick { .. }
            );
            if show {
                show_window(hwnd);
            }
        }));
        let icon = Icon::from_rgba(super::icon_rgba(32), 32, 32).ok()?;
        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Onyx Studio")
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()
            .ok()
    }

    pub fn autostart_enabled() -> bool {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        hkcu.open_subkey(RUN_KEY)
            .and_then(|k| k.get_value::<String, _>(APP_NAME))
            .is_ok()
    }

    pub fn set_autostart(enable: bool) -> anyhow::Result<()> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(RUN_KEY)?;
        if enable {
            let exe = std::env::current_exe()?;
            key.set_value(APP_NAME, &format!("\"{}\" --minimized", exe.display()))?;
        } else {
            let _ = key.delete_value(APP_NAME);
        }
        Ok(())
    }

    pub fn pick_folder() -> Option<String> {
        rfd::FileDialog::new()
            .pick_folder()
            .map(|p| p.to_string_lossy().to_string())
    }

    pub fn pick_image_file() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"],
            )
            .pick_file()
    }

    pub fn pick_any_file() -> Option<PathBuf> {
        rfd::FileDialog::new().pick_file()
    }

    pub fn save_pdf_dialog(default_filename: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_file_name(default_filename)
            .add_filter("PDF 문서 (*.pdf)", &["pdf"])
            .save_file()
    }

    pub type Tray = TrayIcon;
}

#[cfg(not(windows))]
mod imp {
    use super::SharedTrigger;
    use std::path::PathBuf;

    pub type Tray = ();

    #[allow(dead_code)]
    pub fn present_hwnd(_: isize) {}

    #[allow(dead_code)]
    pub fn signal_activate_event() {}

    #[allow(dead_code)]
    pub fn show_window(_: isize) {}

    #[allow(dead_code)]
    pub fn hide_window(_: isize) -> bool {
        false
    }

    #[allow(dead_code)]
    pub fn show_error_box(_: &str, _: &str) {}

    #[allow(dead_code)]
    pub fn single_instance() -> bool {
        true
    }

    #[allow(dead_code)]
    pub fn create_tray(_: isize, _: SharedTrigger) -> Option<Tray> {
        None
    }

    pub fn autostart_enabled() -> bool {
        false
    }

    pub fn set_autostart(_: bool) -> anyhow::Result<()> {
        anyhow::bail!("Windows에서만 지원됩니다")
    }

    pub fn pick_folder() -> Option<String> {
        None
    }

    pub fn pick_image_file() -> Option<PathBuf> {
        None
    }

    pub fn pick_any_file() -> Option<PathBuf> {
        None
    }

    pub fn save_pdf_dialog(_default_filename: &str) -> Option<PathBuf> {
        None
    }
}

pub use imp::*;
