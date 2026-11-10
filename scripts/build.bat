@echo off
rem ============================================================================
rem  build.bat - build cvewatch.exe, cvewatchw.exe and cvewatch-gui.exe
rem
rem  Usage: scripts\build.bat [check] [debug] [nodist]
rem    check   run cargo fmt --check, clippy -D warnings and cargo test first
rem    debug   build with the debug profile (default: release)
rem    nodist  do not copy the executables to dist\
rem
rem  Output:
rem    target\<profile>\cvewatch.exe, cvewatchw.exe, cvewatch-gui.exe
rem    dist\  the same three files, ready to copy to the target machine
rem           (they must stay in the same folder: the GUI and the scheduled
rem           tasks run the sibling cvewatch.exe / cvewatchw.exe)
rem
rem  The first build compiles the vendored OpenSSL for SQLCipher (about 5 min)
rem  and needs Strawberry Perl; see docs/development.md (build section).
rem
rem  Keep this file ASCII-only: cmd parses batch files incrementally and
rem  misreads multi-byte UTF-8 text when the code page differs.
rem ============================================================================
setlocal EnableExtensions

for %%I in ("%~dp0..") do set "ROOT=%%~fI"
set "DIST_DIR=%ROOT%\dist"
set "PROFILE=release"
set "CARGO_PROFILE_FLAG=--release"
set "DO_CHECK="
set "NO_DIST="
set "EXES=cvewatch.exe cvewatchw.exe cvewatch-gui.exe"

:parse_args
if "%~1"=="" goto args_done
if /i "%~1"=="check"  (set "DO_CHECK=1" & shift & goto parse_args)
if /i "%~1"=="debug"  (set "PROFILE=debug" & set "CARGO_PROFILE_FLAG=" & shift & goto parse_args)
if /i "%~1"=="nodist" (set "NO_DIST=1" & shift & goto parse_args)
if /i "%~1"=="help"   goto usage
if /i "%~1"=="-h"     goto usage
if /i "%~1"=="/?"     goto usage
echo [ERROR] Unknown argument: %~1
goto usage_error

:args_done
set "EXE_DIR=%ROOT%\target\%PROFILE%"

where cargo >nul 2>&1 || (echo [ERROR] cargo not found. Install the Rust MSVC toolchain first. & exit /b 1)

rem SQLCipher's vendored OpenSSL needs Strawberry Perl, not Git for Windows' MSYS perl.
if not defined PERL if exist "C:\Strawberry\perl\bin\perl.exe" set "PERL=C:\Strawberry\perl\bin\perl.exe"
call :check_perl || exit /b 1

pushd "%ROOT%" || exit /b 1

rem --- 1. Checks (optional) ----------------------------------------------------
if not defined DO_CHECK goto build
echo [1/3] cargo fmt --check
cargo fmt --check
if errorlevel 1 (echo [ERROR] Formatting differs. Run "cargo fmt". & popd & exit /b 1)
echo [1/3] cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets -- -D warnings
if errorlevel 1 (echo [ERROR] clippy reported problems. & popd & exit /b 1)
echo [1/3] cargo test
cargo test
if errorlevel 1 (echo [ERROR] Tests failed. & popd & exit /b 1)

rem --- 2. Build ----------------------------------------------------------------
:build
echo [2/3] Building cvewatch, cvewatchw and cvewatch-gui (%PROFILE%) ...
cargo build %CARGO_PROFILE_FLAG% -p cvewatch -p cvewatch-gui
set "BUILD_RC=%ERRORLEVEL%"
popd
if not "%BUILD_RC%"=="0" (echo [ERROR] Build failed, exit code %BUILD_RC%. & exit /b %BUILD_RC%)

rem No ( ) blocks around paths: a ")" in the path, e.g. "Program Files (x86)", would end them early.
for %%E in (%EXES%) do call :require "%EXE_DIR%\%%E" || exit /b 1

rem --- 3. Copy to dist\ ----------------------------------------------------------
if defined NO_DIST goto no_dist
if not exist "%DIST_DIR%" mkdir "%DIST_DIR%" || exit /b 1
echo [3/3] Copying executables to "%DIST_DIR%"
for %%E in (%EXES%) do call :copy_one %%E || exit /b 1
echo Done. Copy the three files in "%DIST_DIR%" to the same folder on the target machine.
exit /b 0

:no_dist
echo [3/3] Skipped dist\ (nodist). Executables are in "%EXE_DIR%"
exit /b 0

:check_perl
rem openssl-src runs %PERL% when it is set, otherwise the first perl on PATH.
set "PERL_EXE=%PERL%"
if defined PERL_EXE goto check_perl_exists
for /f "delims=" %%P in ('where perl 2^>nul') do if not defined PERL_EXE set "PERL_EXE=%%P"
if defined PERL_EXE goto check_perl_exists
echo [ERROR] perl not found. Building SQLCipher's vendored OpenSSL needs Strawberry Perl:
echo           winget install --id StrawberryPerl.StrawberryPerl -e
echo         then open a new terminal, or set PERL to the full path of perl.exe.
exit /b 1
:check_perl_exists
if exist "%PERL_EXE%" goto check_perl_native
echo [ERROR] PERL points to a missing file: "%PERL_EXE%"
exit /b 1
:check_perl_native
rem OpenSSL's MSVC build rejects MSYS/Cygwin perl (e.g. Git for Windows' usr\bin\perl.exe).
"%PERL_EXE%" -e "exit($^O eq 'MSWin32' ? 0 : 1)" >nul 2>&1 && exit /b 0
echo [ERROR] "%PERL_EXE%" is not a native Windows perl; OpenSSL's MSVC build needs Strawberry Perl:
echo           winget install --id StrawberryPerl.StrawberryPerl -e
echo         then set PERL to the full path of Strawberry's perl.exe.
exit /b 1

:require
if exist "%~1" exit /b 0
echo [ERROR] Missing build output: "%~1"
exit /b 1

:copy_one
copy /y "%EXE_DIR%\%~1" "%DIST_DIR%\%~1" >nul || goto copy_failed
echo       %~1
exit /b 0
:copy_failed
echo [ERROR] Could not copy %~1. Close any running cvewatch program and retry.
exit /b 1

:usage
echo Usage: scripts\build.bat [check] [debug] [nodist]
echo   check   run cargo fmt --check, clippy -D warnings and cargo test first
echo   debug   build with the debug profile (default: release)
echo   nodist  do not copy the executables to dist\
exit /b 0

:usage_error
echo Usage: scripts\build.bat [check] [debug] [nodist]
exit /b 2
