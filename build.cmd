@echo off
rem Build the production installer (NSIS/MSI) inside the VS BuildTools environment.
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set "TAURI_SIGNING_PRIVATE_KEY=%USERPROFILE%\.tauri\creatio-devhub.key"
cd /d "%~dp0"
npm run tauri build
