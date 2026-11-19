pub mod cleaner;
pub mod clipboard;
pub mod uia;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct CaptureResult {
    pub raw_text: String,
    pub cleaned_text: String,
    pub source: CaptureSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSource {
    WindowsUia,
    ClipboardFallback,
}

pub fn capture_selection() -> Result<Option<CaptureResult>> {
    // 1. 剪贴板快速模拟读取（对终端已做隔离，全面覆盖 Chrome/Edge/VS Code/PDF/Office）
    let clip_text = std::panic::catch_unwind(|| clipboard::capture_via_clipboard_fallback())
        .unwrap_or_else(|_| Ok(None))?;
    if let Some(raw) = clip_text {
        let cleaned = cleaner::clean_selected_text(&raw);
        if !cleaned.is_empty() {
            return Ok(Some(CaptureResult {
                raw_text: raw,
                cleaned_text: cleaned,
                source: CaptureSource::ClipboardFallback,
            }));
        }
    }

    // 2. 剪贴板未命中或终端环境中，使用 Windows 原生 UI Automation 选区兜底
    let uia_text = std::panic::catch_unwind(|| uia::try_get_uia_selection()).unwrap_or(None);
    if let Some(raw) = uia_text {
        let cleaned = cleaner::clean_selected_text(&raw);
        if !cleaned.is_empty() {
            return Ok(Some(CaptureResult {
                raw_text: raw,
                cleaned_text: cleaned,
                source: CaptureSource::WindowsUia,
            }));
        }
    }

    Ok(None)
}
