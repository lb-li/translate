use crate::{resolve_target_language, TranslateChunk, TranslateRequest, TranslationProvider};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct OpenAiCompatProvider {
    client: reqwest::Client,
    api_base: String,
    api_key: String,
    model: String,
}

impl OpenAiCompatProvider {
    pub fn new(
        api_base: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .pool_idle_timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(6))
            .build()?;
        Ok(Self {
            client,
            api_base: api_base.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            model: model.into(),
        })
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("LUMEN_API_KEY").ok()?;
        let api_base = std::env::var("LUMEN_API_BASE")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let model = std::env::var("LUMEN_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        Self::new(api_base, api_key, model).ok()
    }
}

#[derive(Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[async_trait]
impl TranslationProvider for OpenAiCompatProvider {
    fn name(&self) -> &'static str {
        "OpenAI-Compat LLM"
    }

    fn cache_namespace(&self) -> String {
        format!("openai:{}:{}", self.api_base, self.model)
    }

    async fn translate_stream(
        &self,
        req: TranslateRequest,
        tx: mpsc::UnboundedSender<TranslateChunk>,
    ) -> Result<()> {
        let url = format!("{}/chat/completions", self.api_base);
        let target_lang = resolve_target_language(&req.text, &req.target_lang);
        let system_prompt = format!(
            "你是一个高精度桌面划词翻译助手。将用户输入准确、自然地翻译为{target_lang}。只输出译文，不要寒暄或多余解释；若为单个英文单词，简要给出音标、词性与常用释义。"
        );
        let body = serde_json::json!({
            "model": self.model,
            "stream": true,
            "temperature": 0.2,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": req.text }
            ]
        });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("翻译接口返回错误 {status}: {err_body}"));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = Vec::new();
        while let Some(item) = stream.next().await {
            buffer.extend_from_slice(&item?);
            while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
                let line = buffer.drain(..=pos).collect::<Vec<_>>();
                let line = String::from_utf8_lossy(&line);
                let Some(payload) = line.trim().strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim_start();
                if payload == "[DONE]" {
                    let _ = tx.send(TranslateChunk::Done);
                    return Ok(());
                }
                if let Ok(parsed) = serde_json::from_str::<StreamResponse>(payload) {
                    if let Some(delta) =
                        parsed.choices.first().and_then(|c| c.delta.content.clone())
                    {
                        if !delta.is_empty() {
                            let _ = tx.send(TranslateChunk::Delta(delta));
                        }
                    }
                }
            }
        }
        let _ = tx.send(TranslateChunk::Done);
        Ok(())
    }
}
