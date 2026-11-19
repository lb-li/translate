//! 剪贴板原子兜底：保存原剪贴板 -> 模拟 Ctrl+C -> 读取 -> 无损还原。
//! 对终端类应用严格禁用模拟 Ctrl+C，避免发送 SIGINT。

use anyhow::Result;
use std::time::Duration;

const TERMINAL_BLACKLIST: &[&str] = &[
    "windowsterminal.exe",
    "wt.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
    "conemu64.exe",
];

#[cfg(windows)]
pub fn is_foreground_terminal() -> bool {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return false;
        }
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return false;
        }
        let full_path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
        TERMINAL_BLACKLIST.iter().any(|name| full_path.ends_with(name))
    }
}

#[cfg(not(windows))]
pub fn is_foreground_terminal() -> bool {
    false
}

#[cfg(windows)]
fn send_ctrl_c() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C,
        VK_CONTROL,
    };

    let make_key = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let inputs = [
        make_key(VK_CONTROL.0, false),
        make_key(VK_C.0, false),
        make_key(VK_C.0, true),
        make_key(VK_CONTROL.0, true),
    ];

    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(windows))]
fn send_ctrl_c() {}

pub fn capture_via_clipboard_fallback() -> Result<Option<String>> {
    if is_foreground_terminal() {
        tracing::info!("前台窗口为终端应用，已跳过模拟 Ctrl+C 兜底");
        return Ok(None);
    }

    let mut clipboard = arboard::Clipboard::new()?;
    let backup_text = clipboard.get_text().ok();

    let _ = clipboard.clear();
    send_ctrl_c();

    std::thread::sleep(Duration::from_millis(45));
    let captured = clipboard.get_text().ok().filter(|s| !s.trim().is_empty());

    if let Some(prev) = backup_text {
        let _ = clipboard.set_text(prev);
    }

    Ok(captured)
}
