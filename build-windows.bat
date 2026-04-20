@echo off
setlocal

echo ============================================
echo   Bibelsuche Windows Build Starter
echo ============================================
echo.

where node >nul 2>nul
if errorlevel 1 (
  echo [FEHLER] Node.js ist nicht installiert.
  echo Installiere Node.js LTS: https://nodejs.org/
  pause
  exit /b 1
)

where cargo >nul 2>nul
if errorlevel 1 (
  echo [FEHLER] Rust ist nicht installiert.
  echo Installiere Rust: https://rustup.rs/
  pause
  exit /b 1
)

echo [1/3] npm install
call npm install
if errorlevel 1 (
  echo [FEHLER] npm install fehlgeschlagen.
  pause
  exit /b 1
)

echo [2/3] Tauri Build starten
call npm run tauri build
if errorlevel 1 (
  echo [FEHLER] Windows Build fehlgeschlagen.
  echo.
  echo Tipp:
  echo - Visual Studio Build Tools (C++ workload) installieren
  echo - Danach neues Terminal oeffnen und erneut ausfuehren
  pause
  exit /b 1
)

echo [3/3] Fertig. Oeffne Bundle-Ordner...
set BUNDLE_DIR=%~dp0src-tauri\target\release\bundle
if exist "%BUNDLE_DIR%" (
  explorer "%BUNDLE_DIR%"
)

echo.
echo Erfolg. Die fertige Datei ist normalerweise in:
echo - src-tauri\target\release\bundle\msi\
echo - oder src-tauri\target\release\bundle\nsis\
echo.
pause

