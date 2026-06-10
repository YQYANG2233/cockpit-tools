@echo off
setlocal

set SCRIPT_DIR=%~dp0
set SERVICE_EXE=%SCRIPT_DIR%target\release\cockpit-service.exe
set WEB_EXE=%SCRIPT_DIR%target\release\cockpit-web.exe
set WEB_ROOT=%SCRIPT_DIR%dist

echo ========================================
echo   Cockpit Tools Web Server
echo ========================================
echo.

:: Check binaries exist
if not exist "%SERVICE_EXE%" (
    echo ERROR: cockpit-service.exe not found. Run build first.
    echo   cargo build --release --package cockpit-service
    pause
    exit /b 1
)
if not exist "%WEB_EXE%" (
    echo ERROR: cockpit-web.exe not found. Run build first.
    echo   cargo build --release --package cockpit-web
    pause
    exit /b 1
)
if not exist "%WEB_ROOT%\index.html" (
    echo ERROR: Frontend not built. Run build first.
    echo   npm install ^&^& npm run build
    pause
    exit /b 1
)

:: Kill existing processes first
echo [0/2] Stopping any existing instances...
taskkill /F /IM cockpit-service.exe >nul 2>&1
taskkill /F /IM cockpit-web.exe >nul 2>&1
timeout /t 1 /nobreak >nul

:: Start service in background
echo [1/2] Starting cockpit-service on 127.0.0.1:19529...
start /b "" "%SERVICE_EXE%"
timeout /t 3 /nobreak >nul

:: Start web gateway
echo [2/2] Starting cockpit-web on 127.0.0.1:18082...
set COCKPIT_TOOLS_WEB_ROOT=%WEB_ROOT%
start /b "" "%WEB_EXE%"
timeout /t 2 /nobreak >nul

echo.
echo ========================================
echo   Cockpit Tools is running!
echo   Open http://127.0.0.1:18082 in browser
echo ========================================
echo.
echo Press any key to stop all services...
echo.

:: Wait for user to press a key
pause >nul

:: Cleanup
echo Stopping services...
taskkill /F /IM cockpit-service.exe >nul 2>&1
taskkill /F /IM cockpit-web.exe >nul 2>&1
echo Done.
