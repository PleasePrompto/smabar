@echo off
rem The ONLY way to start smabar in dev on Windows: kills every stray
rem instance first (including an installed bar — there is only ever ONE
rem smabar per desktop), waits until the vite port is free again, then
rem starts exactly one dev instance. Windows mirror of dev.sh.
setlocal
cd /d "%~dp0.."
set "REPO=%CD%"

rem 1. Stop the tauri dev CLI and this repo's vite first (they reap or
rem    orphan their children), then any smabar binary that is left.
powershell -NoProfile -Command "Get-CimInstance Win32_Process | Where-Object { ($_.CommandLine -match 'tauri' -and $_.CommandLine -match '\bdev\b') -or ($_.CommandLine -match 'vite' -and $_.CommandLine -like ('*' + $env:REPO + '*')) } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }"
taskkill /F /IM smabar.exe >nul 2>&1

rem 2. Wait until port 5173 is free and no smabar process is left (max 10 s
rem    each); a stale vite serves a stale module graph, a stale bar would
rem    collide with the single-instance guard.
powershell -NoProfile -Command "for ($i = 0; $i -lt 20; $i++) { if (-not (Get-NetTCPConnection -LocalPort 5173 -State Listen -ErrorAction SilentlyContinue)) { break }; Start-Sleep -Milliseconds 500 }"
powershell -NoProfile -Command "for ($i = 0; $i -lt 20; $i++) { if (-not (Get-Process smabar -ErrorAction SilentlyContinue)) { exit 0 }; Start-Sleep -Milliseconds 500 }; exit 1"
if errorlevel 1 (
  echo error: stale smabar process still running — refusing to start a second instance 1>&2
  exit /b 1
)

echo clean — starting single dev instance
rem Dev Python tooling (bundled builds resolve both beside/from resources).
set "SMABAR_SDK_PATH=%REPO%\sdk\python"
if not defined SMABAR_UV (
  if exist "%REPO%\crates\smabar\binaries\uv-x86_64-pc-windows-msvc.exe" (
    set "SMABAR_UV=%REPO%\crates\smabar\binaries\uv-x86_64-pc-windows-msvc.exe"
  ) else (
    for /f "delims=" %%U in ('where uv 2^>nul') do if not defined SMABAR_UV set "SMABAR_UV=%%U"
  )
)
cd /d "%REPO%\shell"
bun run dev:app %*
