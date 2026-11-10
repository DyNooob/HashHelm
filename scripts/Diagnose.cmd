@echo off
setlocal DisableDelayedExpansion
title HashHelm startup diagnostics
echo HashHelm startup diagnostics
echo.
ver
echo.
set "HASHHELM_REPORT_DIR=%LOCALAPPDATA%\DigiForensics\HashHelm\diagnostics"
if not exist "%HASHHELM_REPORT_DIR%" mkdir "%HASHHELM_REPORT_DIR%"
set "HASHHELM_REPORT=%HASHHELM_REPORT_DIR%\startup-check.txt"
echo This report excludes task files and recovered passwords.
"%~dp0HashHelm.exe" --hashhelm-check > "%HASHHELM_REPORT%" 2>&1
set "HASHHELM_EXIT=%ERRORLEVEL%"
echo Exit code: %HASHHELM_EXIT%
echo Report: "%HASHHELM_REPORT%"
echo.
type "%HASHHELM_REPORT%"
echo.
if "%HASHHELM_EXIT%"=="-1073741515" echo A required DLL is missing. Extract the COMPLETE offline package.
if "%HASHHELM_EXIT%"=="-1073741701" echo The executable or a DLL has the wrong architecture. Use the x64 package on x64 Windows.
echo Keep the complete package together. Do not run the EXE inside an archive preview.
pause
endlocal
