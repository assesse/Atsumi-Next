@echo off
setlocal

set "ATSUMI_PROJECT_ROOT=%~dp0"
set "ATSUMI_POWERSHELL=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
set "ATSUMI_REVIEW_URL=http://127.0.0.1:1420"
set "ATSUMI_EDGE=C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"

pushd "%ATSUMI_PROJECT_ROOT%" >nul 2>&1
if errorlevel 1 (
  echo [Atsumi Next] Project folder could not be opened:
  echo %ATSUMI_PROJECT_ROOT%
  pause
  exit /b 1
)

echo [Atsumi Next] Starting the browser fixture review environment...
echo [Atsumi Next] The minimized server window must stay open while reviewing.

start "Atsumi Next Review Server" /min "%ATSUMI_POWERSHELL%" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%ATSUMI_PROJECT_ROOT%tools\run_frontend.ps1" dev

"%ATSUMI_POWERSHELL%" -NoLogo -NoProfile -ExecutionPolicy Bypass -Command ^
  "$deadline = (Get-Date).AddSeconds(20); do { try { $response = Invoke-WebRequest -UseBasicParsing -Uri '%ATSUMI_REVIEW_URL%' -TimeoutSec 1; if ($response.StatusCode -eq 200) { exit 0 } } catch {}; Start-Sleep -Milliseconds 200 } while ((Get-Date) -lt $deadline); exit 1"

if errorlevel 1 (
  echo [Atsumi Next] Review server did not become ready at %ATSUMI_REVIEW_URL%.
  pause
  popd
  exit /b 1
)

if exist "%ATSUMI_EDGE%" (
  start "" "%ATSUMI_EDGE%" --app="%ATSUMI_REVIEW_URL%"
) else (
  start "" "%ATSUMI_REVIEW_URL%"
)

popd
exit /b 0
