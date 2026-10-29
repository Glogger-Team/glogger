@echo off
rem Run scripts/release.sh with Git's bash, without requiring bash on PATH.
rem Searches the standard Git for Windows install locations, then PATH.
setlocal
set "SEARCH=%~1"
if "%SEARCH%"=="" set "SEARCH=bash.exe"
where bash.exe >NUL 2>NUL
if not errorlevel 1 (
  bash.exe scripts/release.sh %*
  exit /b %errorlevel%
)
for %%G in (
  "%ProgramFiles%\Git\bin\bash.exe"
  "%ProgramFiles(x86)%\Git\bin\bash.exe"
) do if exist %%G (
  %%G scripts/release.sh %*
  exit /b %errorlevel%
)
echo Error: bash not found. Install Git for Windows or add its bin dir to PATH.
exit /b 1
