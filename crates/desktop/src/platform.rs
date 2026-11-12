//! Windows 전용 기능 (트레이 아이콘, 창 숨기기, 중복 실행 방지, 시작 프로그램 등록, 파일/폴더 선택)
//! 다른 OS에서는 빈 구현으로 대체되어 개발/테스트 빌드가 가능합니다.

use obsidian_sync_core::service::Trigger;
use std::sync::{Arc, Mutex};

pub type SharedTrigger = Arc<Mutex<Option<Trigger>>>;

/// 앱 아이콘 RGBA 생성 (보라/인디고 그라데이션 원형 + 크리스탈/문서 모티프 링)
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((size * size * 4) as usize);
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - c) / c;
            let dy = (y as f32 - c) / c;
            let d = (dx * dx + dy * dy).sqrt();
            let px = if d <= 1.0 {
                if (0.44..=0.64).contains(&d) {
                    [255, 255, 255, 255]
                } else {
                    // 상단 인디고(99, 102, 241) ~ 하단 퍼플(124, 58, 237) 그라데이션
                    let t = ((dy + 1.0) * 0.5).clamp(0.0, 1.0);
                    let r = (99.0 + (124.0 - 99.0) * t) as u8;
                    let g = (102.0 + (58.0 - 102.0) * t) as u8;
                    let b = (241.0 + (237.0 - 241.0) * t) as u8;
                    [r, g, b, 255]
                }
            } else {
                [0, 0, 0, 0]
            };
            v.extend_from_slice(&px);
        }
    }
    v
}

#[cfg(windows)]
mod imp {
    use super::SharedTrigger;
    use std::path::PathBuf;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOW,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "ObsidianSync";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn show_window(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as *mut core::ffi::c_void;
            ShowWindow(h, SW_SHOW);
            ShowWindow(h, SW_RESTORE);
            SetForegroundWindow(h);
        }
    }

    pub fn hide_window(hwnd: isize) -> bool {
        if hwnd == 0 {
            return false;
        }
        unsafe {
            ShowWindow(hwnd as *mut core::ffi::c_void, SW_HIDE);
        }
        true
    }

    /// 이미 실행 중이면 기존 창을 앞으로 가져오고 `false` 반환
    pub fn single_instance() -> bool {
        let name = wide("Local\\ObsidianSyncSingleInstance");
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if h.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
                let title = wide("Obsidian Sync");
                let existing = FindWindowW(std::ptr::null(), title.as_ptr());
                if !existing.is_null() {
                    show_window(existing as isize);
                }
                return false;
            }
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
            .with_tooltip("Obsidian Sync")
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

    pub type Tray = TrayIcon;
}

#[cfg(not(windows))]
mod imp {
    use super::SharedTrigger;
    use std::path::PathBuf;

    pub type Tray = ();

    #[allow(dead_code)]
    pub fn show_window(_: isize) {}

    #[allow(dead_code)]
    pub fn hide_window(_: isize) -> bool {
        false
    }

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
}

pub use imp::*;
