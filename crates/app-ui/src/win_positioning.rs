use std::sync::atomic::{AtomicIsize, Ordering};

static POPOVER_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn register_popover_window(hwnd: isize) {
    POPOVER_HWND.store(hwnd, Ordering::Release);
}

pub fn popover_window(title: &str) -> Option<isize> {
    let cached = POPOVER_HWND.load(Ordering::Acquire);
    if cached != 0 {
        return Some(cached);
    }
    let hwnd = find_window_by_title(title)?;
    register_popover_window(hwnd);
    Some(hwnd)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopoverRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub fn clamp_to_work_area(
    cursor_x: i32,
    cursor_y: i32,
    popup_w: i32,
    popup_h: i32,
    work: WorkArea,
) -> PopoverRect {
    let margin = 8;
    let offset_x = 12;
    let offset_y = 18;
    let mut x = cursor_x + offset_x;
    if x + popup_w + margin > work.right {
        x = (work.right - popup_w - margin).max(work.left + margin);
    }
    if x < work.left + margin {
        x = work.left + margin;
    }
    let mut y = cursor_y + offset_y;
    if y + popup_h + margin > work.bottom {
        y = cursor_y - popup_h - offset_y;
    }
    if y < work.top + margin {
        y = work.top + margin;
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
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    };

    let hwnd = HWND(hwnd_raw as *mut c_void);
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_ex = ex | (WS_EX_TOOLWINDOW.0 as isize) | (WS_EX_TOPMOST.0 as isize);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
    }
}

#[cfg(not(windows))]
pub fn apply_popover_ex_styles(_hwnd_raw: isize) {}

#[cfg(windows)]
pub fn show_popover_near_cursor(hwnd_raw: isize, logical_w: i32, logical_h: i32) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, SetForegroundWindow, SetWindowPos, HWND_TOPMOST, SWP_SHOWWINDOW,
    };

    let hwnd = HWND(hwnd_raw as *mut c_void);
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_err() {
        return;
    }
    let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    let scale = if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
        .is_ok()
    {
        dpi_x as f32 / 96.0
    } else {
        1.0
    };
    let phys_w = (logical_w as f32 * scale).round() as i32;
    let phys_h = (logical_h as f32 * scale).round() as i32;
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
            WorkArea {
                left: work.left,
                top: work.top,
                right: work.right,
                bottom: work.bottom,
            },
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
            SWP_SHOWWINDOW,
        );
        let _ = SetForegroundWindow(hwnd);
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
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

#[cfg(not(windows))]
pub fn is_popover_visible(_hwnd_raw: isize) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_work_area() -> WorkArea {
        WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }
    }

    #[test]
    fn flips_above_cursor_near_bottom_edge() {
        let rect = clamp_to_work_area(1000, 1020, 420, 280, test_work_area());
        assert_eq!(rect.y, 1020 - 280 - 18);
    }

    #[test]
    fn clamps_right_edge_cleanly() {
        let rect = clamp_to_work_area(1850, 200, 420, 280, test_work_area());
        assert_eq!(rect.x, 1920 - 420 - 8);
    }

    #[test]
    fn respects_left_work_area_margin() {
        let rect = clamp_to_work_area(0, 100, 420, 280, test_work_area());
        assert_eq!(rect.x, 20);
    }
}
