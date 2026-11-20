#[cfg(windows)]
use anyhow::anyhow;
use anyhow::Result;
use core_storage::HotkeySettings;
#[cfg(windows)]
use global_hotkey::hotkey::HotKey;
#[cfg(windows)]
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::sync::atomic::Ordering;
#[cfg(windows)]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
#[cfg(windows)]
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
#[cfg(windows)]
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

pub static IS_PINNED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static MOUSE_CMD_TX: OnceLock<mpsc::UnboundedSender<UiCommand>> = OnceLock::new();

pub struct HotkeyController {
    #[cfg(windows)]
    manager: Arc<GlobalHotKeyManager>,
    #[cfg(windows)]
    current: Mutex<HotKey>,
    #[cfg(not(windows))]
    current: Mutex<HotkeySettings>,
}

impl HotkeyController {
    pub fn new(initial: HotkeySettings) -> Result<Self> {
        #[cfg(windows)]
        {
            let hotkey = crate::hotkey::to_hotkey(&initial)?;
            let manager =
                Arc::new(GlobalHotKeyManager::new().map_err(|error| anyhow!(error.to_string()))?);
            manager
                .register(hotkey)
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(Self {
                manager,
                current: Mutex::new(hotkey),
            })
        }
        #[cfg(not(windows))]
        {
            Ok(Self {
                current: Mutex::new(initial),
            })
        }
    }

    pub fn update(&self, next: HotkeySettings) -> Result<()> {
        #[cfg(windows)]
        {
            let next = crate::hotkey::to_hotkey(&next)?;
            let mut current = self.current.lock().expect("hotkey lock poisoned");
            let previous = *current;
            self.manager
                .unregister(previous)
                .map_err(|error| anyhow!(error.to_string()))?;
            if let Err(error) = self.manager.register(next) {
                self.manager
                    .register(previous)
                    .map_err(|rollback| anyhow!(rollback.to_string()))?;
                return Err(anyhow!(error.to_string()));
            }
            *current = next;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            *self.current.lock().expect("hotkey lock poisoned") = next;
            Ok(())
        }
    }

    #[cfg(windows)]
    fn current_id(&self) -> u32 {
        self.current.lock().expect("hotkey lock poisoned").id()
    }
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    TriggerCapture,
    OnCaptureFinished(Option<core_capture::CaptureResult>),
    Retranslate(String),
    ToggleVisibility,
    HideToTray,
    StopTts,
    CycleTargetLanguage,
    ToggleTheme,
    OpenSettings,
    ApplyHotkey(HotkeySettings),
    SetAutostart(bool),
    SetAnimationsEnabled(bool),
    ToggleFavorite {
        source_text: String,
        target_lang: String,
        translated: String,
        provider: String,
    },
    Quit,
}

#[cfg(windows)]
fn is_taskbar_click(point: windows::Win32::Foundation::POINT) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};

    for class_name in ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"] {
        let wide: Vec<u16> = class_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR(wide.as_ptr()), PCWSTR::null()) }) else {
            continue;
        };
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok()
            && point.x >= rect.left
            && point.x <= rect.right
            && point.y >= rect.top
            && point.y <= rect.bottom
        {
            return true;
        }
    }
    false
}

