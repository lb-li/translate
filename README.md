# Lumen Translate v0.3.3

Lumen 是一款 Windows 11 原生划词翻译悬浮窗，使用 Rust、GPUI、DirectX 11 与 DirectWrite 构建。默认自动识别中文并译为英文、识别英文并译为中文，同时支持免 API Key 翻译、流式输出、Windows 原生语音、Light / Dark 主题、本地缓存和生词收藏。

完整视觉与交互规范见 [DESIGN.md](DESIGN.md)。

## 使用方式

1. 在任意应用中划选文字。
2. 按 `Alt + Q`，浮窗会出现在当前光标附近。
3. 点击浮窗外部或按 `Esc` 隐藏；点击 Pin 可保持浮窗。

| 快捷键 / 操作 | 功能 |
|---|---|
| `Alt + Q` | 捕获选区并翻译 |
| `Esc` | 隐藏至托盘 |
| `S` | 朗读或停止朗读译文 |
| `Enter` | 复制译文 |
| `T` | 切换深浅主题 |
| 点击原文 | 展开或收起原文 |
| 点击原文行右侧喇叭 | 朗读或停止朗读原文 |
| 点击语种胶囊 | 自动方向、强制英文、强制中文循环 |
| 点击星标 | 收藏或取消收藏 |

## 界面与体验

- 420×280px 三段式悬浮窗，36px 顶栏、16px 内容内边距、40px 语音栏。
- Light / Dark 双主题，启动时读取系统外观，也可按 `T` 或托盘菜单切换。
- 原文默认两行截断；英文单词可显示音标与词性。
- 长译文支持原生滚动条，流式输出自动跟随底部。
- 五柱青绿声波只在朗读时更新，静止时不运行动画。
- 多显示器定位按光标所在屏幕的 DPI 和工作区计算，自动避开任务栏与屏幕边缘。

## 捕获与翻译

捕获优先使用 Windows UI Automation，不修改剪贴板；无法取得选区时才模拟 Ctrl+C。剪贴板兜底具备以下约束：

- 终端应用不发送模拟按键。
- 只释放 Alt 修饰键，不影响 Shift、Ctrl 或 Win 选区操作。
- 只有检测到剪贴板序列号真实变化后才采用结果，不读取陈旧内容。
- 读取后恢复原剪贴板文本；图片、文件等非文本格式无法通过当前文本快照恢复。

默认翻译链路为 Google 免费通道、英文短词词典、MyMemory 容灾。长文本会分段请求。配置以下环境变量后可改用 OpenAI 兼容的大模型流式接口：

```powershell
$env:LUMEN_API_BASE = "https://api.openai.com/v1"
$env:LUMEN_API_KEY  = "sk-..."
$env:LUMEN_MODEL    = "gpt-4o-mini"
.\dist\lumen-translate.exe
```

翻译请求支持中止与代际隔离，连续划词不会把多个结果拼接在一起。缓存键包含原文、目标语言和引擎/模型，切换模型不会复用旧引擎译文。

## 本地数据

缓存与收藏位于：

```text
%LOCALAPPDATA%\Lumen\lumen_cache.sqlite3
```

缓存最多保留最近 2000 条。收藏与缓存均为本地明文 SQLite 数据。

## 隐私说明

划词内容会发送给当前翻译服务提供方。默认链路可能涉及 Google、有道和 MyMemory；配置大模型后发送给所配置的 API 服务。音标与词性查询会将有道作为词典服务。请勿在不可信网络或受数据出境、保密要求约束的环境中直接翻译敏感文本。

## 构建

本地构建需要 Rust MSVC 工具链和 Visual Studio 2022 Build Tools：

```powershell
.\build-windows.ps1
```

产物为 `dist\lumen-translate.exe`。

也可以推送至 GitHub，使用 `.github/workflows/build-windows.yml` 完成格式检查、测试、Clippy 和 Release 构建，然后从 Actions 下载 `lumen-translate-windows-x64`。推送 `v*` 标签时会额外创建 GitHub Release，并自动附上 `lumen-translate.exe`。托盘图标内嵌于 `assets/icons/lumen-tray.png`。

## 模块结构

```text
crates/
  app-ui/          GPUI 浮窗、主题、托盘、热键、Win32 集成
  core-capture/    UIA 捕获、剪贴板兜底、PDF 文本清洗
  core-translate/  翻译 Provider、流式 SSE、词典元数据
  core-storage/    SQLite 缓存与收藏
  core-tts/        Windows 原生语音与发音人匹配
```

## 验证范围

核心捕获清洗、文本分段、语言方向和存储逻辑包含离线单元测试。真实翻译接口测试默认标记为 `ignore`，不会影响日常构建。Windows 全局热键、DWM 材质、系统语音和托盘行为需要在 Windows 11 实机验证。
