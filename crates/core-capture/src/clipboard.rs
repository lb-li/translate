use anyhow::Result;

#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
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
        let mut buf = [0u16; 1024];
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
        TERMINAL_BLACKLIST
            .iter()
            .any(|name| full_path.ends_with(name))
    }
}

#[cfg(not(windows))]
pub fn is_foreground_terminal() -> bool {
    false
}

#[cfg(windows)]
fn release_alt_keys() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        VK_LMENU, VK_MENU, VK_RMENU,
    };

    let mut inputs = Vec::new();
    for vk in [VK_MENU, VK_LMENU, VK_RMENU] {
        let state = unsafe { GetAsyncKeyState(vk.0 as i32) };
        if (state as u16 & 0x8000) != 0 {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    if !inputs.is_empty() {
        unsafe {
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(windows)]
fn send_ctrl_c() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_C, VK_CONTROL,
    };

    let scan_ctrl = unsafe { MapVirtualKeyW(VK_CONTROL.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    let scan_c = unsafe { MapVirtualKeyW(VK_C.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    let make_key = |scan: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: if up {
                    KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_SCANCODE
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    unsafe {
        let _ = SendInput(
            &[make_key(scan_ctrl, false), make_key(scan_c, false)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
    std::thread::sleep(Duration::from_millis(15));
    unsafe {
        let _ = SendInput(
            &[make_key(scan_c, true), make_key(scan_ctrl, true)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

#[cfg(windows)]
fn restore_clipboard_text(original: Option<String>) {
    if let Some(text) = original {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }
}

pub fn capture_via_clipboard_fallback() -> Result<Option<String>> {
    #[cfg(not(windows))]
    {
        Ok(None)
    }

    #[cfg(windows)]
    {
        use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

        if is_foreground_terminal() {
            return Ok(None);
        }
        let original_text = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .ok();
        release_alt_keys();
        let initial_seq = unsafe { GetClipboardSequenceNumber() };
        send_ctrl_c();

        let start = std::time::Instant::now();
        let mut changed = false;
        while start.elapsed() < Duration::from_millis(450) {
            if unsafe { GetClipboardSequenceNumber() } != initial_seq {
                changed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        if !changed {
            return Ok(None);
        }

        std::thread::sleep(Duration::from_millis(10));
        let mut captured = None;
        for _ in 0..5 {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(text) = clipboard.get_text() {
                    if !text.trim().is_empty() {
                        captured = Some(text);
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        restore_clipboard_text(original_text);
        Ok(captured)
    }
}
