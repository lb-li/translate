# 一键编译打包 Windows 可执行文件 dist\lumen-translate.exe
$ErrorActionPreference = "Stop"

Write-Host "[1/3] 运行核心模块单元测试..." -ForegroundColor Cyan
cargo test --workspace

Write-Host "[2/3] Release 优化编译 app-ui..." -ForegroundColor Cyan
cargo build --release -p app-ui

Write-Host "[3/3] 打包至 dist\lumen-translate.exe..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path "dist" | Out-Null
Copy-Item "target\release\app-ui.exe" "dist\lumen-translate.exe" -Force

Write-Host "完成！可直接双击或运行 .\dist\lumen-translate.exe 测试（默认按 Alt+Q 划词，托盘图标驻留）。" -ForegroundColor Green
