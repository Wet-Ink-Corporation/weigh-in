@echo off
setlocal
set "D=%~dp0"
if exist "%D%x86_64-pc-windows-msvc\weighin.exe" (
  "%D%x86_64-pc-windows-msvc\weighin.exe" %*
  exit /b
)
if exist "%D%x86_64-pc-windows-gnu\weighin.exe" (
  "%D%x86_64-pc-windows-gnu\weighin.exe" %*
  exit /b
)
weighin.exe %*
