use anyhow::{anyhow, Result};
use std::sync::Mutex;
use tts::{Tts, Voice};

pub struct NativeWinTts {
    engine: Option<Mutex<Tts>>,
    zh_voice: Option<Voice>,
    en_voice: Option<Voice>,
}

impl NativeWinTts {
    pub fn new() -> Result<Self> {
        match Tts::default() {
            Ok(engine) => {
                let mut zh_voice = None;
                let mut en_voice = None;

                if let Ok(voices) = engine.voices() {
                    for v in &voices {
                        let lang = v.language().to_string().to_lowercase();
                        let name = v.name().to_lowercase();

                        if zh_voice.is_none()
                            && (lang.starts_with("zh")
                                || name.contains("chinese")
                                || name.contains("huihui")
                                || name.contains("yaoyao")
                                || name.contains("kangkang"))
                        {
                            zh_voice = Some(v.clone());
                        }

                        if en_voice.is_none()
                            && (lang.starts_with("en")
                                || name.contains("english")
                                || name.contains("zira")
                                || name.contains("david")
                                || name.contains("mark"))
                        {
                            en_voice = Some(v.clone());
                        }
                    }
                }

                tracing::info!(
                    has_zh_voice = zh_voice.is_some(),
                    has_en_voice = en_voice.is_some(),
                    "Windows 原生 TTS 初始化完成"
                );

                Ok(Self {
                    engine: Some(Mutex::new(engine)),
                    zh_voice,
                    en_voice,
                })
            }
            Err(e) => {
                tracing::warn!("初始化 Windows 系统 TTS 失败（将静音降级）: {}", e);
                Ok(Self {
                    engine: None,
                    zh_voice: None,
                    en_voice: None,
                })
            }
        }
    }

    /// 嗅探文本是否包含中文字符
    pub fn contains_chinese(text: &str) -> bool {
        text.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
    }

    pub fn speak(&self, text: &str, rate_multiplier: f32) -> Result<()> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let engine_lock = match &self.engine {
            Some(e) => e,
            None => return Err(anyhow!("系统 TTS 服务不可用")),
        };

        let mut engine = engine_lock.lock().map_err(|_| anyhow!("TTS 引擎锁异常"))?;

        // 智能语种切换：中文选中文发音人，英文/外文选英文发音人
        if Self::contains_chinese(trimmed) {
            if let Some(ref v) = self.zh_voice {
                let _ = engine.set_voice(v);
            }
        } else if let Some(ref v) = self.en_voice {
            let _ = engine.set_voice(v);
        }

        let target_rate =
            (engine.normal_rate() * rate_multiplier).clamp(engine.min_rate(), engine.max_rate());
        let _ = engine.set_rate(target_rate);

        // interrupt = true 确保立刻打断正在播放的内容
        engine
            .speak(trimmed, true)
            .map_err(|e| anyhow!("朗读失败: {}", e))?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        if let Some(engine_lock) = &self.engine {
            let mut engine = engine_lock.lock().map_err(|_| anyhow!("TTS 引擎锁异常"))?;
            let _ = engine.stop();
        }
        Ok(())
    }

    pub fn is_speaking(&self) -> bool {
        if let Some(engine_lock) = &self.engine {
            if let Ok(engine) = engine_lock.lock() {
                return engine.is_speaking().unwrap_or(false);
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chinese_detection() {
        assert!(NativeWinTts::contains_chinese("Hello 世界"));
        assert!(!NativeWinTts::contains_chinese("Hello world 123!@#"));
        assert!(NativeWinTts::contains_chinese("全中文测试"));
    }
}
