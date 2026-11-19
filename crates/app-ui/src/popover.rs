use crate::icons;
use crate::theme::{Palette, ThemeMode};
use crate::tray::UiCommand;
use core_translate::{contains_chinese, WordInfo};
use core_tts::{HybridTtsEngine, TtsTier};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::gpui::{
    div, prelude::*, px, svg, Context, FocusHandle, FontWeight, IntoElement, KeyDownEvent, Render,
    ScrollHandle, Window, WindowAppearance,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpokenTarget {
    Source,
    Translated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewStatus {
    Ready,
    Streaming,
    Cached,
    Complete,
    Error,
}

pub struct TranslatorPopover {
    pub source_text: String,
    pub translated_text: String,
    pub provider_key: String,
    pub target_lang: String,
    pub is_pinned: bool,
    pub theme: ThemeMode,
    status: ViewStatus,
    slow_tts: bool,
    copied: bool,
    is_favorite: bool,
    source_expanded: bool,
    last_spoken: Option<SpokenTarget>,
    anim_offset_y: f32,
    anim_opacity: f32,
    waveform_tick: usize,
    waveform_running: bool,
    word_info: WordInfo,
    translation_scroll: ScrollHandle,
    focus_handle: FocusHandle,
    tts: Arc<HybridTtsEngine>,
    cmd_tx: mpsc::UnboundedSender<UiCommand>,
}

impl TranslatorPopover {
    pub fn new(
        tts: Arc<HybridTtsEngine>,
        cmd_tx: mpsc::UnboundedSender<UiCommand>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            _ => ThemeMode::Light,
        };
        Self {
            source_text: String::new(),
            translated_text: "划选文字后按 Alt + Q，译文会在这里出现。".to_string(),
            provider_key: String::new(),
            target_lang: "auto".to_string(),
            is_pinned: false,
            theme,
            status: ViewStatus::Ready,
            slow_tts: false,
            copied: false,
            is_favorite: false,
            source_expanded: false,
            last_spoken: None,
            anim_offset_y: 0.0,
            anim_opacity: 1.0,
            waveform_tick: 0,
            waveform_running: false,
            word_info: WordInfo::default(),
            translation_scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            tts,
            cmd_tx,
        }
    }

    pub fn set_anim_step(&mut self, opacity: f32, offset_y: f32, cx: &mut Context<Self>) {
        self.anim_opacity = opacity;
        self.anim_offset_y = offset_y;
        cx.notify();
    }

    pub fn focus_window(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
    }

    pub fn show_ready_hint(&mut self, hint: String, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.tts.stop();
        self.last_spoken = None;
        self.source_text.clear();
        self.translated_text = hint;
        self.provider_key.clear();
        self.word_info = WordInfo::default();
        self.status = ViewStatus::Ready;
        self.focus_window(window, cx);
        cx.notify();
    }

    pub fn start_translation(
        &mut self,
        source: String,
        provider_key: String,
        target_lang: String,
        is_favorite: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.tts.stop();
        self.last_spoken = None;
        self.source_text = source;
        self.translated_text.clear();
        self.provider_key = provider_key;
        self.target_lang = target_lang;
        self.is_favorite = is_favorite;
        self.status = ViewStatus::Streaming;
        self.copied = false;
        self.source_expanded = false;
        self.word_info = WordInfo::default();
        self.translation_scroll = ScrollHandle::new();
        self.focus_window(window, cx);
        cx.notify();
    }

    pub fn set_word_info(&mut self, info: WordInfo, cx: &mut Context<Self>) {
        self.word_info = info;
        cx.notify();
    }

    pub fn set_target_language(&mut self, target_lang: String, cx: &mut Context<Self>) {
        self.target_lang = target_lang;
        cx.notify();
    }

    pub fn set_favorite(&mut self, favorite: bool, cx: &mut Context<Self>) {
        self.is_favorite = favorite;
        cx.notify();
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.theme = self.theme.toggle();
        cx.notify();
    }

    pub fn push_delta(&mut self, delta: &str, cx: &mut Context<Self>) {
        self.translated_text.push_str(delta);
        self.translation_scroll.scroll_to_bottom();
        cx.notify();
    }

    pub fn finish_translation(&mut self, cached: bool, cx: &mut Context<Self>) {
        self.status = if cached {
            ViewStatus::Cached
        } else {
            ViewStatus::Complete
        };
        cx.notify();
    }

    pub fn show_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.translated_text = message;
        self.status = ViewStatus::Error;
        cx.notify();
    }

    fn language_label(&self) -> String {
        match self.target_lang.as_str() {
            "中文" => "AUTO → 中".to_string(),
            "English" => "AUTO → EN".to_string(),
            _ if contains_chinese(&self.source_text) => "中 → EN".to_string(),
            _ => "EN → 中".to_string(),
        }
    }

    fn copy_translation(&mut self, cx: &mut Context<Self>) {
        if self.translated_text.trim().is_empty() {
            return;
        }
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(self.translated_text.clone());
            self.copied = true;
            cx.notify();
            let executor = cx.background_executor().clone();
            cx.spawn(async move |this_handle, cx| {
                executor.timer(std::time::Duration::from_millis(1400)).await;
                let _ = this_handle.update(cx, |this, cx| {
                    this.copied = false;
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn toggle_speech(&mut self, target: SpokenTarget, cx: &mut Context<Self>) {
        if self.tts.is_speaking() && self.last_spoken == Some(target) {
            let _ = self.tts.stop();
            self.last_spoken = None;
            cx.notify();
            return;
        }
        let text = match target {
            SpokenTarget::Source => self.source_text.clone(),
            SpokenTarget::Translated => self.translated_text.clone(),
        };
        if text.trim().is_empty() {
            return;
        }
        if let Err(error) = self.tts.speak(&text, self.slow_tts, TtsTier::SystemNative) {
            tracing::warn!("朗读失败: {error}");
            return;
        }
        self.last_spoken = Some(target);
        self.schedule_waveform(cx);
        cx.notify();
    }

    fn schedule_waveform(&mut self, cx: &mut Context<Self>) {
        if self.waveform_running {
            return;
        }
        self.waveform_running = true;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this_handle, cx| {
            executor.timer(std::time::Duration::from_millis(110)).await;
            let _ = this_handle.update(cx, |this, cx| {
                this.waveform_running = false;
                if this.last_spoken.is_some() && this.tts.is_speaking() {
                    this.waveform_tick = this.waveform_tick.wrapping_add(1);
                    this.schedule_waveform(cx);
                } else {
                    this.last_spoken = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn waveform(&self, palette: Palette) -> impl IntoElement {
        let speaking = self.last_spoken.is_some() && self.tts.is_speaking();
        let color = if speaking {
            palette.teal
        } else {
            palette.muted
        };
        let heights = if speaking {
            let phase = self.waveform_tick % 6;
            let patterns = [
                [5.0, 11.0, 16.0, 9.0, 6.0],
                [8.0, 15.0, 9.0, 17.0, 10.0],
                [13.0, 7.0, 17.0, 11.0, 15.0],
                [7.0, 17.0, 12.0, 6.0, 13.0],
                [16.0, 10.0, 6.0, 15.0, 8.0],
                [10.0, 6.0, 14.0, 17.0, 5.0],
            ];
            patterns[phase]
        } else {
            [1.0, 1.0, 1.0, 1.0, 1.0]
        };
        div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(3.0))
            .w(px(28.0))
            .h(px(20.0))
            .children(
                heights
                    .into_iter()
                    .map(|height| div().w(px(2.0)).h(px(height)).rounded_full().bg(color)),
            )
    }
}

impl Render for TranslatorPopover {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme.palette();
        let mut card_bg = palette.window;
        card_bg.a = (0xF2 as f32 * self.anim_opacity).clamp(0.0, 255.0) / 255.0;
        let is_speaking = self.last_spoken.is_some() && self.tts.is_speaking();
        let (status_label, status_color) = match self.status {
            ViewStatus::Ready => ("就绪", palette.muted),
            ViewStatus::Streaming => ("翻译中", palette.accent),
            ViewStatus::Cached => ("缓存", palette.teal),
            ViewStatus::Complete => ("完成", palette.muted),
            ViewStatus::Error => ("错误", palette.error),
        };
        let has_word_meta =
            self.word_info.phonetic.is_some() || self.word_info.part_of_speech.is_some();
        let mut source_block = div()
            .id("source-summary")
            .w_full()
            .cursor_pointer()
            .text_color(palette.secondary)
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.source_expanded = !this.source_expanded;
                cx.notify();
            }));
        if has_word_meta {
            source_block = source_block
                .flex()
                .items_baseline()
                .gap(px(10.0))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.primary)
                        .child(self.source_text.clone()),
                )
                .when(self.word_info.phonetic.is_some(), |element| {
                    element.child(
                        div()
                            .font_family("Cascadia Mono")
                            .text_size(px(11.0))
                            .text_color(palette.muted)
                            .child(self.word_info.phonetic.clone().unwrap_or_default()),
                    )
                })
                .when(self.word_info.part_of_speech.is_some(), |element| {
                    element.child(
                        div()
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(4.0))
                            .bg(palette.hover)
                            .text_size(px(10.0))
                            .text_color(palette.muted)
                            .child(self.word_info.part_of_speech.clone().unwrap_or_default()),
                    )
                });
        } else {
            source_block = source_block
                .text_size(px(13.0))
                .line_height(px(20.0))
                .child(self.source_text.clone());
            if !self.source_expanded {
                source_block = source_block.line_clamp(2);
            }
        }

        let translated_content =
            if self.translated_text.is_empty() && self.status == ViewStatus::Streaming {
                "正在等待译文…".to_string()
            } else {
                self.translated_text.clone()
            };
        let translated_color = if self.status == ViewStatus::Error {
            palette.error
        } else if self.translated_text.is_empty() {
            palette.muted
        } else {
            palette.primary
        };

        div()
            .id("lumen-root")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(card_bg)
            .font_family("Segoe UI Variable Text")
            .text_color(palette.primary)
            .rounded(px(12.0))
            .border_1()
            .border_color(palette.border)
            .shadow_lg()
            .mt(px(self.anim_offset_y))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        let _ = this.cmd_tx.send(UiCommand::HideToTray);
                    }
                    "s" => {
                        let target = if this.translated_text.trim().is_empty() {
                            SpokenTarget::Source
                        } else {
                            SpokenTarget::Translated
                        };
                        this.toggle_speech(target, cx);
                    }
                    "enter" => this.copy_translation(cx),
                    "t" => {
                        let _ = this.cmd_tx.send(UiCommand::ToggleTheme);
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(36.0))
                    .px(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("language-switch")
                                    .h(px(24.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.secondary)
                                    .hover(|style| style.bg(palette.hover))
                                    .active(|style| style.bg(palette.hover))
                                    .on_click(cx.listener(|_this, _event, _window, _cx| {
                                        let _ = _this.cmd_tx.send(UiCommand::CycleTargetLanguage);
                                    }))
                                    .child(self.language_label()),
                            )
                            .child(div().size(px(5.0)).rounded_full().bg(status_color))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(palette.muted)
                                    .child(status_label),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .id("pin-window")
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .text_color(if self.is_pinned {
                                        palette.accent
                                    } else {
                                        palette.muted
                                    })
                                    .hover(|style| style.bg(palette.hover))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.is_pinned = !this.is_pinned;
                                        crate::tray::IS_PINNED
                                            .store(this.is_pinned, Ordering::Relaxed);
                                        cx.notify();
                                    }))
                                    .child(
                                        svg()
                                            .data(if self.is_pinned {
                                                icons::PIN_ON
                                            } else {
                                                icons::PIN_OFF
                                            })
                                            .size(px(13.0)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("hide-window")
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .text_color(palette.muted)
                                    .hover(|style| style.bg(palette.hover))
                                    .on_click(cx.listener(|this, _event, _window, _cx| {
                                        let _ = this.cmd_tx.send(UiCommand::HideToTray);
                                    }))
                                    .child(svg().data(icons::CLOSE).size(px(13.0))),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .min_h(px(0.0))
                    .px(px(16.0))
                    .pt(px(6.0))
                    .pb(px(8.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .min_h(px(40.0))
                            .py(px(4.0))
                            .child(source_block),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .h(px(1.0))
                            .mx(px(4.0))
                            .my(px(7.0))
                            .bg(palette.divider),
                    )
                    .child(
                        div()
                            .id("translation-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.translation_scroll)
                            .vertical_scrollbar(&self.translation_scroll)
                            .pr(px(4.0))
                            .text_size(px(15.0))
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(px(24.75))
                            .text_color(translated_color)
                            .child(translated_content),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(40.0))
                    .px(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .child(
                                div()
                                    .id("speak-translation")
                                    .size(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(palette.accent)
                                    .cursor_pointer()
                                    .text_color(palette.on_accent)
                                    .hover(|style| style.bg(palette.accent_hover))
                                    .active(|style| style.bg(palette.accent_hover))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        let target = if this.translated_text.trim().is_empty() {
                                            SpokenTarget::Source
                                        } else {
                                            SpokenTarget::Translated
                                        };
                                        this.toggle_speech(target, cx);
                                    }))
                                    .child(
                                        svg()
                                            .data(if is_speaking {
                                                icons::STOP
                                            } else {
                                                icons::PLAY
                                            })
                                            .size(px(14.0)),
                                    ),
                            )
                            .child(self.waveform(palette))
                            .child(
                                div()
                                    .id("speech-rate")
                                    .h(px(24.0))
                                    .px(px(5.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .font_family("Cascadia Mono")
                                    .text_size(px(10.0))
                                    .text_color(palette.muted)
                                    .hover(|style| style.bg(palette.hover))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.slow_tts = !this.slow_tts;
                                        cx.notify();
                                    }))
                                    .child(if self.slow_tts { "0.75×" } else { "1.0×" }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(palette.muted)
                            .child("Esc 隐藏 · S 朗读 · Enter 复制"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .id("favorite-word")
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .text_color(if self.is_favorite {
                                        palette.teal
                                    } else {
                                        palette.muted
                                    })
                                    .hover(|style| style.bg(palette.hover))
                                    .on_click(cx.listener(|this, _event, _window, _cx| {
                                        if matches!(
                                            this.status,
                                            ViewStatus::Cached | ViewStatus::Complete
                                        ) {
                                            let _ = this.cmd_tx.send(UiCommand::ToggleFavorite {
                                                source_text: this.source_text.clone(),
                                                target_lang: this.target_lang.clone(),
                                                translated: this.translated_text.clone(),
                                                provider: this.provider_key.clone(),
                                            });
                                        }
                                    }))
                                    .child(
                                        svg()
                                            .data(if self.is_favorite {
                                                icons::STAR_ON
                                            } else {
                                                icons::STAR
                                            })
                                            .size(px(14.0)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("copy-translation")
                                    .h(px(26.0))
                                    .px(px(6.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .text_color(if self.copied {
                                        palette.teal
                                    } else {
                                        palette.secondary
                                    })
                                    .hover(|style| style.bg(palette.hover))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.copy_translation(cx);
                                    }))
                                    .child(
                                        svg()
                                            .data(if self.copied {
                                                icons::CHECK
                                            } else {
                                                icons::COPY
                                            })
                                            .size(px(13.0)),
                                    )
                                    .child(div().text_size(px(10.0)).child(if self.copied {
                                        "已复制"
                                    } else {
                                        "复制"
                                    })),
                            ),
                    ),
            )
    }
}
