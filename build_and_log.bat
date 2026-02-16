@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
cd /d C:\Users\cobra\axiom
cargo build 2>&1 > C:\Users\cobra\axiom\build_log.txt
echo EXIT:%ERRORLEVEL% >> C:\Users\cobra\axiom\build_log.txt
