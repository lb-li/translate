pub mod providers;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub text: String,
    pub target_lang: String,
}

#[derive(Debug, Clone)]
pub enum TranslateChunk {
    Delta(String),
    Done,
    Error(String),
}

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn translate_stream(
        &self,
        req: TranslateRequest,
        tx: mpsc::UnboundedSender<TranslateChunk>,
    ) -> Result<()>;
}

pub struct DemoLocalProvider;

#[async_trait]
impl TranslationProvider for DemoLocalProvider {
    fn name(&self) -> &'static str {
        "Demo Offline Echo"
    }

    async fn translate_stream(
        &self,
        req: TranslateRequest,
        tx: mpsc::UnboundedSender<TranslateChunk>,
    ) -> Result<()> {
        let preview = format!(
            "[未配置 LUMEN_API_KEY，当前为本地演示模式]\n目标语种：{}\n已捕获清洗文本：\n{}",
            req.target_lang, req.text
        );
        let _ = tx.send(TranslateChunk::Delta(preview));
        let _ = tx.send(TranslateChunk::Done);
        Ok(())
    }
}
