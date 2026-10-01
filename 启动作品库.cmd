@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"
title AI Gallery

if not exist "package.json" (
  echo [ERROR] package.json was not found. Keep this script in the project root.
  goto :failed
)

where node >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Node.js 22 or newer is required.
  goto :failed
)

where cargo >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Rust and Cargo are required.
  goto :failed
)

if not exist "node_modules\@tauri-apps\cli" (
  echo [FIRST RUN] Installing project dependencies...
  call npm ci --no-audit --no-fund
  if errorlevel 1 goto :failed
)

if /i "%~1"=="--check" (
  echo [OK] Runtime and project dependencies are ready.
  exit /b 0
)

echo [START] Opening AI Gallery...
call npm run desktop:dev
if errorlevel 1 goto :failed

exit /b 0

:failed
echo.
echo Startup failed. Keep the error messages shown above.
pause
exit /b 1
