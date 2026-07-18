@echo off
rem Launch Creatio DevHub in dev mode inside the VS BuildTools environment.
rem Needed because this machine's VS Community has a partial MSVC toolset
rem (no libs) that the Rust toolchain would otherwise pick up.
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0"
npm run tauri dev
