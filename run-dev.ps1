Write-Host "启动 AI 错题管理系统开发服务器..." -ForegroundColor Green
Write-Host ""
npm run tauri dev
Write-Host ""
Write-Host "按任意键退出..." -ForegroundColor Yellow
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown") 