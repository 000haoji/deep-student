# 设置环境变量以使用MSVC
$vsPath = "${env:ProgramFiles}\Microsoft Visual Studio\2022\BuildTools"
if (!(Test-Path $vsPath)) {
    $vsPath = "${env:ProgramFiles}\Microsoft Visual Studio\2022\Community"
}
if (!(Test-Path $vsPath)) {
    $vsPath = "${env:ProgramFiles}\Microsoft Visual Studio\2022\Professional"
}
if (!(Test-Path $vsPath)) {
    $vsPath = "${env:ProgramFiles}\Microsoft Visual Studio\2022\Enterprise"
}

$vcvarsall = "$vsPath\VC\Auxiliary\Build\vcvarsall.bat"

if (Test-Path $vcvarsall) {
    Write-Host "找到 MSVC 工具链，正在配置环境..." -ForegroundColor Green
    & cmd /c "`"$vcvarsall`" x64 && set" | ForEach-Object {
        if ($_ -match "=") {
            $v = $_.split("=", 2)
            Set-Item -Force -Path "env:\$($v[0])" -Value $v[1]
        }
    }
    Write-Host "环境配置完成，开始编译..." -ForegroundColor Green
    
    # 进入 src-tauri 目录并编译
    Set-Location src-tauri
    cargo build --release
} else {
    Write-Host "错误：未找到 MSVC 工具链！" -ForegroundColor Red
    Write-Host "请确保已安装 Visual Studio Build Tools 2022 并选择了 C++ 工作负载。" -ForegroundColor Yellow
}
