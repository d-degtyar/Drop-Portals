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
