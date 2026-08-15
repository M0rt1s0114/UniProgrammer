#!/usr/bin/env bash
# UniProgrammer 编译选单（Linux / macOS）
# 选择 HAL 后端后构建前端 + Rust release。
set -euo pipefail
cd "$(dirname "$0")"

echo
echo '  UniProgrammer build menu'
echo '  ------------------------'
echo '  [1] 自动（本平台默认：Windows=DLL，Linux=libusb）'
echo '  [2] 强制 libusb（rusb）'
echo '  [3] 强制 DLL（CH34X.DLL，仅 Windows 有意义）'
echo
read -r -p '  选择后端 (1/2/3): ' choice

features=()
case "$choice" in
  1) ;;
  2) features=(--features hal-libusb) ;;
  3) features=(--features hal-dll) ;;
  *) echo '无效选择' >&2; exit 1 ;;
esac

echo
echo '>>> npm run build'
npm run build

echo
echo ">>> cargo build --release ${features[*]}"
(cd src-tauri && cargo build --release "${features[@]}")

echo
echo '完成。可执行文件：'
echo '  src-tauri/target/release/chip-validator'