#[cfg(windows)]
unsafe extern "system" fn low_level_mouse_proc(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetWindowRect, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_NCLBUTTONDOWN,
        WM_NCRBUTTONDOWN, WM_RBUTTONDOWN,
    };

    if code >= 0 {
        let msg = wparam.0 as u32;
        if matches!(
            msg,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN
        ) && !IS_PINNED.load(Ordering::Relaxed)
        {
            let hook_struct = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            let click_pt = hook_struct.pt;
            if !is_taskbar_click(click_pt) {
                if let Some(hwnd_raw) = crate::win_positioning::popover_window(crate::POPOVER_TITLE)
                {
                    if crate::win_positioning::is_popover_visible(hwnd_raw) {
                        let mut rc = RECT::default();
                        let hwnd =
                            windows::Win32::Foundation::HWND(hwnd_raw as *mut std::ffi::c_void);
                        if GetWindowRect(hwnd, &mut rc).is_ok() {
                            let inside = click_pt.x >= rc.left
                                && click_pt.x <= rc.right
                                && click_pt.y >= rc.top
                                && click_pt.y <= rc.bottom;
                            if !inside {
                                if let Some(tx) = MOUSE_CMD_TX.get() {
                                    let _ = tx.send(UiCommand::HideToTray);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

#[cfg(windows)]
fn build_default_tray_icon() -> Icon {
    let bytes = include_bytes!("../../../assets/icons/lumen-tray.png");
    let image = image::load_from_memory(bytes)
        .expect("内嵌托盘 PNG 图标可解码")
        .to_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).expect("托盘图标 RGBA 数据合法")
}

#[cfg(windows)]
pub fn spawn_tray_and_hotkey_thread(
    cmd_tx: mpsc::UnboundedSender<UiCommand>,
    initial_hotkey: HotkeySettings,
) -> Result<Arc<HotkeyController>> {
    let controller = Arc::new(HotkeyController::new(initial_hotkey)?);
    let thread_controller = controller.clone();
    let _ = MOUSE_CMD_TX.set(cmd_tx.clone());
    std::thread::spawn(move || {
        let hotkey_manager = thread_controller.manager.clone();
        let menu = Menu::new();
        let item_toggle = MenuItem::new("显示 / 隐藏浮窗", true, None);
        let item_settings = MenuItem::new("设置", true, None);
        let item_language = MenuItem::new("切换翻译方向", true, None);
        let item_theme = MenuItem::new("切换深浅主题", true, None);
        let item_stop_tts = MenuItem::new("停止朗读", true, None);
        let item_quit = MenuItem::new("退出 Lumen", true, None);
        let _ = menu.append(&item_toggle);
        let _ = menu.append(&item_settings);
        let _ = menu.append(&item_language);
        let _ = menu.append(&item_theme);
        let _ = menu.append(&item_stop_tts);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_quit);
        let toggle_id = item_toggle.id().clone();
        let settings_id = item_settings.id().clone();
        let language_id = item_language.id().clone();
        let theme_id = item_theme.id().clone();
        let stop_tts_id = item_stop_tts.id().clone();
        let quit_id = item_quit.id().clone();
        let _tray_icon = match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Lumen 划词翻译")
            .with_icon(build_default_tray_icon())
            .build()
        {
            Ok(icon) => icon,
            Err(error) => {
                tracing::error!("创建系统托盘失败: {error}");
                return;
            }
        };

        let event_controller = thread_controller.clone();
        let tx_hk = cmd_tx.clone();
        std::thread::spawn(move || {
            let rx = GlobalHotKeyEvent::receiver();
            while let Ok(event) = rx.recv() {
                if event.id == event_controller.current_id() && event.state == HotKeyState::Pressed
                {
                    let _ = tx_hk.send(UiCommand::TriggerCapture);
                }
            }
        });
        let tx_menu = cmd_tx.clone();
        std::thread::spawn(move || {
            let rx = MenuEvent::receiver();
            while let Ok(event) = rx.recv() {
                let command = if event.id == toggle_id {
                    Some(UiCommand::ToggleVisibility)
                } else if event.id == settings_id {
                    Some(UiCommand::OpenSettings)
                } else if event.id == language_id {
                    Some(UiCommand::CycleTargetLanguage)
                } else if event.id == theme_id {
                    Some(UiCommand::ToggleTheme)
                } else if event.id == stop_tts_id {
                    Some(UiCommand::StopTts)
                } else if event.id == quit_id {
                    Some(UiCommand::Quit)
                } else {
                    None
                };
                if let Some(command) = command {
                    let quit = matches!(command, UiCommand::Quit);
                    let _ = tx_menu.send(command);
                    if quit {
                        break;
                    }
                }
            }
        });
        let tx_click = cmd_tx.clone();
        std::thread::spawn(move || {
            let rx = TrayIconEvent::receiver();
            while let Ok(event) = rx.recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let _ = tx_click.send(UiCommand::ToggleVisibility);
                }
            }
        });

        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
                UnhookWindowsHookEx, MSG, WH_MOUSE_LL,
            };
            let mouse_hook =
                SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), None, 0).ok();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if let Some(hook) = mouse_hook {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
        drop(_tray_icon);
        drop(hotkey_manager);
    });
    Ok(controller)
}

#[cfg(not(windows))]
pub fn spawn_tray_and_hotkey_thread(
    _cmd_tx: mpsc::UnboundedSender<UiCommand>,
    initial_hotkey: HotkeySettings,
) -> Result<Arc<HotkeyController>> {
    Ok(Arc::new(HotkeyController::new(initial_hotkey)?))
}
