# 切换到 GNU 工具链而不是 MSVC
Write-Host "切换到 GNU 工具链..." -ForegroundColor Yellow

# 安装 GNU 工具链
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu

Write-Host "GNU 工具链已设置为默认" -ForegroundColor Green
Write-Host "现在可以尝试重新编译项目" -ForegroundColor Green 