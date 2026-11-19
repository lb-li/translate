pub fn clean_selected_text(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalized.len());
    let lines: Vec<&str> = normalized.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !out.ends_with("\n\n") && !out.is_empty() {
                out.push_str("\n\n");
            }
            continue;
        }
        let is_last = idx + 1 == lines.len();
        let next_is_empty = !is_last && lines[idx + 1].trim().is_empty();
        if !is_last && !next_is_empty && trimmed.ends_with('-') && trimmed.len() > 1 {
            out.push_str(&trimmed[..trimmed.len() - 1]);
            continue;
        }
        out.push_str(trimmed);
        if !is_last && !next_is_empty {
            let last_is_cjk = trimmed.chars().last().map(is_cjk).unwrap_or(false);
            let next_is_cjk = lines[idx + 1]
                .trim_start()
                .chars()
                .next()
                .map(is_cjk)
                .unwrap_or(false);
            if !(last_is_cjk && next_is_cjk) {
                out.push(' ');
            }
        }
    }
    out.trim().to_string()
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_pdf_hyphenated_lines() {
        let raw = "High-perfor-\nmance desktop\ntranslation.";
        assert_eq!(
            clean_selected_text(raw),
            "High-performance desktop translation."
        );
    }

    #[test]
    fn joins_cjk_lines_without_extra_spaces() {
        let raw = "高性能划词\n翻译工具";
        assert_eq!(clean_selected_text(raw), "高性能划词翻译工具");
    }

    #[test]
    fn preserves_paragraph_boundaries() {
        let raw = "第一段\n\n第二段";
        assert_eq!(clean_selected_text(raw), "第一段\n\n第二段");
    }

    #[test]
    fn normalizes_windows_newlines() {
        assert_eq!(clean_selected_text("hello\r\nworld"), "hello world");
    }
}
