# Cockpit Tools Web Server - PowerShell Start Script
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

$ServiceExe = Join-Path $ScriptDir "target\release\cockpit-service.exe"
$WebExe = Join-Path $ScriptDir "target\release\cockpit-web.exe"
$WebRoot = Join-Path $ScriptDir "dist"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Cockpit Tools Web Server" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check binaries
if (-not (Test-Path $ServiceExe)) {
    Write-Host "ERROR: cockpit-service.exe not found. Run build first." -ForegroundColor Red
    Write-Host "  cargo build --release --package cockpit-service"
    exit 1
}
if (-not (Test-Path $WebExe)) {
    Write-Host "ERROR: cockpit-web.exe not found. Run build first." -ForegroundColor Red
    Write-Host "  cargo build --release --package cockpit-web"
    exit 1
}
if (-not (Test-Path "$WebRoot\index.html")) {
    Write-Host "ERROR: Frontend not built. Run build first." -ForegroundColor Red
    Write-Host "  npm install && npm run build"
    exit 1
}

# Kill existing processes
Write-Host "[0/2] Stopping any existing instances..." -ForegroundColor Yellow
Get-Process -Name "cockpit-service","cockpit-web" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

# Start service
Write-Host "[1/2] Starting cockpit-service on 127.0.0.1:19529..." -ForegroundColor Yellow
$serviceJob = Start-Job -ScriptBlock {
    param($exe)
    & $exe 2>&1
} -ArgumentList $ServiceExe

Start-Sleep -Seconds 3

# Start web gateway
Write-Host "[2/2] Starting cockpit-web on 127.0.0.1:18082..." -ForegroundColor Yellow
$env:COCKPIT_TOOLS_WEB_ROOT = $WebRoot
$webJob = Start-Job -ScriptBlock {
    param($exe, $root)
    $env:COCKPIT_TOOLS_WEB_ROOT = $root
    & $exe 2>&1
} -ArgumentList $WebExe, $WebRoot

Start-Sleep -Seconds 2

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Cockpit Tools is running!" -ForegroundColor Green
Write-Host "  Open http://127.0.0.1:18082 in browser" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "Press any key to stop all services..." -ForegroundColor Yellow
Write-Host ""

# Wait for key press
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")

# Cleanup
Write-Host "Stopping services..." -ForegroundColor Yellow
Stop-Job -Job $serviceJob, $webJob -ErrorAction SilentlyContinue
Remove-Job -Job $serviceJob, $webJob -Force -ErrorAction SilentlyContinue
Get-Process -Name "cockpit-service","cockpit-web" -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host "Done." -ForegroundColor Green
