use anyhow::{anyhow, Result};
use std::sync::Mutex;
use tts::Tts;

pub struct NativeWinTts {
    engine: Mutex<Tts>,
}

impl NativeWinTts {
    pub fn new() -> Result<Self> {
        let engine = Tts::default().map_err(|e| anyhow!("初始化系统 TTS 失败: {}", e))?;
        Ok(Self {
            engine: Mutex::new(engine),
        })
    }

    pub fn speak(&self, text: &str, rate_multiplier: f32) -> Result<()> {
        let mut engine = self.engine.lock().map_err(|_| anyhow!("TTS 引擎锁异常"))?;
        let target_rate = (engine.normal_rate() * rate_multiplier)
            .clamp(engine.min_rate(), engine.max_rate());
        let _ = engine.set_rate(target_rate);
        engine.speak(text, true).map_err(|e| anyhow!("朗读失败: {}", e))?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        let mut engine = self.engine.lock().map_err(|_| anyhow!("TTS 引擎锁异常"))?;
        let _ = engine.stop();
        Ok(())
    }
}
