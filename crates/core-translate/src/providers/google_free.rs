use crate::{
    contains_chinese, is_english_word, resolve_target_language, TranslateChunk, TranslateRequest,
    TranslationProvider, WordInfo,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct GoogleTranslateFreeProvider {
    client: reqwest::Client,
}

impl GoogleTranslateFreeProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }

    fn detect_source_lang(text: &str) -> &'static str {
        if contains_chinese(text) {
            "zh-CN"
        } else if text.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)) {
            "ja"
        } else if text.chars().any(|c| ('\u{ac00}'..='\u{d7af}').contains(&c)) {
            "ko"
        } else {
            "en"
        }
    }

    fn resolve_target_code(source_text: &str, target_hint: &str) -> &'static str {
        match resolve_target_language(source_text, target_hint) {
            "英语" => "en",
            "日语" => "ja",
            "韩语" => "ko",
            _ => "zh-CN",
        }
    }

    async fn try_google_chunk(&self, text: &str, target_lang: &str) -> Result<String> {
        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}",
            target_lang,
            urlencoding::encode(text)
        );
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_millis(2500))
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36",
            )
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("Google 接口返回状态码: {}", resp.status()));
        }
        let body_json: serde_json::Value = resp.json().await?;
        let mut translated = String::new();
        if let Some(sentences) = body_json.get(0).and_then(|v| v.as_array()) {
            for sentence in sentences {
                if let Some(part) = sentence.get(0).and_then(|s| s.as_str()) {
                    translated.push_str(part);
                }
            }
        }
        if translated.trim().is_empty() {
            return Err(anyhow!("未能解析到有效译文"));
        }
        Ok(translated)
    }

    async fn try_google_translate(&self, text: &str, target_lang: &str) -> Result<String> {
        let mut output = String::new();
        for chunk in split_text(text, 700) {
            let part = self.try_google_chunk(&chunk, target_lang).await?;
            append_segment(&mut output, &part, contains_chinese(text));
        }
        Ok(output)
    }

    async fn try_youdao_suggest(&self, text: &str) -> Result<String> {
        let trimmed = text.trim();
        if contains_chinese(trimmed) || trimmed.split_whitespace().count() > 6 || trimmed.len() > 60
        {
            return Err(anyhow!("当前文本不适合词典查询"));
        }
        let url = format!(
            "https://dict.youdao.com/suggest?num=1&doctype=json&q={}",
            urlencoding::encode(trimmed)
        );
        let body_json: serde_json::Value = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await?
            .json()
            .await?;
        if let Some(explain) = body_json
            .pointer("/data/entries/0/explain")
            .and_then(|v| v.as_str())
        {
            if !explain.trim().is_empty() {
                return Ok(explain.to_string());
            }
        }
        Err(anyhow!("有道词典未匹配到词条"))
    }

    async fn try_mymemory_chunk(&self, text: &str, target_lang: &str) -> Result<String> {
        let detected_src = Self::detect_source_lang(text);
        let (src, tgt) = (detected_src, target_lang);
        if src == tgt {
            return Ok(text.to_string());
        }
        let url = format!(
            "https://api.mymemory.translated.net/get?q={}&langpair={}%7C{}",
            urlencoding::encode(text),
            urlencoding::encode(src),
            urlencoding::encode(tgt)
        );
        let body_json: serde_json::Value = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .json()
            .await?;
        let status_ok = match body_json.get("responseStatus") {
            Some(v) => v.as_i64() == Some(200) || v.as_str() == Some("200"),
            None => false,
        };
        if !status_ok {
            let details = body_json
                .get("responseDetails")
                .and_then(|v| v.as_str())
                .unwrap_or("接口状态异常");
            return Err(anyhow!("MyMemory 服务响应异常: {details}"));
        }
        if let Some(translated) = body_json
            .pointer("/responseData/translatedText")
            .and_then(|s| s.as_str())
        {
            let trimmed = translated.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("'AUTO' IS AN INVALID") {
                return Ok(trimmed.to_string());
            }
        }
        Err(anyhow!("MyMemory 未返回有效翻译结果"))
    }

    async fn try_mymemory_translate(&self, text: &str, target_lang: &str) -> Result<String> {
        let mut output = String::new();
        for chunk in split_text(text, 400) {
            let part = self.try_mymemory_chunk(&chunk, target_lang).await?;
            append_segment(&mut output, &part, contains_chinese(text));
        }
        Ok(output)
    }
}

