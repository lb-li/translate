use gpui_kit::gpui::{rgb, rgba, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Light => Palette {
                window: rgba(0xFFFFFFF2),
                border: rgba(0x0F172A14),
                divider: rgba(0x0F172A12),
                hover: rgba(0x0F172A0F),
                primary: rgb(0x0F172A),
                secondary: rgb(0x64748B),
                muted: rgb(0x94A3B8),
                accent: rgb(0x0F52BA),
                accent_hover: rgb(0x0D47A1),
                on_accent: rgb(0xF8FAFC),
                teal: rgb(0x0D9488),
                error: rgb(0xDC2626),
            },
            Self::Dark => Palette {
                window: rgba(0x101318F2),
                border: rgba(0xFFFFFF1A),
                divider: rgba(0xFFFFFF14),
                hover: rgba(0xFFFFFF0F),
                primary: rgb(0xF8FAFC),
                secondary: rgb(0x94A3B8),
                muted: rgb(0x64748B),
                accent: rgb(0x4F8CFF),
                accent_hover: rgb(0x6B9FFF),
                on_accent: rgb(0x0F172A),
                teal: rgb(0x0D9488),
                error: rgb(0xF87171),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub window: Rgba,
    pub border: Rgba,
    pub divider: Rgba,
    pub hover: Rgba,
    pub primary: Rgba,
    pub secondary: Rgba,
    pub muted: Rgba,
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub on_accent: Rgba,
    pub teal: Rgba,
    pub error: Rgba,
}
