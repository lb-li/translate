use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeySettings {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: false,
            win: false,
            key: "q".to_string(),
        }
    }
}

impl HotkeySettings {
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.win {
            parts.push("Win".to_string());
        }
        let key = match self.key.as_str() {
            "space" => "Space".to_string(),
            "enter" => "Enter".to_string(),
            "tab" => "Tab".to_string(),
            "backspace" => "Backspace".to_string(),
            "delete" => "Delete".to_string(),
            "insert" => "Insert".to_string(),
            "home" => "Home".to_string(),
            "end" => "End".to_string(),
            "pageup" => "PageUp".to_string(),
            "pagedown" => "PageDown".to_string(),
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            value if value.chars().count() == 1 => value.to_uppercase(),
            value => value.to_string(),
        };
        parts.push(key);
        parts.join(" + ")
    }

    pub fn has_required_modifier(&self) -> bool {
        self.ctrl || self.alt || self.win
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub hotkey: HotkeySettings,
    pub launch_at_startup: bool,
    pub animations_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: HotkeySettings::default(),
            launch_at_startup: false,
            animations_enabled: true,
        }
    }
}

impl AppSettings {
    pub fn default_path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Lumen")
            .join("settings.json")
    }

    pub fn load() -> Result<Self> {
        let content = std::fs::read_to_string(Self::default_path())?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(Self::default_path())
    }

    pub fn save_to(&self, path: PathBuf) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("设置路径缺少父目录"))?;
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.flush()?;
        file.persist(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_is_alt_q() {
        let settings = AppSettings::default();
        assert_eq!(settings.hotkey.display(), "Alt + Q");
        assert!(settings.hotkey.has_required_modifier());
        assert!(settings.animations_enabled);
    }

    #[test]
    fn serializes_and_restores_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let settings = AppSettings {
            hotkey: HotkeySettings {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                key: "q".to_string(),
            },
            launch_at_startup: true,
            animations_enabled: false,
        };
        settings.save_to(path.clone()).unwrap();
        let restored: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(restored, settings);
        assert_eq!(restored.hotkey.display(), "Ctrl + Shift + Q");
    }

    #[test]
    fn missing_fields_use_defaults() {
        let settings: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn rejects_unmodified_letter_hotkey() {
        let hotkey = HotkeySettings {
            alt: false,
            key: "a".to_string(),
            ..HotkeySettings::default()
        };
        assert!(!hotkey.has_required_modifier());
    }
}
