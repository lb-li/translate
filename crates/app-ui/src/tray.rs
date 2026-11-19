//! Windows 系统托盘与全局热键（Alt + Q）统一事件泵。
//! 在独立线程运行内核阻塞式 GetMessageW 循环，实现待机 0% CPU。

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tokio::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    TriggerCapture,
    ToggleVisibility,
    HideToTray,
    StopTts,
    Quit,
}

/// 生成内置 32x32 钴蓝色圆角极简托盘图标（免外部 .ico 文件依赖）
fn build_default_tray_icon() -> Icon {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) / 2.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;

            if dist <= 13.5 {
                // 钴蓝底色 (#0F52BA)
                rgba[idx] = 0x0F;
                rgba[idx + 1] = 0x52;
                rgba[idx + 2] = 0xBA;
                rgba[idx + 3] = 0xFF;

                // 内部白色简约菱形光点
                if (dx.abs() + dy.abs()) <= 5.0 {
                    rgba[idx] = 0xFF;
                    rgba[idx + 1] = 0xFF;
                    rgba[idx + 2] = 0xFF;
                }
            }
        }
    }

    Icon::from_rgba(rgba, size, size).expect("托盘图标 RGBA 数据合法")
}

/// 启动零轮询 Windows 后台事件线程：托管 Alt+Q 热键与系统托盘图标/右键菜单。
pub fn spawn_tray_and_hotkey_thread(cmd_tx: mpsc::UnboundedSender<UiCommand>) {
    std::thread::spawn(move || {
        // 1. 注册全局快捷键 Alt + Q
        let hotkey_manager = match GlobalHotKeyManager::new() {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("创建 GlobalHotKeyManager 失败: {}", e);
                return;
            }
        };
        let translate_hotkey = HotKey::new(Some(Modifiers::ALT), Code::KeyQ);
        if let Err(e) = hotkey_manager.register(translate_hotkey) {
            tracing::error!("注册 Alt+Q 失败: {}", e);
        }
        let hotkey_id = translate_hotkey.id();

        // 2. 构建系统托盘菜单
        let menu = Menu::new();
        let item_toggle = MenuItem::new("显示 / 隐藏浮窗", true, None);
        let item_stop_tts = MenuItem::new("停止朗读", true, None);
        let item_quit = MenuItem::new("退出 Lumen", true, None);
        let _ = menu.append(&item_toggle);
        let _ = menu.append(&item_stop_tts);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_quit);

        let toggle_id = item_toggle.id().clone();
        let stop_tts_id = item_stop_tts.id().clone();
        let quit_id = item_quit.id().clone();

        let _tray_icon = match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Lumen 划词翻译（Alt + Q）")
            .with_icon(build_default_tray_icon())
            .build()
        {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("创建系统托盘失败: {}", e);
                return;
            }
        };

        // 3. 转发热键事件（事件驱动阻塞接收）
        let tx_hk = cmd_tx.clone();
        std::thread::spawn(move || {
            let rx = GlobalHotKeyEvent::receiver();
            while let Ok(ev) = rx.recv() {
                if ev.id == hotkey_id && ev.state == HotKeyState::Pressed {
                    let _ = tx_hk.send(UiCommand::TriggerCapture);
                }
            }
        });

        // 4. 转发托盘菜单事件
        let tx_menu = cmd_tx.clone();
        std::thread::spawn(move || {
            let rx = MenuEvent::receiver();
            while let Ok(ev) = rx.recv() {
                if ev.id == toggle_id {
                    let _ = tx_menu.send(UiCommand::ToggleVisibility);
                } else if ev.id == stop_tts_id {
                    let _ = tx_menu.send(UiCommand::StopTts);
                } else if ev.id == quit_id {
                    let _ = tx_menu.send(UiCommand::Quit);
                    break;
                }
            }
        });

        // 5. 转发托盘图标左键单击事件（快速显隐）
        let tx_click = cmd_tx.clone();
        std::thread::spawn(move || {
            let rx = TrayIconEvent::receiver();
            while let Ok(ev) = rx.recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    let _ = tx_click.send(UiCommand::ToggleVisibility);
                }
            }
        });

        // 6. 运行原生 Win32 消息循环（GetMessageW 阻塞，无事件时 0% CPU）
        #[cfg(windows)]
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, TranslateMessage, MSG,
            };
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // 保持句柄存活直至退出
        drop(_tray_icon);
        drop(hotkey_manager);
    });
}
