@echo off
setlocal
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust and Cargo must be installed and available on PATH.
    exit /b 1
)
if not defined CARGO_HOME if exist "%~dp0.cargo-home" set "CARGO_HOME=%~dp0.cargo-home"
if not defined VSCMD_VER (
    if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" (
        for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do (
            call "%%i\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 -no_logo
            if errorlevel 1 exit /b 1
        )
    )
)
pushd "%~dp0"
if errorlevel 1 exit /b 1
if "%~1"=="" (
    cargo build --workspace --release --locked
) else (
    cargo %*
)
set "CARGO_RESULT=%errorlevel%"
popd
exit /b %CARGO_RESULT%
