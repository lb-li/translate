pub mod native_win;

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsTier {
    SystemNative,
    OfflineNeural,
    CloudStreaming,
}

pub struct HybridTtsEngine {
    native: native_win::NativeWinTts,
}

impl HybridTtsEngine {
    pub fn new() -> Result<Self> {
        Ok(Self {
            native: native_win::NativeWinTts::new()?,
        })
    }

    pub fn speak(&self, text: &str, slow_mode: bool, tier: TtsTier) -> Result<()> {
        let rate = if slow_mode { 0.75 } else { 1.0 };
        match tier {
            TtsTier::SystemNative => self.native.speak(text, rate),
            TtsTier::OfflineNeural | TtsTier::CloudStreaming => {
                tracing::info!(?tier, "当前自动使用 L1 系统原生语音通道");
                self.native.speak(text, rate)
            }
        }
    }

    pub fn stop(&self) -> Result<()> {
        self.native.stop()
    }

    pub fn is_speaking(&self) -> bool {
        self.native.is_speaking()
    }
}
