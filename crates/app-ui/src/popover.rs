//! GPUI 高颜值悬浮翻译卡片视图（矢量图标化、少冗余文字、外部失焦自适应收起、原文实时编辑与回车重译）。

use crate::icons;
use crate::tray::UiCommand;
use core_tts::{HybridTtsEngine, TtsTier};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::gpui::{
    div, prelude::*, px, rgb, rgba, svg, Context, Entity, FontWeight, IntoElement, KeyDownEvent,
    Render, Window,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpokenTarget {
    Source,
    Translated,
}

pub struct TranslatorPopover {
    pub input_state: Entity<InputState>,
    pub source_text: String,
    pub translated_text: String,
    pub provider_label: String,
    pub is_streaming: bool,
    pub slow_tts: bool,
    pub is_pinned: bool,
    pub copied: bool,
    pub last_spoken: Option<SpokenTarget>,
    pub anim_offset_y: f32,
    pub anim_opacity: f32,
    pub tts: Arc<HybridTtsEngine>,
    pub cmd_tx: mpsc::UnboundedSender<UiCommand>,
}

impl TranslatorPopover {
    pub fn new(
        tts: Arc<HybridTtsEngine>,
        cmd_tx: mpsc::UnboundedSender<UiCommand>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入或划选原文，按 Enter 重新翻译")
        });

        cx.subscribe(&input_state, |this, _entity, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = this.input_state.read(cx).value().to_string();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let _ = this.cmd_tx.send(UiCommand::Retranslate(trimmed.to_string()));
                }
            }
        })
        .detach();

        Self {
            input_state,
            source_text: "按下 Alt + Q 划词捕获".to_string(),
            translated_text: "已就绪 · 双击划选文字后按 Alt + Q 翻译；点击外部任意位置自动关闭。".to_string(),
            provider_label: "就绪".to_string(),
            is_streaming: false,
            slow_tts: false,
            is_pinned: false,
            copied: false,
            last_spoken: None,
            anim_offset_y: 0.0,
            anim_opacity: 1.0,
            tts,
            cmd_tx,
        }
    }

    /// 设定平滑打开动画步进
    pub fn set_anim_step(&mut self, opacity: f32, offset_y: f32, cx: &mut Context<Self>) {
        self.anim_opacity = opacity;
        self.anim_offset_y = offset_y;
        cx.notify();
    }

    /// 未选中文字时按下快捷键的友好提示
    pub fn show_ready_hint(&mut self, hint: String, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.tts.stop();
        self.last_spoken = None;
        self.source_text = "未检测到选中文本".to_string();
        self.translated_text = hint;
        self.provider_label = "等待划词".to_string();
        self.is_streaming = false;
        self.input_state.update(cx, |state, cx| {
            state.clean(window, cx);
        });
        cx.notify();
    }

    pub fn start_translation(
        &mut self,
        source: String,
        provider_label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.tts.stop();
        self.last_spoken = None;
        self.copied = false;
        self.source_text = source.clone();
        self.translated_text.clear();
        self.provider_label = if provider_label.is_empty() {
            "翻译中".to_string()
        } else {
            provider_label
        };
        self.is_streaming = true;
        self.input_state.update(cx, |state, cx| {
            state.set_value(source, window, cx);
        });
        cx.notify();
    }

    pub fn push_delta(&mut self, delta: &str, cx: &mut Context<Self>) {
        self.translated_text.push_str(delta);
        cx.notify();
    }

    pub fn finish_translation(&mut self, cx: &mut Context<Self>) {
        self.is_streaming = false;
        self.provider_label = "完成".to_string();
        cx.notify();
    }
}

impl Render for TranslatorPopover {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let translated_preview = if self.translated_text.is_empty() && self.is_streaming {
            "正在翻译中…".to_string()
        } else {
            self.translated_text.clone()
        };
        let speed_label = if self.slow_tts { "0.75x" } else { "1.0x" };

        let is_speaking = self.tts.is_speaking();
        let is_speaking_source = is_speaking && self.last_spoken == Some(SpokenTarget::Source);
        let is_speaking_trans = is_speaking && self.last_spoken == Some(SpokenTarget::Translated);

        // 背景卡片透明度随打开动画平滑渐变
        let alpha = (0xFA as f32 * self.anim_opacity).clamp(0.0, 255.0) as u32;
        let card_bg = rgba((0xFFFFFF00) | alpha);

