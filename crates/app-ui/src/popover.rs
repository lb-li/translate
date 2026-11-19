//! GPUI 高颜值悬浮翻译卡片视图（按需重绘，支持钉住与一键隐藏至托盘）。

use crate::tray::UiCommand;
use core_tts::{HybridTtsEngine, TtsTier};
use gpui_kit::gpui::{
    div, prelude::*, px, rgb, rgba, Context, FontWeight, IntoElement, KeyDownEvent, Render, Window,
};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct TranslatorPopover {
    pub source_text: String,
    pub translated_text: String,
    pub provider_label: String,
    pub is_streaming: bool,
    pub slow_tts: bool,
    pub is_pinned: bool,
    pub tts: Arc<HybridTtsEngine>,
    pub cmd_tx: mpsc::UnboundedSender<UiCommand>,
}

impl TranslatorPopover {
    pub fn new(tts: Arc<HybridTtsEngine>, cmd_tx: mpsc::UnboundedSender<UiCommand>) -> Self {
        Self {
            source_text: "按下 Alt + Q 捕获任意窗口选中文本".to_string(),
            translated_text: "已启用光标跟随与托盘驻留；按 Esc 或点击右上角 × 可隐藏至系统托盘。".to_string(),
            provider_label: "就绪 · Windows UIA".to_string(),
            is_streaming: false,
            slow_tts: false,
            is_pinned: false,
            tts,
            cmd_tx,
        }
    }

    pub fn start_translation(&mut self, source: String, provider_label: String, cx: &mut Context<Self>) {
        self.source_text = source;
        self.translated_text.clear();
        self.provider_label = provider_label;
        self.is_streaming = true;
        cx.notify();
    }

    pub fn push_delta(&mut self, delta: &str, cx: &mut Context<Self>) {
        self.translated_text.push_str(delta);
        cx.notify();
    }

    pub fn finish_translation(&mut self, cx: &mut Context<Self>) {
        self.is_streaming = false;
        cx.notify();
    }
}

impl Render for TranslatorPopover {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let source_preview = self.source_text.clone();
        let translated_preview = if self.translated_text.is_empty() && self.is_streaming {
            "正在流式翻译…".to_string()
        } else {
            self.translated_text.clone()
        };
        let slow_label = if self.slow_tts { "0.75× 慢速" } else { "1.0× 常速" };
        let pin_label = if self.is_pinned { "已钉住" } else { "钉住" };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgba(0xFFFFFFEE))
            .text_color(rgb(0x0F172A))
            .p(px(16.0))
            .gap(px(12.0))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, _cx| {
                if ev.keystroke.key == "escape" && !this.is_pinned {
                    let _ = this.cmd_tx.send(UiCommand::HideToTray);
                }
            }))
            // 顶部状态条：品牌、引擎来源、钉住、隐藏到托盘
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(0xE2E8F0))
                    .pb(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x0A3A8A))
                                    .child("Lumen"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x475569))
                                    .child(self.provider_label.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .id("btn-pin")
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded(px(5.0))
                                    .border_1()
                                    .border_color(if self.is_pinned {
                                        rgb(0x0F52BA)
                                    } else {
                                        rgb(0xCBD5E1)
                                    })
                                    .text_color(if self.is_pinned {
                                        rgb(0x0A3A8A)
                                    } else {
                                        rgb(0x475569)
                                    })
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(pin_label)
                                    .on_click(cx.listener(|this, _ev, _win, cx| {
                                        this.is_pinned = !this.is_pinned;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("btn-hide")
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded(px(5.0))
                                    .border_1()
                                    .border_color(rgb(0xCBD5E1))
                                    .text_color(rgb(0x475569))
                                    .text_xs()
                                    .cursor_pointer()
                                    .child("×")
                                    .on_click(cx.listener(|this, _ev, _win, _cx| {
                                        let _ = this.cmd_tx.send(UiCommand::HideToTray);
                                    })),
                            ),
                    ),
            )
            // 原文摘要区
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x475569))
                    .child(source_preview),
            )
            // 译文主体区
            .child(
                div()
                    .flex_1()
                    .text_base()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0x0F172A))
                    .child(translated_preview),
            )
            // 底部工具栏：朗读、语速切换、复制译文
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(0xE2E8F0))
                    .pt(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("btn-speak")
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(0x0F52BA))
                                    .text_color(rgb(0xFFFFFF))
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .child("朗读原文")
                                    .on_click(cx.listener(|this, _ev, _win, _cx| {
                                        let _ = this.tts.speak(
                                            &this.source_text,
                                            this.slow_tts,
                                            TtsTier::SystemNative,
                                        );
                                    })),
                            )
                            .child(
                                div()
                                    .id("btn-rate")
                                    .px(px(10.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(0xCBD5E1))
                                    .text_color(rgb(0x334155))
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(slow_label)
                                    .on_click(cx.listener(|this, _ev, _win, cx| {
                                        this.slow_tts = !this.slow_tts;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("btn-copy")
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(rgb(0xCBD5E1))
                            .text_color(rgb(0x334155))
                            .text_xs()
                            .cursor_pointer()
                            .child("复制译文")
                            .on_click(cx.listener(|this, _ev, _win, _cx| {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    let _ = cb.set_text(this.translated_text.clone());
                                }
                            })),
                    ),
            )
    }
}
