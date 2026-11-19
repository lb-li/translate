use anyhow::{anyhow, Result};

pub struct NativeWinTts;

impl NativeWinTts {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn speak(&self, _text: &str, _rate_multiplier: f32) -> Result<()> {
        Err(anyhow!("当前平台不支持 Windows 原生语音"))
    }

    pub fn stop(&self) -> Result<()> {
        Ok(())
    }

    pub fn is_speaking(&self) -> bool {
        false
    }
}
