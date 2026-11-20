#![windows_subsystem = "windows"]

mod hotkey;
mod icons;
mod popover;
mod settings_view;
mod startup;
mod theme;
pub mod tray;
mod win_effects;
pub mod win_positioning;

use anyhow::Result;
use core_storage::{AppSettings, TranslationCache};
use core_translate::providers::openai_compat::OpenAiCompatProvider;
use core_translate::{
    is_english_word, GoogleTranslateFreeProvider, TranslateChunk, TranslateRequest,
    TranslationProvider,
};
use core_tts::HybridTtsEngine;
use gpui_kit::gpui::{
    px, size, App, AppContext, AsyncApp, Bounds, QuitMode, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use popover::TranslatorPopover;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tray::UiCommand;

pub const POPOVER_TITLE: &str = "Lumen Translate Popover";
pub const POPOVER_W: i32 = 420;
pub const POPOVER_H: i32 = 280;
const MAX_INPUT_CHARS: usize = 4000;

#[cfg(windows)]
fn acquire_single_instance() -> Option<windows::Win32::Foundation::HANDLE> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = "Local\\LumenTranslateSingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr())).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            None
        } else {
            Some(handle)
        }
    }
}

#[cfg(not(windows))]
fn acquire_single_instance() -> Option<bool> {
    Some(true)
}

fn play_open_animation(wh: WindowHandle<TranslatorPopover>, cx: &AsyncApp, enabled: bool) {
    let mut cx_async = cx.clone();
    let executor = cx.background_executor().clone();
    cx.spawn(async move |_| {
        if !enabled {
            let _ = wh.update(&mut cx_async, |view, _window, cx| {
                view.set_anim_step(1.0, 0.0, cx);
            });
            return;
        }
        for step in 1..=8 {
            let progress = step as f32 / 8.0;
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            let ease = 1.0 + c3 * (progress - 1.0).powi(3) + c1 * (progress - 1.0).powi(2);
            let opacity = (0.35 + 0.65 * progress).min(1.0);
            let offset_y = (1.0 - ease) * 8.0;
            let _ = wh.update(&mut cx_async, |view, _window, cx| {
                view.set_anim_step(opacity, offset_y, cx);
            });
            executor.timer(std::time::Duration::from_millis(15)).await;
        }
    })
    .detach();
}

