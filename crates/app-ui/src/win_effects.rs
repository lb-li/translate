//! Windows 11 原生 DWM 毛玻璃材质 (Acrylic / Mica) 与圆角桥接。

#[cfg(windows)]
pub fn apply_win11_popover_backdrop(hwnd_raw: isize, dark_mode: bool) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    let hwnd = HWND(hwnd_raw as *mut c_void);

    unsafe {
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&corner as *const _) as *const c_void,
            std::mem::size_of_val(&corner) as u32,
        );

        // Acrylic 瞬态浮窗材质 (DWMSBT_TRANSIENTWINDOW = 3)
        let backdrop_type: i32 = 3;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&backdrop_type as *const _) as *const c_void,
            std::mem::size_of_val(&backdrop_type) as u32,
        );

        let dark_val: i32 = if dark_mode { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark_val as *const _) as *const c_void,
            std::mem::size_of_val(&dark_val) as u32,
        );
    }
}

#[cfg(not(windows))]
pub fn apply_win11_popover_backdrop(_hwnd_raw: isize, _dark_mode: bool) {}