        // 状态指示灯颜色
        let status_color = if self.is_streaming {
            rgb(0x3B82F6) // 蓝色（翻译中）
        } else if self.translated_text.starts_with("[错误]") {
            rgb(0xEF4444) // 红色（异常）
        } else {
            rgb(0x10B981) // 绿色（就绪）
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(card_bg)
            .text_color(rgb(0x0F172A))
            .p(px(14.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgba(0xCBD5E1BB))
            .shadow_lg()
            .justify_between()
            .mt(px(self.anim_offset_y))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, _cx| {
                if ev.keystroke.key == "escape" && !this.is_pinned {
                    let _ = this.cmd_tx.send(UiCommand::HideToTray);
                }
            }))
            // 顶部极简状态栏（严格防挤压 flex_shrink_0）
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(0xF1F5F9))
                    .pb(px(8.0))
                    // 左侧：品牌 Logo + 微小状态指示点
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x0F52BA))
                                    .child("Lumen"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .size(px(6.0))
                                            .rounded_full()
                                            .bg(status_color),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x94A3B8))
                                            .child(self.provider_label.clone()),
                                    ),
                            ),
                    )
                    // 右侧：纯图标控制组（图钉 + 关闭叉号）
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            // 纯矢量 Pin 图标按钮
                            .child(
                                div()
                                    .id("btn-pin")
                                    .p(px(5.0))
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .bg(if self.is_pinned {
                                        rgb(0x0F52BA)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .border_1()
                                    .border_color(if self.is_pinned {
                                        rgb(0x0F52BA)
                                    } else {
                                        rgb(0xE2E8F0)
                                    })
                                    .child(
                                        svg()
                                            .data(if self.is_pinned {
                                                icons::PIN_ON
                                            } else {
                                                icons::PIN_OFF
                                            })
                                            .size(px(14.0))
                                            .text_color(if self.is_pinned {
                                                rgb(0xFFFFFF)
                                            } else {
                                                rgb(0x64748B)
                                            }),
                                    )
                                    .on_click(cx.listener(|this, _ev, _win, cx| {
                                        this.is_pinned = !this.is_pinned;
                                        crate::tray::IS_PINNED.store(this.is_pinned, Ordering::Relaxed);
                                        cx.notify();
                                    })),
                            )
                            // 纯矢量关闭叉号图标按钮
                            .child(
                                div()
                                    .id("btn-close")
                                    .p(px(5.0))
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(rgb(0xE2E8F0))
                                    .child(
                                        svg()
                                            .data(icons::CLOSE)
                                            .size(px(14.0))
                                            .text_color(rgb(0x64748B)),
                                    )
                                    .on_click(cx.listener(|this, _ev, _win, _cx| {
                                        let _ = this.cmd_tx.send(UiCommand::HideToTray);
                                    })),
                            ),
                    ),
            )
            // 原文摘要卡片（右上角微型喇叭纯图标快速朗读）
            .child(
                div()
                    .flex_shrink_0()
                    .my(px(6.0))
                    .p(px(8.0))
                    .rounded(px(8.0))
                    .bg(rgba(0xF8FAFCFF))
                    .border_1()
                    .border_color(rgb(0xE2E8F0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .mb(px(4.0))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x94A3B8))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("原文"),
                            )
                            // 原文右侧控制组：Enter重译图标 + 朗读喇叭图标
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    // 重新翻译微按钮（按回车或点击此图标重译）
                                    .child(
                                        div()
                                            .id("btn-retranslate")
                                            .p(px(3.0))
                                            .rounded(px(4.0))
                                            .cursor_pointer()
                                            .child(
                                                svg()
                                                    .data(icons::ENTER_KEY)
                                                    .size(px(13.0))
                                                    .text_color(rgb(0x64748B)),
                                            )
                                            .on_click(cx.listener(|this, _ev, _win, cx| {
                                                let text = this.input_state.read(cx).value().to_string();
                                                let trimmed = text.trim();
                                                if !trimmed.is_empty() {
                                                    let _ = this.cmd_tx.send(UiCommand::Retranslate(trimmed.to_string()));
                                                }
                                            })),
                                    )
                                    // 原文独立朗读微图标按钮
                                    .child(
                                        div()
                                            .id("btn-speak-source-mini")
                                            .p(px(3.0))
                                            .rounded(px(4.0))
                                            .cursor_pointer()
                                            .child(
                                                svg()
                                                    .data(if is_speaking_source {
                                                        icons::SPEAKER_ACTIVE
                                                    } else {
                                                        icons::SPEAKER
                                                    })
                                                    .size(px(13.0))
                                                    .text_color(if is_speaking_source {
                                                        rgb(0xE11D48)
                                                    } else {
                                                        rgb(0x64748B)
                                                    }),
                                            )
                                            .on_click(cx.listener(|this, _ev, _win, cx| {
                                                let text = this.input_state.read(cx).value().to_string();
                                                let to_speak = if text.trim().is_empty() {
                                                    this.source_text.clone()
                                                } else {
                                                    text
                                                };
                                                if this.tts.is_speaking() && this.last_spoken == Some(SpokenTarget::Source) {
                                                    let _ = this.tts.stop();
                                                    this.last_spoken = None;
                                                } else {
                                                    this.last_spoken = Some(SpokenTarget::Source);
                                                    let _ = this.tts.speak(
                                                        &to_speak,
                                                        this.slow_tts,
                                                        TtsTier::SystemNative,
                                                    );
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    // 原文输入与编辑区（支持直接修改、粘贴与回车重新翻译）
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .child(
                                Input::new(&self.input_state)
                                    .cleanable(true)
                                    .appearance(false)
                            ),
                    ),
            )
            // 译文主体区（自适应空间）
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .py(px(4.0))
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0x0F172A))
                    .child(translated_preview),
            )
            // 底部现代图标工具栏（严格防截断 flex_shrink_0）
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(0xF1F5F9))
                    .pt(px(8.0))
                    // 左侧：主要朗读操作与语速调节
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            // 朗读译文胶囊（小喇叭矢量图标 + 紧凑短文本）
                            .child(
                                div()
                                    .id("btn-speak-trans")
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .px(px(10.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .bg(if is_speaking_trans {
                                        rgb(0xE11D48)
                                    } else {
                                        rgb(0x0F52BA)
                                    })
                                    .cursor_pointer()
                                    .child(
                                        svg()
                                            .data(if is_speaking_trans {
                                                icons::SPEAKER_ACTIVE
                                            } else {
                                                icons::SPEAKER
                                            })
                                            .size(px(14.0))
                                            .text_color(rgb(0xFFFFFF)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0xFFFFFF))
                                            .child(if is_speaking_trans { "停止" } else { "朗读" }),
                                    )
                                    .on_click(cx.listener(|this, _ev, _win, cx| {
                                        if this.tts.is_speaking() && this.last_spoken == Some(SpokenTarget::Translated) {
                                            let _ = this.tts.stop();
                                            this.last_spoken = None;
                                        } else {
                                            this.last_spoken = Some(SpokenTarget::Translated);
                                            let _ = this.tts.speak(
                                                &this.translated_text,
                                                this.slow_tts,
                                                TtsTier::SystemNative,
                                            );
                                        }
                                        cx.notify();
                                    })),
                            )
                            // 倍速切换胶囊（速度表矢量图标 + 极简数值）
                            .child(
                                div()
                                    .id("btn-speed")
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(0xE2E8F0))
                                    .bg(rgb(0xFFFFFF))
                                    .cursor_pointer()
                                    .child(
                                        svg()
                                            .data(icons::SPEED)
                                            .size(px(13.0))
                                            .text_color(rgb(0x64748B)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x334155))
                                            .child(speed_label),
                                    )
                                    .on_click(cx.listener(|this, _ev, _win, cx| {
                                        this.slow_tts = !this.slow_tts;
                                        cx.notify();
                                    })),
                            ),
                    )
                    // 右侧：复制胶囊（剪贴板矢量图标 + 复制，复制后动态变为绿色对勾图标）
                    .child(
                        div()
                            .id("btn-copy")
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .px(px(10.0))
                            .py(px(5.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(if self.copied {
                                rgb(0xBBF7D0)
                            } else {
                                rgb(0xE2E8F0)
                            })
                            .bg(if self.copied {
                                rgb(0xF0FDF4)
                            } else {
                                rgb(0xFFFFFF)
                            })
                            .cursor_pointer()
                            .child(
                                svg()
                                    .data(if self.copied {
                                        icons::CHECK
                                    } else {
                                        icons::COPY
                                    })
                                    .size(px(13.0))
                                    .text_color(if self.copied {
                                        rgb(0x16A34A)
                                    } else {
                                        rgb(0x64748B)
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if self.copied {
                                        rgb(0x16A34A)
                                    } else {
                                        rgb(0x334155)
                                    })
                                    .child(if self.copied { "已复制" } else { "复制" }),
                            )
                            .on_click(cx.listener(|this, _ev, _win, cx| {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    let _ = cb.set_text(this.translated_text.clone());
                                }
                                this.copied = true;
                                cx.notify();

                                let executor = cx.background_executor().clone();
                                cx.spawn(async move |this_handle, cx| {
                                    executor.timer(std::time::Duration::from_millis(1500)).await;
                                    let _ = cx.update(|cx| {
                                        let _ = this_handle.update(cx, |this, cx| {
                                            this.copied = false;
                                            cx.notify();
                                        });
                                    });
                                })
                                .detach();
                            })),
                    ),
            )
    }
}
