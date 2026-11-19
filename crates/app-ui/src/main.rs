#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod popover;
mod tray;
mod win_effects;
mod win_positioning;

use anyhow::Result;
use core_storage::TranslationCache;
use core_translate::providers::openai_compat::OpenAiCompatProvider;
use core_translate::{DemoLocalProvider, TranslateChunk, TranslateRequest, TranslationProvider};
use core_tts::HybridTtsEngine;
use gpui_kit::gpui::{
    px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};
use popover::TranslatorPopover;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tray::UiCommand;

const POPOVER_TITLE: &str = "Lumen Translate Popover";
const POPOVER_W: i32 = 420;
const POPOVER_H: i32 = 280;

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

    let cache = Arc::new(TranslationCache::open("lumen_cache.sqlite3")?);
    let tts = Arc::new(HybridTtsEngine::new()?);
    let provider: Arc<dyn TranslationProvider> = match OpenAiCompatProvider::from_env() {
        Some(p) => Arc::new(p),
        None => Arc::new(DemoLocalProvider),
    };

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<UiCommand>();

    // 启动零轮询 Windows 托盘与全局热键（Alt + Q）消息泵线程
    tray::spawn_tray_and_hotkey_thread(cmd_tx.clone());

    Application::new().run(move |cx: &mut App| {
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
                    ..Default::default()
                },
                |_window, cx| cx.new(|_cx| TranslatorPopover::new(tts_for_view, cmd_tx_for_view)),
            )
            .expect("无法创建 GPUI 浮窗");

        // 初始化 Windows 11 原生毛玻璃材质、ToolWindow/NoActivate 样式，并默认隐藏至托盘等待 Alt+Q
        if let Some(hwnd) = win_positioning::find_window_by_title(POPOVER_TITLE) {
            win_positioning::apply_popover_ex_styles(hwnd);
            win_effects::apply_win11_popover_backdrop(hwnd, false);
            win_positioning::hide_popover_to_tray(hwnd);
        }

        let rt_clone = rt.clone();
        let cache_clone = cache.clone();
        let provider_clone = provider.clone();
        let tts_clone = tts.clone();

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
                                win_positioning::show_popover_near_cursor(hwnd, POPOVER_W, POPOVER_H);
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
                        let captured = match core_capture::capture_selection() {
                            Ok(Some(res)) => res,
                            Ok(None) => continue,
                            Err(e) => {
                                tracing::warn!("划词捕获失败: {}", e);
                                continue;
                            }
                        };

                        // 紧贴当前鼠标光标弹出（多屏防裁切、不抢占原焦点）
                        if let Some(hwnd) = hwnd_opt {
                            win_positioning::show_popover_near_cursor(hwnd, POPOVER_W, POPOVER_H);
                        }

                        let text = captured.cleaned_text.clone();
                        let target_lang = "中文".to_string();

                        if let Ok(Some(cached)) = cache_clone.get(&text, &target_lang) {
                            let _ = window_handle.update(cx, |view, _win, cx| {
                                view.start_translation(text.clone(), "SQLite 本地缓存命中".into(), cx);
                                view.push_delta(&cached, cx);
                                view.finish_translation(cx);
                            });
                            continue;
                        }

                        let provider_name = provider_clone.name().to_string();
                        let _ = window_handle.update(cx, |view, _win, cx| {
                            view.start_translation(text.clone(), provider_name, cx);
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

                        let mut full_result = String::new();
                        while let Some(chunk) = chunk_rx.recv().await {
                            match chunk {
                                TranslateChunk::Delta(d) => {
                                    full_result.push_str(&d);
                                    let _ = window_handle.update(cx, |view, _win, cx| {
                                        view.push_delta(&d, cx);
                                    });
                                }
                                TranslateChunk::Done => {
                                    let _ = cache_clone.put(
                                        &text,
                                        &target_lang,
                                        &full_result,
                                        provider_clone.name(),
                                    );
                                    let _ = window_handle.update(cx, |view, _win, cx| {
                                        view.finish_translation(cx);
                                    });
                                    break;
                                }
                                TranslateChunk::Error(err) => {
                                    let _ = window_handle.update(cx, |view, _win, cx| {
                                        view.push_delta(&format!("\n[错误] {}", err), cx);
                                        view.finish_translation(cx);
                                    });
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        })
        .detach();
    });

    Ok(())
}
