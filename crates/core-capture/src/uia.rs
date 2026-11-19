//! Windows 原生 UI Automation 选区读取（零剪贴板副作用，耗时通常 2-5ms）。

#[cfg(windows)]
pub fn try_get_uia_selection() -> Option<String> {
    use uiautomation::patterns::UITextPattern;
    use uiautomation::UIAutomation;

    let automation = UIAutomation::new().ok()?;
    let focused = automation.get_focused_element().ok()?;
    let text_pattern = focused.get_pattern::<UITextPattern>().ok()?;
    let ranges = text_pattern.get_selection().ok()?;

    let mut combined = String::new();
    for range in ranges {
        if let Ok(text) = range.get_text(-1) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(trimmed);
            }
        }
    }

    if combined.is_empty() { None } else { Some(combined) }
}

#[cfg(not(windows))]
pub fn try_get_uia_selection() -> Option<String> {
    None
}
