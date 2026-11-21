@echo off
title rooPlayer Audiophile Media Hub (Rust Native)
chcp 65001 > nul
cd /d "%~dp0"
echo ==========================================================
echo   正在啟動 rooPlayer (Rust 發燒級純原生數位串流播放器)...
echo ==========================================================

if exist "%~dp0target\release\rooplayer.exe" (
    start "" "%~dp0target\release\rooplayer.exe"
    exit
) else if exist "%~dp0target\debug\rooplayer.exe" (
    start "" "%~dp0target\debug\rooplayer.exe"
    exit
) else (
    cargo run -p rooplayer --release
)
