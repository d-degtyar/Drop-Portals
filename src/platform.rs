//! Усе, що залежить від Windows: Провідник, діалог папки, клавіші, автозапуск.
//! На інших системах - прості заглушки, щоб код збирався і тестувався.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn open_folder(path: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program).arg(path).spawn();
}

#[cfg(windows)]
pub fn pick_folder(title: &str, start: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if !start.is_empty() && Path::new(start).is_dir() {
        dialog = dialog.set_directory(start);
    }
    dialog.pick_folder()
}

#[cfg(not(windows))]
pub fn pick_folder(_title: &str, _start: &str) -> Option<PathBuf> {
    None
}

/// Чи затиснутий Ctrl просто зараз. Під час перетягування з Провідника вікно
/// порталу не у фокусі, тож egui клавіш не бачить - питаємо Windows напряму.
#[cfg(windows)]
pub fn ctrl_pressed() -> Option<bool> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
    Some(unsafe { GetAsyncKeyState(VK_CONTROL as i32) } as u16 & 0x8000 != 0)
}

#[cfg(not(windows))]
pub fn ctrl_pressed() -> Option<bool> {
    None
}

/// Позиція курсора у фізичних пікселях.
#[cfg(windows)]
pub fn cursor_position() -> Option<(f32, f32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then_some((point.x as f32, point.y as f32))
}

#[cfg(not(windows))]
pub fn cursor_position() -> Option<(f32, f32)> {
    None
}

/// Межі всіх моніторів разом (x, y, ширина, висота) у фізичних пікселях.
#[cfg(windows)]
pub fn virtual_screen() -> Option<(f32, f32, f32, f32)> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };
    let metric = |index| unsafe { GetSystemMetrics(index) } as f32;
    let size = (metric(SM_CXVIRTUALSCREEN), metric(SM_CYVIRTUALSCREEN));
    (size.0 > 0.0 && size.1 > 0.0)
        .then(|| (metric(SM_XVIRTUALSCREEN), metric(SM_YVIRTUALSCREEN), size.0, size.1))
}

#[cfg(not(windows))]
pub fn virtual_screen() -> Option<(f32, f32, f32, f32)> {
    None
}

/// Робоча область монітора (без панелі задач), на якому лежить точка, у фізичних пікселях.
#[cfg(windows)]
pub fn work_area_at(x: f32, y: f32) -> Option<(f32, f32, f32, f32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    let point = POINT { x: x as i32, y: y as i32 };
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    (unsafe { GetMonitorInfoW(monitor, &mut info) } != 0).then(|| {
        let r = info.rcWork;
        (r.left as f32, r.top as f32, (r.right - r.left) as f32, (r.bottom - r.top) as f32)
    })
}

#[cfg(not(windows))]
pub fn work_area_at(_x: f32, _y: f32) -> Option<(f32, f32, f32, f32)> {
    None
}

/// Windows інколи «опускає» вікна, закріплені поверх усіх: напр., коли активується інша
/// програма, теж закріплена поверх. Тож раз на `interval` знову піднімаємо наші вікна.
/// Окремий потік, щоб не будити інтерфейс; на повноекранних відео й іграх нічого не робимо.
#[cfg(windows)]
pub fn keep_windows_on_top(interval: std::time::Duration) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            if !other_app_is_fullscreen() {
                raise_own_windows();
            }
        }
    });
}

#[cfg(not(windows))]
pub fn keep_windows_on_top(_interval: std::time::Duration) {}

#[cfg(windows)]
fn raise_own_windows() {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, HWND_TOPMOST, IsWindowVisible, SWP_ASYNCWINDOWPOS,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowPos,
    };

    unsafe extern "system" fn visit(hwnd: HWND, _: LPARAM) -> i32 {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 {
                // асинхронно: потік інтерфейсу застосує сам, без блокування
                let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS;
                SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
            }
        }
        1 // далі по списку вікон
    }
    unsafe { EnumWindows(Some(visit), 0) };
}

/// Чи займає активне вікно іншої програми весь монітор (відео, гра, презентація).
#[cfg(windows)]
fn other_app_is_fullscreen() -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetShellWindow, GetWindowRect, GetWindowThreadProcessId,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if hwnd.is_null() || pid == GetCurrentProcessId() || hwnd == GetShellWindow() {
            return false;
        }
        // робочий стіл теж на весь екран, але це не відео і не гра
        let mut class = [0u16; 16];
        let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        if matches!(String::from_utf16_lossy(&class[..len]).as_str(), "WorkerW" | "Progman") {
            return false;
        }
        let mut window: RECT = std::mem::zeroed();
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetWindowRect(hwnd, &mut window) == 0
            || GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) == 0
        {
            return false;
        }
        let screen = info.rcMonitor;
        window.left <= screen.left
            && window.top <= screen.top
            && window.right >= screen.right
            && window.bottom >= screen.bottom
    }
}

/// `false`, якщо програма вже запущена.
#[cfg(windows)]
pub fn single_instance() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "Local\\DropPortals.SingleInstance\0".encode_utf16().collect();
    // м'ютекс живе до кінця процесу - дескриптор навмисно не закриваємо
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    !handle.is_null() && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
}

#[cfg(not(windows))]
pub fn single_instance() -> bool {
    true
}

/// Шрифт з `C:\Windows\Fonts`, якщо є.
pub fn system_font(file: &str) -> Option<Vec<u8>> {
    if !cfg!(windows) {
        return None;
    }
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    std::fs::read(PathBuf::from(windir).join("Fonts").join(file)).ok()
}

/// Шрифт із символами (→ ✓ ✕ ↑ ↓ ↩): Segoe UI Symbol на Windows, DejaVu Sans деінде.
pub fn symbol_font() -> Option<Vec<u8>> {
    if cfg!(windows) {
        return system_font("seguisym.ttf");
    }
    ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/Library/Fonts/Arial Unicode.ttf"]
        .iter()
        .find_map(|path| std::fs::read(path).ok())
}

pub mod autostart {
    #[cfg(windows)]
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    #[cfg(windows)]
    const VALUE: &str = "DropPortals";

    #[cfg(windows)]
    pub fn is_enabled() -> bool {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(RUN_KEY)
            .and_then(|key| key.get_value::<String, _>(VALUE))
            .is_ok()
    }

    #[cfg(windows)]
    pub fn set_enabled(enabled: bool) -> std::io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
        if enabled {
            let exe = std::env::current_exe()?;
            key.set_value(VALUE, &format!("\"{}\"", exe.display()))
        } else {
            match key.delete_value(VALUE) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        }
    }

    #[cfg(not(windows))]
    pub fn is_enabled() -> bool {
        false
    }

    #[cfg(not(windows))]
    pub fn set_enabled(_enabled: bool) -> std::io::Result<()> {
        Ok(())
    }
}
