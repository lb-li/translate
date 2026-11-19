#![windows_subsystem = "windows"]

mod icons;
mod popover;
pub mod tray;
mod win_effects;
pub mod win_positioning;

use anyhow::Result;
use core_storage::TranslationCache;
use core_translate::providers::openai_compat::OpenAiCompatProvider;
use core_translate::{GoogleTranslateFreeProvider, TranslateChunk, TranslateRequest, TranslationProvider};
use core_tts::HybridTtsEngine;
use gpui_kit::gpui::{
    px, size, App, AppContext, AsyncApp, Bounds, QuitMode, TitlebarOptions, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
};
use popover::TranslatorPopover;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tray::UiCommand;

pub const POPOVER_TITLE: &str = "Lumen Translate Popover";
pub const POPOVER_W: i32 = 460;
pub const POPOVER_H: i32 = 340;

fn play_open_animation(wh: WindowHandle<TranslatorPopover>, cx: &AsyncApp) {
    let mut cx_async = cx.clone();
    let executor = cx.background_executor().clone();
    cx.spawn(async move |_| {
        let total_steps = 5;
        for step in 1..=total_steps {
            let progress = step as f32 / total_steps as f32;
            let ease = 1.0 - (1.0 - progress) * (1.0 - progress);
            let opacity = 0.3 + 0.7 * ease;
            let offset_y = (1.0 - ease) * 8.0;
            let _ = wh.update(&mut cx_async, |view, _win, cx| {
                view.set_anim_step(opacity, offset_y, cx);
            });
            executor.timer(std::time::Duration::from_millis(16)).await;
        }
    })
    .detach();
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?,
    );
    let _rt_guard = rt.enter();

    let cache = Arc::new(TranslationCache::open("lumen_cache.sqlite3")?);
    let tts = Arc::new(HybridTtsEngine::new()?);
    let provider: Arc<dyn TranslationProvider> = match OpenAiCompatProvider::from_env() {
        Some(p) => Arc::new(p),
        None => Arc::new(GoogleTranslateFreeProvider::new()),
    };

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<UiCommand>();

    // 启动零轮询 Windows 托盘与全局热键（Alt + Q）消息泵线程
    tray::spawn_tray_and_hotkey_thread(cmd_tx.clone());

    gpui_kit::application()
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx: &mut App| {
        gpui_kit::init(cx);

        let bounds = Bounds::centered(None, size(px(POPOVER_W as f32), px(POPOVER_H as f32)), cx);
        let tts_for_view = tts.clone();
        let cmd_tx_for_view = cmd_tx.clone();

        let window_handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(POPOVER_TITLE.into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    focus: false,
                    show: false,
                    kind: WindowKind::PopUp,
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| TranslatorPopover::new(tts_for_view, cmd_tx_for_view, window, cx)),
            )
            .expect("无法创建 GPUI 浮窗");

        // 仅在启动完毕后安全应用 Win11 毛玻璃材质与 ToolWindow 扩展样式，避免启动闭包内的重入借用
        let bg_executor = cx.background_executor().clone();
        cx.spawn(async move |_cx| {
            bg_executor.timer(std::time::Duration::from_millis(50)).await;
            if let Some(hwnd) = win_positioning::find_window_by_title(POPOVER_TITLE) {
                win_positioning::apply_popover_ex_styles(hwnd);
                win_effects::apply_win11_popover_backdrop(hwnd, false);
            }
        })
        .detach();

        let rt_clone = rt.clone();
        let cache_clone = cache.clone();
        let provider_clone = provider.clone();
        let tts_clone = tts.clone();
        let cmd_tx_bg = cmd_tx.clone();

        cx.spawn(async move |cx| {
            while let Some(cmd) = cmd_rx.recv().await {
                let hwnd_opt = win_positioning::find_window_by_title(POPOVER_TITLE);

                match cmd {
                    UiCommand::HideToTray => {
                        if let Some(hwnd) = hwnd_opt {
                            win_positioning::hide_popover_to_tray(hwnd);
                        }
                    }
                    UiCommand::ToggleVisibility => {
                        if let Some(hwnd) = hwnd_opt {
                            if win_positioning::is_popover_visible(hwnd) {
                                win_positioning::hide_popover_to_tray(hwnd);
                            } else {
                                let _ = window_handle.update(cx, |view, _win, cx| {
                                    view.set_anim_step(0.3, 8.0, cx);
                                });
                                win_positioning::show_popover_near_cursor(hwnd, POPOVER_W, POPOVER_H);
                                play_open_animation(window_handle.clone(), cx);
                            }
                        }
                    }
                    UiCommand::StopTts => {
                        let _ = tts_clone.stop();
                    }
                    UiCommand::Quit => {
                        let _ = tts_clone.stop();
                        let _ = cx.update(|app| app.quit());
                        break;
                    }
                    UiCommand::TriggerCapture => {
                        // 关键架构优化：划词捕获完全在独立后台阻塞线程中执行，
                        // 绝不提前遮挡目标窗口、绝不阻塞主 UI 线程消息循环、彻底杜绝输入死锁
                        let cmd_tx_inner = cmd_tx_bg.clone();
                        rt_clone.spawn(async move {
                            let res = tokio::task::spawn_blocking(|| {
                                core_capture::capture_selection()
                            })
                            .await
                            .unwrap_or(Ok(None));

                            let _ = cmd_tx_inner.send(UiCommand::OnCaptureFinished(res.unwrap_or(None)));
                        });
                    }
                    UiCommand::Retranslate(input_text) => {
                        let text = input_text.trim().to_string();
                        if text.is_empty() {
                            continue;
                        }
                        let target_lang = "中文".to_string();

                        if let Ok(Some(cached)) = cache_clone.get(&text, &target_lang) {
                            let _ = window_handle.update(cx, |view, win, cx| {
                                view.start_translation(text.clone(), "SQLite 本地缓存命中".into(), win, cx);
                                view.push_delta(&cached, cx);
                                view.finish_translation(cx);
                            });
                            continue;
                        }

                        let provider_name = provider_clone.name().to_string();
                        let _ = window_handle.update(cx, |view, win, cx| {
                            view.start_translation(text.clone(), provider_name, win, cx);
                        });

                        let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel();
                        let req = TranslateRequest {
                            text: text.clone(),
                            target_lang: target_lang.clone(),
                        };
                        let prov = provider_clone.clone();
                        rt_clone.spawn(async move {
                            if let Err(err) = prov.translate_stream(req, chunk_tx.clone()).await {
                                let _ = chunk_tx.send(TranslateChunk::Error(err.to_string()));
                            }
                        });

                        let wh_chunk = window_handle.clone();
                        let cache_chunk = cache_clone.clone();
                        let prov_name = provider_clone.name();
                        let text_chunk = text.clone();
                        let target_lang_chunk = target_lang.clone();

                        cx.spawn(async move |cx| {
                            let mut full_result = String::new();
                            while let Some(chunk) = chunk_rx.recv().await {
                                match chunk {
                                    TranslateChunk::Delta(d) => {
                                        full_result.push_str(&d);
                                        let _ = wh_chunk.update(cx, |view, _win, cx| {
                                            view.push_delta(&d, cx);
                                        });
                                    }
                                    TranslateChunk::Done => {
                                        let _ = cache_chunk.put(
                                            &text_chunk,
                                            &target_lang_chunk,
                                            &full_result,
                                            prov_name,
                                        );
                                        let _ = wh_chunk.update(cx, |view, _win, cx| {
                                            view.finish_translation(cx);
                                        });
                                        break;
                                    }
                                    TranslateChunk::Error(err) => {
                                        let _ = wh_chunk.update(cx, |view, _win, cx| {
                                            view.push_delta(&format!("\n[错误] {}", err), cx);
                                            view.finish_translation(cx);
                                        });
                                        break;
                                    }
                                }
                            }
                        })
                        .detach();
                    }
                    UiCommand::OnCaptureFinished(captured_opt) => {
                        // 捕获完成后，在当前鼠标光标位置安全弹出浮窗并触发打开微动效
                        if let Some(hwnd) = hwnd_opt {
                            let _ = window_handle.update(cx, |view, _win, cx| {
                                view.set_anim_step(0.3, 8.0, cx);
                            });
                            win_positioning::show_popover_near_cursor(hwnd, POPOVER_W, POPOVER_H);
                            play_open_animation(window_handle.clone(), cx);
                        }

                        let captured = match captured_opt {
                            Some(c) => c,
                            None => {
                                let _ = window_handle.update(cx, |view, win, cx| {
                                    view.show_ready_hint(
                                        "未检测到选中文本。请先用鼠标划选文字，再按下 Alt + Q 快速翻译。".into(),
                                        win,
                                        cx,
                                    );
                                });
                                continue;
                            }
                        };

                        let text = captured.cleaned_text.clone();
                        let target_lang = "中文".to_string();

                        if let Ok(Some(cached)) = cache_clone.get(&text, &target_lang) {
                            let _ = window_handle.update(cx, |view, win, cx| {
                                view.start_translation(text.clone(), "SQLite 本地缓存命中".into(), win, cx);
                                view.push_delta(&cached, cx);
                                view.finish_translation(cx);
                            });
                            continue;
                        }

                        let provider_name = provider_clone.name().to_string();
                        let _ = window_handle.update(cx, |view, win, cx| {
                            view.start_translation(text.clone(), provider_name, win, cx);
                        });

                        let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel();
                        let req = TranslateRequest {
                            text: text.clone(),
                            target_lang: target_lang.clone(),
                        };
                        let prov = provider_clone.clone();
                        rt_clone.spawn(async move {
                            if let Err(err) = prov.translate_stream(req, chunk_tx.clone()).await {
                                let _ = chunk_tx.send(TranslateChunk::Error(err.to_string()));
                            }
                        });

                        // 在独立的轻量任务中消费翻译分块，保证 cmd_rx 外部主事件循环永远 0 阻塞
                        let wh_chunk = window_handle.clone();
                        let cache_chunk = cache_clone.clone();
                        let prov_name = provider_clone.name();
                        let text_chunk = text.clone();
                        let target_lang_chunk = target_lang.clone();

                        cx.spawn(async move |cx| {
                            let mut full_result = String::new();
                            while let Some(chunk) = chunk_rx.recv().await {
                                match chunk {
                                    TranslateChunk::Delta(d) => {
                                        full_result.push_str(&d);
                                        let _ = wh_chunk.update(cx, |view, _win, cx| {
                                            view.push_delta(&d, cx);
                                        });
                                    }
                                    TranslateChunk::Done => {
                                        let _ = cache_chunk.put(
                                            &text_chunk,
                                            &target_lang_chunk,
                                            &full_result,
                                            prov_name,
                                        );
                                        let _ = wh_chunk.update(cx, |view, _win, cx| {
                                            view.finish_translation(cx);
                                        });
                                        break;
                                    }
                                    TranslateChunk::Error(err) => {
                                        let _ = wh_chunk.update(cx, |view, _win, cx| {
                                            view.push_delta(&format!("\n[错误] {}", err), cx);
                                            view.finish_translation(cx);
                                        });
                                        break;
                                    }
                                }
                            }
                        })
                        .detach();
                    }
                }
            }
        })
        .detach();
    });

    Ok(())
}
