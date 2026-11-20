use core_storage::HotkeySettings;
use gpui_kit::gpui::Keystroke;

pub fn from_keystroke(keystroke: &Keystroke) -> Option<HotkeySettings> {
    let key = keystroke.key.to_lowercase();
    if matches!(
        key.as_str(),
        "escape" | "control" | "ctrl" | "alt" | "shift" | "super" | "meta" | "win" | "platform"
    ) || !is_supported_key(&key)
    {
        return None;
    }
    let settings = HotkeySettings {
        ctrl: keystroke.modifiers.control,
        alt: keystroke.modifiers.alt,
        shift: keystroke.modifiers.shift,
        win: keystroke.modifiers.platform,
        key,
    };
    settings.has_required_modifier().then_some(settings)
}

fn is_supported_key(key: &str) -> bool {
    matches!(key.chars().count(), 1)
        || matches!(
            key,
            "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12"
        )
        || matches!(
            key,
            "space"
                | "tab"
                | "enter"
                | "backspace"
                | "delete"
                | "insert"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
                | "left"
                | "right"
                | "up"
                | "down"
        )
}

#[cfg(windows)]
pub fn to_hotkey(settings: &HotkeySettings) -> anyhow::Result<global_hotkey::hotkey::HotKey> {
    use anyhow::anyhow;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    let mut modifiers = Modifiers::empty();
    if settings.ctrl {
        modifiers |= Modifiers::CONTROL;
    }
    if settings.alt {
        modifiers |= Modifiers::ALT;
    }
    if settings.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if settings.win {
        modifiers |= Modifiers::META;
    }
    if !settings.has_required_modifier() {
        return Err(anyhow!("热键至少需要包含 Ctrl、Alt 或 Win"));
    }
    let code = key_code(&settings.key).ok_or_else(|| anyhow!("不支持的按键: {}", settings.key))?;
    Ok(HotKey::new(Some(modifiers), code))
}

#[cfg(windows)]
fn key_code(key: &str) -> Option<global_hotkey::hotkey::Code> {
    use global_hotkey::hotkey::Code;
    let code = match key {
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "space" => Code::Space,
        "tab" => Code::Tab,
        "enter" => Code::Enter,
        "backspace" => Code::Backspace,
        "delete" => Code::Delete,
        "insert" => Code::Insert,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "left" => Code::ArrowLeft,
        "right" => Code::ArrowRight,
        "up" => Code::ArrowUp,
        "down" => Code::ArrowDown,
        _ => return None,
    };
    Some(code)
}
