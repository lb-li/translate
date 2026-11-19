pub mod providers;

pub use providers::google_free::GoogleTranslateFreeProvider;
pub use providers::openai_compat::OpenAiCompatProvider;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub text: String,
    pub target_lang: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateChunk {
    Delta(String),
    Done,
    Error(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordInfo {
    pub phonetic: Option<String>,
    pub part_of_speech: Option<String>,
}

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn cache_namespace(&self) -> String {
        self.name().to_string()
    }

    async fn translate_stream(
        &self,
        req: TranslateRequest,
        tx: mpsc::UnboundedSender<TranslateChunk>,
    ) -> Result<()>;

    async fn word_info(&self, _text: &str) -> Result<Option<WordInfo>> {
        Ok(None)
    }
}

pub fn contains_chinese(text: &str) -> bool {
    text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

pub fn resolve_target_language(text: &str, target_hint: &str) -> &'static str {
    let hint = target_hint.trim().to_lowercase();
    if hint.contains("en") || hint.contains("英") {
        return "英语";
    }
    if hint.contains("ja") || hint.contains("日") {
        return "日语";
    }
    if hint.contains("ko") || hint.contains("韩") {
        return "韩语";
    }
    if hint.contains("zh") || hint.contains("中") {
        return "简体中文";
    }
    if contains_chinese(text) {
        "英语"
    } else {
        "简体中文"
    }
}

pub fn is_english_word(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '-' || c == '\'')
        && trimmed.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_language_reverses_chinese() {
        assert_eq!(resolve_target_language("你好", "auto"), "英语");
        assert_eq!(resolve_target_language("hello", "auto"), "简体中文");
        assert_eq!(resolve_target_language("你好", "中文"), "简体中文");
    }

    #[test]
    fn detects_single_english_words() {
        assert!(is_english_word("lumen"));
        assert!(is_english_word("state-of-the-art"));
        assert!(!is_english_word("two words"));
        assert!(!is_english_word("流明"));
    }
}
