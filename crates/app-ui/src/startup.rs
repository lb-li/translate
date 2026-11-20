use anyhow::Result;

#[cfg(windows)]
pub fn set_launch_at_startup(enabled: bool) -> Result<()> {
    use anyhow::anyhow;
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_SZ,
    };

    let executable = std::env::current_exe()?;
    let value = format!("\"{}\"", executable.display());
    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let name: Vec<u16> = "LumenTranslate"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut key = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            Some(0),
            KEY_SET_VALUE,
            &mut key,
        );
        if status.0 != 0 {
            return Err(anyhow!("无法打开当前用户启动项: {status:?}"));
        }
        let result = if enabled {
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(key, PCWSTR(name.as_ptr()), Some(0), REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key, PCWSTR(name.as_ptr()))
        };
        let _ = RegCloseKey(key);
        if result.0 != 0 && !(!enabled && result.0 == 2) {
            return Err(anyhow!("无法更新当前用户启动项: {result:?}"));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_launch_at_startup(_enabled: bool) -> Result<()> {
    anyhow::bail!("开机自启动仅支持 Windows")
}
