# UniProgrammer 编译选单（Windows PowerShell）
# 选择 HAL 后端后构建前端 + Rust release。

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

Write-Host ''
Write-Host '  UniProgrammer build menu'
Write-Host '  ------------------------'
Write-Host '  [1] 自动（本平台默认：Windows=DLL，Linux=libusb）'
Write-Host '  [2] 强制 libusb（rusb）'
Write-Host '  [3] 强制 DLL（CH34X.DLL，仅 Windows 有意义）'
Write-Host ''
$choice = Read-Host '  选择后端 (1/2/3)'
$features = ''
switch ($choice) {
  '1' { $features = '' }
  '2' { $features = '--features hal-libusb' }
  '3' { $features = '--features hal-dll' }
  default { Write-Host '无效选择'; exit 1 }
}

Write-Host ''
Write-Host '>>> npm run build'
npm run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ''
Write-Host ">>> cargo build --release $features"
Push-Location src-tauri
try {
  $cmd = "cargo build --release $features"
  Invoke-Expression $cmd
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
  Pop-Location
}

Write-Host ''
Write-Host '完成。可执行文件：'
Write-Host '  src-tauri\target\release\chip-validator.exe'
Write-Host '  （同目录需要 CH34X.DLL 与 chiplib.bin）'
