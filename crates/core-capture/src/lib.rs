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
    if let Some(raw) = uia::try_get_uia_selection() {
        let cleaned = cleaner::clean_selected_text(&raw);
        if !cleaned.is_empty() {
            return Ok(Some(CaptureResult {
                raw_text: raw,
                cleaned_text: cleaned,
                source: CaptureSource::WindowsUia,
            }));
        }
    }

    if let Some(raw) = clipboard::capture_via_clipboard_fallback()? {
        let cleaned = cleaner::clean_selected_text(&raw);
        if !cleaned.is_empty() {
            return Ok(Some(CaptureResult {
                raw_text: raw,
                cleaned_text: cleaned,
                source: CaptureSource::ClipboardFallback,
            }));
        }
    }

    Ok(None)
}
