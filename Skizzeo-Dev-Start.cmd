@echo off
rem Startet die lokale Entwicklung: baut bei jeder Änderung neu, Browser lädt selbst nach.
cd /d "%~dp0"
start "" http://localhost:8080
python scripts\dev.py
pause
