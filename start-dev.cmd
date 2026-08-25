@echo off
setlocal

set "ATSUMI_PROJECT_ROOT=%~dp0"
set "ATSUMI_POWERSHELL=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"

pushd "%ATSUMI_PROJECT_ROOT%" >nul 2>&1
if errorlevel 1 (
  echo [Atsumi Next] Project folder could not be opened:
  echo %ATSUMI_PROJECT_ROOT%
  pause
  exit /b 1
)

echo [Atsumi Next] Starting the review environment...
echo [Atsumi Next] Keep this window open while reviewing the app.
echo.

"%ATSUMI_POWERSHELL%" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%ATSUMI_PROJECT_ROOT%tools\run_frontend.ps1" tauri dev
set "ATSUMI_EXIT_CODE=%ERRORLEVEL%"

if not "%ATSUMI_EXIT_CODE%"=="0" (
  echo.
  echo [Atsumi Next] The development app could not be started. Exit code: %ATSUMI_EXIT_CODE%
  echo See the error above, then press any key to close this window.
  pause >nul
)

popd
exit /b %ATSUMI_EXIT_CODE%
