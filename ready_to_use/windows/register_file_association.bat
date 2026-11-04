@echo off
:: Reddart Video Editor - Windows File Association Installer
:: Associates .reddart project files with Reddart.exe

echo [Reddart] Registering .reddart file association...

set "APP_PATH=%~dp0Reddart.exe"

if not exist "%APP_PATH%" (
    echo Error: Reddart.exe not found in %~dp0
    pause
    exit /b 1
)

assoc .reddart=Reddart.Project
ftype Reddart.Project="%APP_PATH%" "%%1"

echo [Reddart] Association registered successfully!
echo You can now double-click any .reddart file to open it in Reddart.
pause

