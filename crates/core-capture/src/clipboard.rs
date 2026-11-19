//! 工业级划选剪贴板捕获机制：
//! 1. 彻底释放物理 Alt/Shift/Win 等修饰键，根除 Alt+Q 组合键残留导致的复制失效；
//! 2. 模拟真实硬件扫描码的纯净 Ctrl+C，兼容 Chromium/Electron/Office/PDF/IDE；
//! 3. 采用 Windows 内核金标准 `GetClipboardSequenceNumber` 侦听剪贴板真实写入，零锁竞争；
//! 4. 对终端类应用（cmd, powershell, wt）严格禁用模拟按键，避免发送中断信号。

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
fn release_modifier_keys() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
        VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    };

    let keys_to_check: [VIRTUAL_KEY; 11] = [
        VK_MENU, VK_LMENU, VK_RMENU,
        VK_SHIFT, VK_LSHIFT, VK_RSHIFT,
        VK_CONTROL, VK_LCONTROL, VK_RCONTROL,
        VK_LWIN, VK_RWIN,
    ];

    let mut release_inputs = Vec::new();
    for &vk in &keys_to_check {
        let state = unsafe { GetAsyncKeyState(vk.0 as i32) };
        if (state as u16 & 0x8000) != 0 {
            release_inputs.push(INPUT {
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

    if !release_inputs.is_empty() {
        unsafe {
            let _ = SendInput(&release_inputs, std::mem::size_of::<INPUT>() as i32);
        }
        // 微小让步，等待前台目标窗口消息队列完成释放并脱离 Alt 菜单模式
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(not(windows))]
fn release_modifier_keys() {}

#[cfg(windows)]
fn send_ctrl_c() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_C, VK_CONTROL,
    };

    let vkey_ctrl = VK_CONTROL;
    let vkey_c = VK_C;
    let scan_ctrl = unsafe { MapVirtualKeyW(vkey_ctrl.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    let scan_c = unsafe { MapVirtualKeyW(vkey_c.0 as u32, MAPVK_VK_TO_VSC) } as u16;

    let make_key = |vk: VIRTUAL_KEY, scan: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    // 1. Ctrl 键按下
    // 2. C 键按下
    let down_inputs = [
        make_key(vkey_ctrl, scan_ctrl, false),
        make_key(vkey_c, scan_c, false),
    ];
    unsafe {
        let _ = SendInput(&down_inputs, std::mem::size_of::<INPUT>() as i32);
    }

    // 维持按下 15ms，防止按键由于时间过短被系统消息循环吞掉或合并
    std::thread::sleep(Duration::from_millis(15));

    // 3. C 键抬起
    // 4. Ctrl 键抬起
    let up_inputs = [
        make_key(vkey_c, scan_c, true),
        make_key(vkey_ctrl, scan_ctrl, true),
    ];
    unsafe {
        let _ = SendInput(&up_inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(windows))]
fn send_ctrl_c() {}

pub fn capture_via_clipboard_fallback() -> Result<Option<String>> {
    if is_foreground_terminal() {
        tracing::info!("前台窗口为终端应用，已跳过模拟 Ctrl+C 兜底");
        return Ok(None);
    }

    // 1. 强制释放物理 Alt 及其他修饰键，解决 Alt+Q 组合键残留
    release_modifier_keys();

    // 2. 记录当前系统剪贴板序列号
    #[cfg(windows)]
    let initial_seq = unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() };

    // 3. 发送纯净硬件扫描码 Ctrl+C
    send_ctrl_c();

    // 4. 使用 Windows 剪贴板序列号智能等待目标应用写入（最大 450ms，每次 15ms，内核原子 0 开销）
    #[cfg(windows)]
    {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(450);
        let mut seq_changed = false;

        while start.elapsed() < timeout {
            let current_seq = unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() };
            if current_seq != initial_seq {
                seq_changed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }

        if seq_changed {
            // 目标程序已明确完成写入，给 10ms 确保前台应用完成 CloseClipboard
            std::thread::sleep(Duration::from_millis(10));
            // 带有重试的读取（防止极短时间的锁竞争）
            for _ in 0..5 {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    if let Ok(text) = cb.get_text() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Ok(Some(text));
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    // 5. 兜底方案：如果序列号未变动，尝试直接读取一次
    if let Ok(mut cb) = arboard::Clipboard::new() {
        if let Ok(text) = cb.get_text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(Some(text));
            }
        }
    }

    Ok(None)
}