impl Default for GoogleTranslateFreeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TranslationProvider for GoogleTranslateFreeProvider {
    fn name(&self) -> &'static str {
        "智能免费翻译"
    }

    fn cache_namespace(&self) -> String {
        "free-v3".to_string()
    }

    async fn translate_stream(
        &self,
        req: TranslateRequest,
        tx: mpsc::UnboundedSender<TranslateChunk>,
    ) -> Result<()> {
        let target_lang = Self::resolve_target_code(&req.text, &req.target_lang);
        if Self::detect_source_lang(&req.text) == target_lang {
            let _ = tx.send(TranslateChunk::Delta(req.text));
            let _ = tx.send(TranslateChunk::Done);
            return Ok(());
        }
        if let Ok(result) = self.try_google_translate(&req.text, target_lang).await {
            let _ = tx.send(TranslateChunk::Delta(result));
            let _ = tx.send(TranslateChunk::Done);
            return Ok(());
        }
        if target_lang == "zh-CN" {
            if let Ok(result) = self.try_youdao_suggest(&req.text).await {
                let _ = tx.send(TranslateChunk::Delta(result));
                let _ = tx.send(TranslateChunk::Done);
                return Ok(());
            }
        }
        let result = self.try_mymemory_translate(&req.text, target_lang).await?;
        let _ = tx.send(TranslateChunk::Delta(result));
        let _ = tx.send(TranslateChunk::Done);
        Ok(())
    }

    async fn word_info(&self, text: &str) -> Result<Option<WordInfo>> {
        if !is_english_word(text) {
            return Ok(None);
        }
        let url = format!(
            "https://dict.youdao.com/jsonapi?q={}",
            urlencoding::encode(text.trim())
        );
        let body: serde_json::Value = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await?
            .json()
            .await?;
        let word = body.pointer("/ec/word/0");
        let phonetic = word
            .and_then(|w| w.get("usphone"))
            .and_then(|v| v.as_str())
            .or_else(|| word.and_then(|w| w.get("ukphone")).and_then(|v| v.as_str()))
            .map(|value| format!("/{value}/"));
        let part_of_speech = word
            .and_then(|w| w.pointer("/trs/0/pos"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if phonetic.is_none() && part_of_speech.is_none() {
            return Ok(None);
        }
        Ok(Some(WordInfo {
            phonetic,
            part_of_speech,
        }))
    }
}

fn append_segment(output: &mut String, segment: &str, source_is_chinese: bool) {
    if !output.is_empty() && !source_is_chinese {
        output.push(' ');
    }
    output.push_str(segment.trim());
}

fn split_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut remaining = text.trim();
    while remaining.chars().count() > max_chars {
        let hard_cut = remaining
            .char_indices()
            .nth(max_chars)
            .map(|(index, _)| index)
            .unwrap_or(remaining.len());
        let candidate = &remaining[..hard_cut];
        let cut = candidate
            .rfind('\n')
            .or_else(|| candidate.rfind(char::is_whitespace))
            .filter(|index| *index >= hard_cut / 2)
            .unwrap_or(hard_cut);
        let chunk = remaining[..cut].trim();
        if !chunk.is_empty() {
            chunks.push(chunk.to_string());
        }
        remaining = remaining[cut..].trim_start();
    }
    if !remaining.is_empty() {
        chunks.push(remaining.to_string());
    }
    chunks
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for b in s.as_bytes() {
            match *b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(*b as char);
                }
                b' ' => result.push_str("%20"),
                _ => result.push_str(&format!("%{b:02X}")),
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_translation_direction() {
        assert_eq!(
            GoogleTranslateFreeProvider::resolve_target_code("Hello world", "auto"),
            "zh-CN"
        );
        assert_eq!(
            GoogleTranslateFreeProvider::resolve_target_code("你好世界", "auto"),
            "en"
        );
        assert_eq!(
            GoogleTranslateFreeProvider::resolve_target_code("你好", "中文"),
            "zh-CN"
        );
    }

    #[test]
    fn chunks_long_text_at_utf8_boundaries() {
        let text = "高性能桌面翻译 ".repeat(150);
        let chunks = split_text(&text, 80);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 80));
        assert!(chunks.iter().all(|chunk| !chunk.is_empty()));
    }

    #[tokio::test]
    async fn same_language_short_circuits_without_network() {
        let provider = GoogleTranslateFreeProvider::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .translate_stream(
                TranslateRequest {
                    text: "你好".to_string(),
                    target_lang: "中文".to_string(),
                },
                tx,
            )
            .await
            .unwrap();
        assert_eq!(
            rx.recv().await,
            Some(TranslateChunk::Delta("你好".to_string()))
        );
        assert_eq!(rx.recv().await, Some(TranslateChunk::Done));
    }

    #[tokio::test]
    #[ignore = "需要真实网络，单独运行"]
    async fn live_fallback_channels() {
        let provider = GoogleTranslateFreeProvider::new();
        assert!(provider.try_youdao_suggest("lumen").await.is_ok());
        assert!(provider
            .try_mymemory_translate("Handling resize", "zh-CN")
            .await
            .is_ok());
    }
}
