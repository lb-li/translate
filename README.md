# Lumen Translate v0.2 (Windows + GPUI)

基于 **Rust + GPUI (DirectX 11 / DirectWrite)** 的高性能、高颜值、低功耗 Windows 划词翻译（含语音），本版本已补全：

1. **多屏 DPI 感知的光标跟随定位 (`app-ui/src/win_positioning.rs`)**：
   - 实时读取鼠标光标坐标与所在显示器工作区 (`MonitorFromPoint` + `GetMonitorInfoW` 避开任务栏)。
   - 靠近屏幕下边缘时自动向上翻转，左右边界自动防裁切。
   - 使用 `WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` + `SetWindowPos(SWP_NOACTIVATE)`，弹出时不抢占当前编辑器焦点、不占据任务栏图标。
2. **零轮询系统托盘驻留与隐藏逻辑 (`app-ui/src/tray.rs`)**：
   - 独立 Win32 消息泵线程（`GetMessageW` 内核态阻塞，待机 0% CPU）统一托管 `global-hotkey` (`Alt + Q`) 与 `tray-icon` 托盘菜单。
   - 托盘右键菜单支持：**显示/隐藏浮窗**、**停止朗读**、**退出 Lumen**；左键单击托盘快速切换显隐。
   - 浮窗顶部支持 **钉住 (Pin)** 与 **隐藏至托盘 (`×`)**，未钉住时按 `Esc` 立即隐藏至托盘而非退出进程。
3. **两种一键获取 `.exe` 的方式**：
   - **本地编译**：在安装了 Rust MSVC 与 VS Build Tools 2022 的 Windows 上运行 `.\build-windows.ps1`，产物位于 `dist\lumen-translate.exe`。
   - **云端零环境编译**：将本目录推送到 GitHub 仓库，触发自带的 `.github/workflows/build-windows.yml`，约 3 分钟后直接在 Actions 页面下载编译好的 `lumen-translate.exe`。

## 可选 API 配置

未设置环境变量时默认进入本地演示模式（可直接测试划词捕获、光标跟随、托盘显隐与 WinRT 朗读）：

```powershell
$env:LUMEN_API_BASE = "https://api.openai.com/v1"
$env:LUMEN_API_KEY  = "sk-..."
$env:LUMEN_MODEL    = "gpt-4o-mini"
.\dist\lumen-translate.exe
```
