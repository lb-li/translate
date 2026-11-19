use crate::{TranslateChunk, TranslateRequest, TranslationProvider};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

/// 免 Key 智能双通道极速翻译 Provider（Google 开放通道 + 国内直连免翻墙神经翻译容灾）。
pub struct GoogleTranslateFreeProvider {
    client: reqwest::Client,
}

impl GoogleTranslateFreeProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(6))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }

    /// 检测文本是否主要为中文。
    fn contains_chinese(text: &str) -> bool {
        text.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
    }

    /// 精准嗅探源文本语言代码（避免 API 不支持 auto 参数时报错）。
    fn detect_source_lang(text: &str) -> &'static str {
        if Self::contains_chinese(text) {
            "zh-CN"
        } else if text.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)) {
            "ja"
        } else if text.chars().any(|c| ('\u{ac00}'..='\u{d7af}').contains(&c)) {
            "ko"
        } else {
            // 英文、技术词汇、代码符号等默认源语言为英文
            "en"
        }
    }

    /// 规范化目标语言代码：
    /// 如果原文已是中文，自动反向翻译为英文；否则翻译为简体中文（zh-CN）。
    fn resolve_target_lang(source_text: &str, target_hint: &str) -> &'static str {
        if Self::contains_chinese(source_text) {
            "en"
        } else {
            let hint = target_hint.trim().to_lowercase();
            if hint.contains("en") || hint.contains("英") {
                "en"
            } else if hint.contains("ja") || hint.contains("日") {
                "ja"
            } else if hint.contains("ko") || hint.contains("韩") {
                "ko"
            } else {
                "zh-CN"
            }
        }
    }

    /// 尝试通过 Google GTX 开放接口翻译（超时限制为 2 秒，避免国内直连阻塞）。
    async fn try_google_translate(&self, text: &str, target_lang: &str) -> Result<String> {
        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}",
            target_lang,
            urlencoding::encode(text)
        );

        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_millis(2000))
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            )
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow!("Google 接口返回状态码: {}", resp.status()));
        }

        let body_json: serde_json::Value = resp.json().await?;
        let mut full_translated = String::new();
        if let Some(sentences) = body_json.get(0).and_then(|v| v.as_array()) {
            for sentence in sentences {
                if let Some(trans_part) = sentence.get(0).and_then(|s| s.as_str()) {
                    full_translated.push_str(trans_part);
                }
            }
        }

        if full_translated.trim().is_empty() {
            return Err(anyhow!("未能解析到有效译文"));
        }

        Ok(full_translated)
    }

    /// 国内免翻墙源 1：有道词典快速查询（适用于单词、技术词汇与短语，国内毫秒级直连）。
    async fn try_youdao_suggest(&self, text: &str) -> Result<String> {
        let trimmed = text.trim();
        // 词典 suggest 适合较短的查询
        if trimmed.split_whitespace().count() > 6 || trimmed.len() > 60 {
            return Err(anyhow!("文本过长，建议使用神经机器翻译"));
        }

        let url = format!(
            "https://dict.youdao.com/suggest?num=1&doctype=json&q={}",
            urlencoding::encode(trimmed)
        );

        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await?;

        let body_json: serde_json::Value = resp.json().await?;
        if let Some(entries) = body_json.pointer("/data/entries").and_then(|v| v.as_array()) {
            if let Some(first) = entries.first() {
                if let Some(explain) = first.get("explain").and_then(|s| s.as_str()) {
                    if !explain.trim().is_empty() {
                        return Ok(explain.to_string());
                    }
                }
            }
        }

        Err(anyhow!("有道词典未匹配到词条"))
    }

    /// 国内免翻墙源 2：MyMemory 免费神经机器翻译（适用于任意技术长句、代码注释与复合语句，国内直连畅通）。
    async fn try_mymemory_translate(&self, text: &str, target_lang: &str) -> Result<String> {
        let detected_src = Self::detect_source_lang(text);
        let (src, tgt) = if target_lang == "en" {
            (
                if detected_src == "en" { "zh-CN" } else { detected_src },
                "en",
            )
        } else {
            (
                detected_src,
                if target_lang == "zh-CN" { "zh-CN" } else { target_lang },
            )
        };

        // 若源语种与目标语种相同，自动反转
        let (src, tgt) = if src == tgt {
            if src == "zh-CN" {
                ("zh-CN", "en")
            } else {
                (src, "zh-CN")
            }
        } else {
            (src, tgt)
        };

        let url = format!(
            "https://api.mymemory.translated.net/get?q={}&langpair={}|{}",
            urlencoding::encode(text),
            src,
            tgt
        );

        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        let body_json: serde_json::Value = resp.json().await?;

        // 严格检验 responseStatus 必须为 200，杜绝非法的 'AUTO' 参数或限流错误被误当做翻译结果
        let status_ok = match body_json.get("responseStatus") {
            Some(v) => v.as_i64() == Some(200) || v.as_str() == Some("200"),
            None => false,
        };

        if !status_ok {
            let details = body_json
                .get("responseDetails")
                .and_then(|v| v.as_str())
                .unwrap_or("接口状态异常");
            return Err(anyhow!("MyMemory 服务响应异常: {}", details));
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
}

impl Default for GoogleTranslateFreeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TranslationProvider for GoogleTranslateFreeProvider {
    fn name(&self) -> &'static str {
        "智能免费翻译 (免Key·免翻墙)"
    }

    async fn translate_stream(
        &self,
        req: TranslateRequest,
        tx: mpsc::UnboundedSender<TranslateChunk>,
    ) -> Result<()> {
        let target_lang = Self::resolve_target_lang(&req.text, &req.target_lang);

        // 1. 优先尝试 Google 开放通道（若有代理或海外直连，极速返回）
        match self.try_google_translate(&req.text, target_lang).await {
            Ok(result) => {
                let _ = tx.send(TranslateChunk::Delta(result));
                let _ = tx.send(TranslateChunk::Done);
                return Ok(());
            }
            Err(e) => {
                tracing::info!(%e, "Google 通道直连不可达，无缝切换至国内免翻墙高速容灾通道");
            }
        }

        // 2. 国内免翻墙回退通道：短词优先查有道词典，获得最精准专业释义
        if let Ok(dict_result) = self.try_youdao_suggest(&req.text).await {
            let _ = tx.send(TranslateChunk::Delta(dict_result));
            let _ = tx.send(TranslateChunk::Done);
            return Ok(());
        }

        // 3. 通用免翻墙回退通道：长句与复合文本使用 MyMemory 神经机器翻译
        match self.try_mymemory_translate(&req.text, target_lang).await {
            Ok(result) => {
                let _ = tx.send(TranslateChunk::Delta(result));
                let _ = tx.send(TranslateChunk::Done);
                Ok(())
            }
            Err(err) => {
                let msg = format!("翻译请求失败（已尝试 Google 及国内免翻墙容灾通道）: {}", err);
                let _ = tx.send(TranslateChunk::Error(msg.clone()));
                Err(anyhow!(msg))
            }
        }
    }
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
                _ => {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_target_lang() {
        assert_eq!(
            GoogleTranslateFreeProvider::resolve_target_lang("Hello world", "中文"),
            "zh-CN"
        );
        assert_eq!(
            GoogleTranslateFreeProvider::resolve_target_lang("你好世界", "中文"),
            "en"
        );
        assert_eq!(
            GoogleTranslateFreeProvider::resolve_target_lang("Hello", "日语"),
            "ja"
        );
    }

    #[tokio::test]
    async fn test_fallback_channels_live() {
        let provider = GoogleTranslateFreeProvider::new();
        // 测试国内免翻墙源（有道词典）
        let youdao_res = provider.try_youdao_suggest("WindowOptions").await;
        assert!(youdao_res.is_ok(), "有道词典应返回词条");
        let text = youdao_res.unwrap();
        assert!(text.contains("窗口") || text.contains("选项"));

        // 测试国内免翻墙源（MyMemory 神经翻译）
        let mymemory_res = provider.try_mymemory_translate("Handling resize", "zh-CN").await;
        assert!(mymemory_res.is_ok(), "MyMemory 应成功返回译文");
    }
}
