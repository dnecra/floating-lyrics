@echo off
setlocal EnableDelayedExpansion

set "SCRIPT_DIR=%~dp0"
if "%SCRIPT_DIR:~-1%"=="\" set "SCRIPT_DIR=%SCRIPT_DIR:~0,-1%"
set "SRC_DIR=%SCRIPT_DIR%\src"
set "NSIS_OUT_DIR=%SCRIPT_DIR%\nsis"
set "TAURI_CONFIG="
set "BUILD_CONFIG="
set "TARGET_TRIPLE="
set "TARGET_ARG="
set "ARCH_LIST="
set "VCVARS_BAT="
set "VCVARS_ARCH="
set "VSWHERE_EXE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"

set "COMMAND=%~1"
set "MODE=%~2"
set "ARCH=%~3"

if "%COMMAND%"=="" goto :help
if /I "%COMMAND%"=="help" goto :help
if "%MODE%"=="" set "MODE=serverless"
if "%ARCH%"=="" set "ARCH=host"

if /I "%COMMAND%"=="dev" goto :dev
if /I "%COMMAND%"=="build" goto :build
if /I "%COMMAND%"=="dev-watch" goto :devwatch
goto :help

:resolve_source
if /I "%MODE%"=="serverless" set "SRC=%SRC_DIR%\serverless.rs" & set "TAURI_CONFIG=tauri.serverless.conf.json" & goto :eof
if /I "%MODE%"=="withserver" set "SRC=%SRC_DIR%\withserver.rs" & set "TAURI_CONFIG=tauri.withserver.conf.json" & goto :eof
if /I "%MODE%"=="standalone" set "SRC=%SRC_DIR%\standalone.rs" & set "TAURI_CONFIG=tauri.standalone.conf.json" & goto :eof
if /I "%MODE%"=="both" set "SRC=both" & goto :eof
if /I "%MODE%"=="all" set "SRC=all" & goto :eof
echo Invalid mode: %MODE%
exit /b 1

:resolve_target
if /I "%ARCH%"=="host" set "TARGET_TRIPLE=" & set "TARGET_ARG=" & goto :eof
if /I "%ARCH%"=="x64" set "TARGET_TRIPLE=x86_64-pc-windows-msvc" & goto :set_target_arg
if /I "%ARCH%"=="x86_64-pc-windows-msvc" set "TARGET_TRIPLE=%ARCH%" & goto :set_target_arg
echo Invalid architecture/target: %ARCH%
echo Supported values: host, x64, x86_64-pc-windows-msvc
exit /b 1

:set_target_arg
set "TARGET_ARG=--target %TARGET_TRIPLE%"
goto :eof

:set_arch_label
if not defined TARGET_TRIPLE (
  set "ARCH_LABEL=host"
  goto :eof
)
if /I "%TARGET_TRIPLE%"=="x86_64-pc-windows-msvc" set "ARCH_LABEL=x64" & goto :eof
set "ARCH_LABEL=%TARGET_TRIPLE%"
goto :eof

:ensure_target_installed
if not defined TARGET_TRIPLE goto :eof
rustup target list --installed | findstr /I /X /C:"%TARGET_TRIPLE%" >nul
if not errorlevel 1 goto :eof
echo Rust target %TARGET_TRIPLE% is not installed. Installing it now...
rustup target add %TARGET_TRIPLE%
if errorlevel 1 (
  echo Failed to install Rust target %TARGET_TRIPLE%.
  exit /b 1
)
goto :eof

:ensure_msvc_env
where link.exe >nul 2>nul
if not errorlevel 1 (
  if defined LIB goto :eof
)

if not exist "!VSWHERE_EXE!" (
  echo Could not find vswhere.exe at !VSWHERE_EXE!
  exit /b 1
)

