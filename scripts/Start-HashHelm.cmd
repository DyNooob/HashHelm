@echo off
setlocal DisableDelayedExpansion
set "HASHHELM_REPORT_DIR=%LOCALAPPDATA%\DigiForensics\HashHelm\diagnostics"
if not exist "%HASHHELM_REPORT_DIR%" mkdir "%HASHHELM_REPORT_DIR%"
set "HASHHELM_REPORT=%HASHHELM_REPORT_DIR%\last-launch.txt"
echo Launching HashHelm. This window waits for the application to exit.
start "" /wait "%~dp0HashHelm.exe" > "%HASHHELM_REPORT%" 2>&1
set "HASHHELM_EXIT=%ERRORLEVEL%"
if "%HASHHELM_EXIT%"=="0" exit /b 0
echo HashHelm stopped with exit code %HASHHELM_EXIT%.
echo Report: "%HASHHELM_REPORT%"
type "%HASHHELM_REPORT%"
echo.
echo Please run Diagnose.cmd and keep the application logs for troubleshooting.
pause
endlocal