fn main() -> Result<()> {
    let _instance_guard = match acquire_single_instance() {
        Some(guard) => guard,
        None => return Ok(()),
    };
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
    let cache = Arc::new(TranslationCache::open(TranslationCache::default_path())?);
    let tts = Arc::new(HybridTtsEngine::new()?);
    let provider: Arc<dyn TranslationProvider> = match OpenAiCompatProvider::from_env() {
        Some(provider) => Arc::new(provider),
        None => Arc::new(GoogleTranslateFreeProvider::new()),
    };
    let settings = AppSettings::load().unwrap_or_default();
    #[cfg(windows)]
    if let Err(error) = startup::set_launch_at_startup(settings.launch_at_startup) {
        tracing::warn!("同步开机启动项失败: {error}");
    }
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<UiCommand>();
    let hotkey_controller =
        tray::spawn_tray_and_hotkey_thread(cmd_tx.clone(), settings.hotkey.clone())?;
    let translation_generation = Arc::new(AtomicU64::new(0));

    gpui_kit::application()
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            let bounds =
                Bounds::centered(None, size(px(POPOVER_W as f32), px(POPOVER_H as f32)), cx);
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
                        window_background: WindowBackgroundAppearance::Blurred,
                        is_resizable: false,
                        is_minimizable: false,
                        ..Default::default()
                    },
                    |window, cx| {
                        #[cfg(windows)]
                        {
                            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                            if let Ok(handle) = window.window_handle() {
                                if let RawWindowHandle::Win32(raw) = handle.as_raw() {
                                    win_positioning::register_popover_window(raw.hwnd.get());
                                }
                            }
                        }
                        cx.new(|cx| {
                            TranslatorPopover::new(
                                tts.clone(),
                                cmd_tx.clone(),
                                settings.clone(),
                                window,
                                cx,
                            )
                        })
                    },
                )
                .expect("无法创建 GPUI 浮窗");

            let bg_executor = cx.background_executor().clone();
            cx.spawn(async move |_cx| {
                bg_executor
                    .timer(std::time::Duration::from_millis(60))
                    .await;
                if let Some(hwnd) = win_positioning::popover_window(POPOVER_TITLE) {
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
            let mut settings = settings;
            let mut target_lang = "auto".to_string();
            let mut capture_in_progress = false;
            let mut active_request: Option<tokio::task::JoinHandle<()>> = None;

            cx.spawn(async move |cx| {
                while let Some(cmd) = cmd_rx.recv().await {
                    let hwnd_opt = win_positioning::popover_window(POPOVER_TITLE);
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
                                    let _ = window_handle.update(cx, |view, window, cx| {
                                        view.set_anim_step(0.35, 8.0, cx);
                                        view.focus_window(window, cx);
                                    });
                                    win_positioning::show_popover_near_cursor(
                                        hwnd, POPOVER_W, POPOVER_H,
                                    );
                                    play_open_animation(
                                        window_handle,
                                        cx,
                                        settings.animations_enabled,
                                    );
                                }
                            }
                        }
                        UiCommand::StopTts => {
                            let _ = tts_clone.stop();
                        }
                        UiCommand::CycleTargetLanguage => {
                            target_lang = match target_lang.as_str() {
                                "auto" => "English".to_string(),
                                "English" => "中文".to_string(),
                                _ => "auto".to_string(),
                            };
                            let _ = window_handle.update(cx, |view, _window, cx| {
                                view.set_target_language(target_lang.clone(), cx);
                            });
                        }
                        UiCommand::ToggleTheme => {
                            let mut dark = false;
                            let _ = window_handle.update(cx, |view, _window, cx| {
                                view.toggle_theme(cx);
                                dark = view.theme.is_dark();
                            });
                            if let Some(hwnd) = hwnd_opt {
                                win_effects::apply_win11_popover_backdrop(hwnd, dark);
                            }
                        }
                        UiCommand::OpenSettings => {
                            let _ = window_handle.update(cx, |view, window, cx| {
                                view.open_settings(window, cx);
                            });
                            if let Some(hwnd) = hwnd_opt {
                                win_positioning::show_popover_near_cursor(
                                    hwnd, POPOVER_W, POPOVER_H,
                                );
                                play_open_animation(window_handle, cx, settings.animations_enabled);
                            }
                        }
                        UiCommand::ApplyHotkey(hotkey) => {
                            match hotkey_controller.update(hotkey.clone()) {
                                Ok(()) => {
                                    settings.hotkey = hotkey;
                                    let error = settings.save().err().map(|error| {
                                        format!("热键已生效，但设置保存失败：{error}")
                                    });
                                    let _ = window_handle.update(cx, |view, _window, cx| {
                                        view.set_settings(settings.clone(), error, cx);
                                    });
                                }
                                Err(error) => {
                                    let _ = window_handle.update(cx, |view, _window, cx| {
                                        view.show_settings_error(
                                            format!("热键注册失败，已保留原热键：{error}"),
                                            cx,
                                        );
                                    });
                                }
                            }
                        }
                        UiCommand::SetAutostart(enabled) => {
                            match startup::set_launch_at_startup(enabled) {
                                Ok(()) => {
                                    settings.launch_at_startup = enabled;
                                    let error = settings.save().err().map(|error| {
                                        format!("自启动已更新，但设置保存失败：{error}")
                                    });
                                    let _ = window_handle.update(cx, |view, _window, cx| {
                                        view.set_settings(settings.clone(), error, cx);
                                    });
                                }
                                Err(error) => {
                                    let _ = window_handle.update(cx, |view, _window, cx| {
                                        view.show_settings_error(
                                            format!("无法更新开机自启动：{error}"),
                                            cx,
                                        );
                                    });
                                }
                            }
                        }
                        UiCommand::SetAnimationsEnabled(enabled) => {
                            settings.animations_enabled = enabled;
                            let error = settings
                                .save()
                                .err()
                                .map(|error| format!("动效已更新，但设置保存失败：{error}"));
                            let _ = window_handle.update(cx, |view, _window, cx| {
                                view.set_settings(settings.clone(), error, cx);
                            });
                        }
                        UiCommand::Quit => {
                            if let Some(handle) = active_request.take() {
                                handle.abort();
                            }
                            let _ = tts_clone.stop();
                            cx.update(|app| app.quit());
                            break;
                        }
                        UiCommand::TriggerCapture => {
                            if capture_in_progress {
                                continue;
                            }
                            capture_in_progress = true;
                            let cmd_tx_inner = cmd_tx_bg.clone();
                            rt_clone.spawn(async move {
                                let result =
                                    tokio::task::spawn_blocking(core_capture::capture_selection)
                                        .await
                                        .unwrap_or(Ok(None));
                                let _ = cmd_tx_inner
                                    .send(UiCommand::OnCaptureFinished(result.unwrap_or(None)));
                            });
                        }
                        UiCommand::OnCaptureFinished(captured) => {
                            capture_in_progress = false;
                            if let Some(hwnd) = hwnd_opt {
                                let _ = window_handle.update(cx, |view, window, cx| {
                                    view.set_anim_step(0.35, 8.0, cx);
                                    view.focus_window(window, cx);
                                });
                                win_positioning::show_popover_near_cursor(
                                    hwnd, POPOVER_W, POPOVER_H,
                                );
                                play_open_animation(window_handle, cx, settings.animations_enabled);
                            }
                            match captured {
                                Some(result) if !result.cleaned_text.trim().is_empty() => {
                                    let _ =
                                        cmd_tx_bg.send(UiCommand::Retranslate(result.cleaned_text));
                                }
                                _ => {
                                    let _ = window_handle.update(cx, |view, window, cx| {
                                        view.show_ready_hint(
                                            format!(
                                                "未检测到选中文本。请先划选文字，再按 {}。",
                                                settings.hotkey.display()
                                            ),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }
                        }
                        UiCommand::Retranslate(input) => {
                            let text = input.trim().to_string();
                            if text.is_empty() {
                                continue;
                            }
                            if text.chars().count() > MAX_INPUT_CHARS {
                                let _ = window_handle.update(cx, |view, _window, cx| {
                                    view.show_error(
                                        format!(
                                            "选中文本过长，请控制在 {MAX_INPUT_CHARS} 字符以内。"
                                        ),
                                        cx,
                                    );
                                });
                                continue;
                            }
                            if let Some(handle) = active_request.take() {
                                handle.abort();
                            }
                            let generation =
                                translation_generation.fetch_add(1, Ordering::SeqCst) + 1;
                            let provider_key = provider_clone.cache_namespace();
                            let is_favorite = cache_clone
                                .is_favorite(&text, &target_lang)
                                .unwrap_or(false);
                            let _ = window_handle.update(cx, |view, window, cx| {
                                view.start_translation(
                                    text.clone(),
                                    provider_key.clone(),
                                    target_lang.clone(),
                                    is_favorite,
                                    window,
                                    cx,
                                );
                            });

                            if is_english_word(&text) {
                                let word_provider = provider_clone.clone();
                                let word_rt = rt_clone.clone();
                                let word_text = text.clone();
                                let word_window = window_handle;
                                let word_generation = translation_generation.clone();
                                cx.spawn(async move |cx| {
                                    let handle = word_rt.spawn(async move {
                                        word_provider.word_info(&word_text).await
                                    });
                                    if let Ok(Ok(Some(info))) = handle.await {
                                        if word_generation.load(Ordering::SeqCst) == generation {
                                            let _ = word_window.update(cx, |view, _window, cx| {
                                                view.set_word_info(info, cx);
                                            });
                                        }
                                    }
                                })
                                .detach();
                            }

                            if let Ok(Some(cached)) =
                                cache_clone.get(&text, &target_lang, &provider_key)
                            {
                                let _ = window_handle.update(cx, |view, _window, cx| {
                                    view.push_delta(&cached, cx);
                                    view.finish_translation(true, cx);
                                });
                                continue;
                            }

                            let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel();
                            let request = TranslateRequest {
                                text: text.clone(),
                                target_lang: target_lang.clone(),
                            };
                            let request_provider = provider_clone.clone();
                            active_request = Some(rt_clone.spawn(async move {
                                if let Err(error) = request_provider
                                    .translate_stream(request, chunk_tx.clone())
                                    .await
                                {
                                    let _ = chunk_tx.send(TranslateChunk::Error(error.to_string()));
                                }
                            }));

                            let chunk_window = window_handle;
                            let chunk_cache = cache_clone.clone();
                            let chunk_generation = translation_generation.clone();
                            let chunk_target = target_lang.clone();
                            cx.spawn(async move |cx| {
                                let mut full_result = String::new();
                                while let Some(chunk) = chunk_rx.recv().await {
                                    if chunk_generation.load(Ordering::SeqCst) != generation {
                                        break;
                                    }
                                    match chunk {
                                        TranslateChunk::Delta(delta) => {
                                            full_result.push_str(&delta);
                                            let _ = chunk_window.update(cx, |view, _window, cx| {
                                                view.push_delta(&delta, cx);
                                            });
                                        }
                                        TranslateChunk::Done => {
                                            let _ = chunk_cache.put(
                                                &text,
                                                &chunk_target,
                                                &full_result,
                                                &provider_key,
                                            );
                                            let _ = chunk_window.update(cx, |view, _window, cx| {
                                                view.finish_translation(false, cx);
                                            });
                                            break;
                                        }
                                        TranslateChunk::Error(error) => {
                                            let _ = chunk_window.update(cx, |view, _window, cx| {
                                                view.show_error(format!("翻译失败：{error}"), cx);
                                            });
                                            break;
                                        }
                                    }
                                }
                            })
                            .detach();
                        }
                        UiCommand::ToggleFavorite {
                            source_text,
                            target_lang,
                            translated,
                            provider,
                        } => {
                            let favorite = cache_clone
                                .toggle_favorite(&source_text, &target_lang, &translated, &provider)
                                .unwrap_or(false);
                            let _ = window_handle.update(cx, |view, _window, cx| {
                                view.set_favorite(favorite, cx);
                            });
                        }
                    }
                }
            })
            .detach();
        });
    Ok(())
}