for /f "usebackq tokens=*" %%I in (`"!VSWHERE_EXE!" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do (
  set "VSINSTALLDIR=%%I"
)

if not defined VSINSTALLDIR (
  echo Could not locate a Visual Studio installation with C++ build tools.
  exit /b 1
)

set "VCVARS_BAT=%VSINSTALLDIR%\VC\Auxiliary\Build\vcvarsall.bat"
if not exist "!VCVARS_BAT!" (
  echo Could not find vcvarsall.bat at !VCVARS_BAT!
  exit /b 1
)

call :set_vcvars_arch
if errorlevel 1 exit /b 1

echo Initializing MSVC build environment: %VCVARS_ARCH%
call "%VCVARS_BAT%" %VCVARS_ARCH% >nul
if errorlevel 1 (
  echo Failed to initialize MSVC build environment.
  exit /b 1
)
call :ensure_msvc_target_libs
if errorlevel 1 exit /b 1
goto :eof

:set_vcvars_arch
if not defined TARGET_TRIPLE (
  set "VCVARS_ARCH=amd64"
  goto :eof
)
if /I "%TARGET_TRIPLE%"=="x86_64-pc-windows-msvc" (
  set "VCVARS_ARCH=amd64"
  goto :eof
)
echo Unsupported MSVC target environment for %TARGET_TRIPLE%
exit /b 1

:ensure_msvc_target_libs
goto :eof

:build_current
call :resolve_target
if errorlevel 1 exit /b 1
call :ensure_target_installed
if errorlevel 1 exit /b 1
call :ensure_msvc_env
if errorlevel 1 exit /b 1
call :set_main
if errorlevel 1 exit /b 1
set "BUILD_CONFIG=%TAURI_CONFIG%"
if /I "%MODE%"=="standalone" (
  if /I "%TARGET_TRIPLE%"=="x86_64-pc-windows-msvc" set "BUILD_CONFIG=tauri.standalone.x64.conf.json"
)
cargo tauri build --config "%BUILD_CONFIG%" --features %MODE% %TARGET_ARG%
set "BUILD_ERR=%ERRORLEVEL%"
if errorlevel 1 exit /b 1
call :collect_nsis_artifacts
if errorlevel 1 exit /b 1
exit /b %BUILD_ERR%

:collect_nsis_artifacts

::collect_nsis_artifacts
:collect_nsis_artifacts_body
if not exist "%NSIS_OUT_DIR%" mkdir "%NSIS_OUT_DIR%"
if errorlevel 1 (
  echo Failed to create NSIS output directory: %NSIS_OUT_DIR%
  exit /b 1
)

if defined TARGET_TRIPLE (
  set "NSIS_SRC_DIR=%SCRIPT_DIR%\target\%TARGET_TRIPLE%\release\bundle\nsis"
) else (
  set "NSIS_SRC_DIR=%SCRIPT_DIR%\target\release\bundle\nsis"
)

if not exist "%NSIS_SRC_DIR%" (
  echo NSIS bundle directory not found: %NSIS_SRC_DIR%
  exit /b 1
)

copy /Y "%NSIS_SRC_DIR%\*.exe" "%NSIS_OUT_DIR%\" >nul
if errorlevel 1 (
  echo Failed to copy NSIS installers from %NSIS_SRC_DIR%
  exit /b 1
)

echo Copied NSIS installers to: %NSIS_OUT_DIR%
exit /b 0

:set_main
:set_main
if not exist "%SRC%" (
  echo Missing source variant file: %SRC%
  exit /b 1
)
copy /Y "%SRC%" "%SRC_DIR%\main.rs" >nul
if errorlevel 1 exit /b 1
echo Active main.rs variant: %MODE%
if defined TARGET_TRIPLE (
  echo Target triple: %TARGET_TRIPLE%
) else (
  echo Target triple: host default
)
exit /b 0

:dev
if /I "%MODE%"=="both" (
  echo dev supports only one mode at a time: serverless, withserver, or standalone.
  exit /b 1
)
if /I "%MODE%"=="all" (
  echo dev supports only one mode at a time: serverless, withserver, or standalone.
  exit /b 1
)
call :resolve_source
if errorlevel 1 exit /b 1
call :resolve_target
if errorlevel 1 exit /b 1
call :ensure_target_installed
if errorlevel 1 exit /b 1
call :ensure_msvc_env
if errorlevel 1 exit /b 1
call :set_main
if errorlevel 1 exit /b 1
pushd "%SCRIPT_DIR%"
cargo tauri dev --config "%TAURI_CONFIG%" --features %MODE% %TARGET_ARG%
set "ERR=%ERRORLEVEL%"
popd
exit /b %ERR%

:build
if /I "%MODE%"=="both" goto :build_all
if /I "%MODE%"=="all" goto :build_all
call :resolve_source
if errorlevel 1 exit /b 1
pushd "%SCRIPT_DIR%"
call :build_current
set "ERR=%ERRORLEVEL%"
popd
exit /b %ERR%

:build_all
  set "MODE=serverless"
  set "SRC=%SRC_DIR%\serverless.rs"
  set "TAURI_CONFIG=tauri.serverless.conf.json"
  pushd "%SCRIPT_DIR%"
  call :build_current
  if errorlevel 1 (
    set "ERR=%ERRORLEVEL%"
    popd
    exit /b %ERR%
  )
  set "MODE=withserver"
  set "SRC=%SRC_DIR%\withserver.rs"
  set "TAURI_CONFIG=tauri.withserver.conf.json"
  call :build_current
  if errorlevel 1 (
    set "ERR=%ERRORLEVEL%"
    popd
    exit /b %ERR%
  )
  set "MODE=standalone"
  set "SRC=%SRC_DIR%\standalone.rs"
  set "TAURI_CONFIG=tauri.standalone.conf.json"
  call :build_current
  set "ERR=%ERRORLEVEL%"
  popd
  exit /b %ERR%

:devwatch
if /I "%MODE%"=="both" (
  echo dev-watch supports only one mode at a time: serverless, withserver, or standalone.
  exit /b 1
)
if /I "%MODE%"=="all" (
  echo dev-watch supports only one mode at a time: serverless, withserver, or standalone.
  exit /b 1
)
call :resolve_source
if errorlevel 1 exit /b 1
call :resolve_target
if errorlevel 1 exit /b 1
call :ensure_target_installed
if errorlevel 1 exit /b 1
call :ensure_msvc_env
if errorlevel 1 exit /b 1
call :set_main
if errorlevel 1 exit /b 1
pushd "%SCRIPT_DIR%"
where cargo-watch >nul 2>nul
if errorlevel 1 (
  echo cargo-watch not found. Install with: cargo install cargo-watch
  echo Falling back to normal dev mode...
  cargo tauri dev --config "%TAURI_CONFIG%" --features %MODE% %TARGET_ARG%
) else (
  cargo watch -x "tauri dev --config %TAURI_CONFIG% --features %MODE% %TARGET_ARG%"
)
set "ERR=%ERRORLEVEL%"
popd
exit /b %ERR%

:help
echo Usage:
echo   tauri.cmd dev serverless [host^|x64]
echo   tauri.cmd dev withserver [host^|x64]
echo   tauri.cmd dev standalone [host^|x64]
echo   tauri.cmd build serverless [host^|x64]
echo   tauri.cmd build withserver [host^|x64]
echo   tauri.cmd build standalone [host^|x64]
echo   tauri.cmd build both [host^|x64]
echo   tauri.cmd build all [host^|x64]
echo   tauri.cmd dev-watch serverless [host^|x64]
echo   tauri.cmd dev-watch withserver [host^|x64]
echo   tauri.cmd dev-watch standalone [host^|x64]
echo.
echo Examples:
echo   tauri.cmd build standalone x64
echo   tauri.cmd build all x64
exit /b 1
