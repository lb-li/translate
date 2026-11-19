//! Windows 多屏 DPI 感知光标跟随定位、屏幕边界防裁切、不抢焦点弹出与隐藏至托盘。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopoverRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// 纯几何边界防裁切计算（支持单元测试）：
/// 默认定位在光标右下方 (cursor_x + 12, cursor_y + 18)；
/// 若超出工作区下沿则翻转至光标上方；水平方向自动贴边留出 8px 边距。
pub fn clamp_to_work_area(
    cursor_x: i32,
    cursor_y: i32,
    popup_w: i32,
    popup_h: i32,
    work_left: i32,
    work_top: i32,
    work_right: i32,
    work_bottom: i32,
) -> PopoverRect {
    let margin = 8;
    let offset_x = 12;
    let offset_y = 18;

    let mut x = cursor_x + offset_x;
    if x + popup_w + margin > work_right {
        x = (work_right - popup_w - margin).max(work_left + margin);
    }
    if x < work_left + margin {
        x = work_left + margin;
    }

    let mut y = cursor_y + offset_y;
    if y + popup_h + margin > work_bottom {
        // 靠底边时翻转至光标上方，避免遮挡任务栏或被裁切
        y = cursor_y - popup_h - offset_y;
    }
    if y < work_top + margin {
        y = work_top + margin;
    }

    PopoverRect {
        x,
        y,
        width: popup_w,
        height: popup_h,
    }
}

#[cfg(windows)]
pub fn find_window_by_title(title: &str) -> Option<isize> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) }.ok()?;
    if hwnd.0.is_null() {
        None
    } else {
        Some(hwnd.0 as isize)
    }
}

#[cfg(not(windows))]
pub fn find_window_by_title(_title: &str) -> Option<isize> {
    None
}

#[cfg(windows)]
pub fn apply_popover_ex_styles(hwnd_raw: isize) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST,
    };

    let hwnd = HWND(hwnd_raw as *mut c_void);
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_ex = ex
            | (WS_EX_TOOLWINDOW.0 as isize)
            | (WS_EX_TOPMOST.0 as isize)
            | (WS_EX_NOACTIVATE.0 as isize);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
    }
}

#[cfg(not(windows))]
pub fn apply_popover_ex_styles(_hwnd_raw: isize) {}

#[cfg(windows)]
pub fn get_window_scale_factor(hwnd: windows::Win32::Foundation::HWND) -> f32 {
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

#[cfg(windows)]
pub fn show_popover_near_cursor(hwnd_raw: isize, logical_w: i32, logical_h: i32) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    };

    let hwnd = HWND(hwnd_raw as *mut c_void);
    let scale = get_window_scale_factor(hwnd);
    let phys_w = (logical_w as f32 * scale).round() as i32;
    let phys_h = (logical_h as f32 * scale).round() as i32;

    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_err() {
        return;
    }

    let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    let rect = if unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
        let work = info.rcWork;
        clamp_to_work_area(
            pt.x,
            pt.y,
            phys_w,
            phys_h,
            work.left,
            work.top,
            work.right,
            work.bottom,
        )
    } else {
        PopoverRect {
            x: pt.x + 12,
            y: pt.y + 18,
            width: phys_w,
            height: phys_h,
        }
    };

    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

#[cfg(not(windows))]
pub fn show_popover_near_cursor(_hwnd_raw: isize, _logical_w: i32, _logical_h: i32) {}

#[cfg(windows)]
pub fn hide_popover_to_tray(hwnd_raw: isize) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_HIDE};

    let hwnd = HWND(hwnd_raw as *mut c_void);
    unsafe {
        let _ = ShowWindowAsync(hwnd, SW_HIDE);
    }
}

#[cfg(not(windows))]
pub fn hide_popover_to_tray(_hwnd_raw: isize) {}

#[cfg(windows)]
pub fn is_popover_visible(hwnd_raw: isize) -> bool {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;

    let hwnd = HWND(hwnd_raw as *mut c_void);
    unsafe {
        IsWindowVisible(hwnd).as_bool()
    }
}

#[cfg(not(windows))]
pub fn is_popover_visible(_hwnd_raw: isize) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_above_cursor_near_bottom_edge() {
        let rect = clamp_to_work_area(1000, 1020, 420, 280, 0, 0, 1920, 1040);
        assert!(rect.y < 1020);
        assert_eq!(rect.y, 1020 - 280 - 18);
    }

    #[test]
    fn clamps_right_edge_cleanly() {
        let rect = clamp_to_work_area(1850, 200, 420, 280, 0, 0, 1920, 1040);
        assert_eq!(rect.x, 1920 - 420 - 8);
    }
}
