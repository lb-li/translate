$ErrorActionPreference = "Stop"

Write-Host "[1/4] 检查 Rust 格式..." -ForegroundColor Cyan
cargo fmt --all -- --check

Write-Host "[2/4] 运行离线单元测试..." -ForegroundColor Cyan
cargo test --workspace --locked

Write-Host "[3/4] Release 优化编译 app-ui..." -ForegroundColor Cyan
cargo build --release -p app-ui --locked

Write-Host "[4/4] 打包至 dist\lumen-translate.exe..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path "dist" | Out-Null
Copy-Item "target\release\app-ui.exe" "dist\lumen-translate.exe" -Force

Write-Host "完成。运行 .\dist\lumen-translate.exe，划选文字后按 Alt+Q。" -ForegroundColor Green
