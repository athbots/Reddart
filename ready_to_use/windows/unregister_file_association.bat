@echo off
:: Reddart Video Editor - Windows File Association Uninstaller
:: Removes .reddart project file associations

echo [Reddart] Removing .reddart file association...

assoc .reddart=
ftype Reddart.Project=

echo [Reddart] File association removed successfully.
pause

