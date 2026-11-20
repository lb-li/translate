use crate::icons;
use crate::popover::{icon, TranslatorPopover};
use crate::tray::UiCommand;
use gpui_kit::gpui::{div, prelude::*, px, Context, FontWeight, IntoElement};

impl TranslatorPopover {
    pub fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme.palette();
        let mut background = palette.window;
        background.a = 1.0;
        let startup_enabled = self.settings.launch_at_startup;
        let animations_enabled = self.settings.animations_enabled;

        div()
            .absolute()
            .top(px(36.0))
            .bottom(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bg(background)
            .rounded_b(px(12.0))
            .px(px(16.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("设置"),
                    )
                    .child(
                        div()
                            .id("close-settings")
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(palette.hover))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.show_settings = false;
                                this.recording_hotkey = false;
                                cx.notify();
                            }))
                            .child(icon(icons::CLOSE, 13.0, palette.muted)),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(palette.secondary)
                    .child("全局热键"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_family("Cascadia Mono")
                            .text_size(px(12.0))
                            .text_color(palette.primary)
                            .child(self.settings.hotkey.display()),
                    )
                    .child(
                        div()
                            .id("record-hotkey")
                            .h(px(26.0))
                            .px(px(9.0))
                            .flex()
                            .items_center()
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(if self.recording_hotkey {
                                palette.accent
                            } else {
                                palette.secondary
                            })
                            .bg(palette.hover)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.recording_hotkey = true;
                                this.settings_error = None;
                                cx.notify();
                            }))
                            .child(if self.recording_hotkey {
                                "按下组合键…"
                            } else {
                                "更改热键"
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(palette.muted)
                    .child("需包含 Ctrl、Alt 或 Win；Esc 取消录制"),
            )
            .child(div().h(px(1.0)).my(px(1.0)).bg(palette.divider))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(div().text_size(px(12.0)).child("开机自启动"))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(palette.muted)
                                    .child("登录 Windows 后自动运行"),
                            ),
                    )
                    .child(
                        div()
                            .id("autostart-switch")
                            .w(px(36.0))
                            .h(px(20.0))
                            .rounded_full()
                            .cursor_pointer()
                            .bg(if startup_enabled {
                                palette.accent
                            } else {
                                palette.hover
                            })
                            .on_click(cx.listener(|this, _event, _window, _cx| {
                                let _ = this.cmd_tx.send(UiCommand::SetAutostart(
                                    !this.settings.launch_at_startup,
                                ));
                            }))
                            .child(
                                div()
                                    .size(px(16.0))
                                    .mt(px(2.0))
                                    .ml(if startup_enabled { px(18.0) } else { px(2.0) })
                                    .rounded_full()
                                    .bg(if startup_enabled {
                                        palette.on_accent
                                    } else {
                                        palette.muted
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(div().text_size(px(12.0)).child("界面动效"))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(palette.muted)
                                    .child("弹性打开、加载骨架与开关反馈"),
                            ),
                    )
                    .child(
                        div()
                            .id("animations-switch")
                            .w(px(36.0))
                            .h(px(20.0))
                            .rounded_full()
                            .cursor_pointer()
                            .bg(if animations_enabled {
                                palette.accent
                            } else {
                                palette.hover
                            })
                            .on_click(cx.listener(|this, _event, _window, _cx| {
                                let _ = this.cmd_tx.send(UiCommand::SetAnimationsEnabled(
                                    !this.settings.animations_enabled,
                                ));
                            }))
                            .child(
                                div()
                                    .size(px(16.0))
                                    .mt(px(2.0))
                                    .ml(if animations_enabled {
                                        px(18.0)
                                    } else {
                                        px(2.0)
                                    })
                                    .rounded_full()
                                    .bg(if animations_enabled {
                                        palette.on_accent
                                    } else {
                                        palette.muted
                                    }),
                            ),
                    ),
            )
            .when(self.settings_error.is_some(), |element| {
                element.child(
                    div()
                        .text_size(px(10.0))
                        .line_height(px(15.0))
                        .text_color(palette.error)
                        .child(self.settings_error.clone().unwrap_or_default()),
                )
            })
            .child(
                div()
                    .mt_auto()
                    .text_size(px(10.0))
                    .text_color(palette.muted)
                    .child("设置自动保存，更改后立即生效"),
            )
    }
}
