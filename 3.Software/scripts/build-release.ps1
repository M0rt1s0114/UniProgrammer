# UniProgrammer release build wrapper.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Profile dll
#   powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Profile libusb
#
# Profiles:
#   libusb  GPL-compliant public build: only uni.hal.ch34x_libusb is shipped,
#           CH34X.DLL / uni.hal.ch34x_dll are kept out of the bundle.
#   dll     local/maintainer build: only uni.hal.ch34x_dll (+ CH34X.DLL) is
#           shipped for friends who use the vendor driver.
#
# The non-selected plugin package is temporarily moved out of
# `plugins/builtin/` during `tauri build` and restored afterwards, so the
# bundled resource tree contains exactly one CH34X backend.

param(
  [ValidateSet('dll', 'libusb')]
  [string]$Profile = 'libusb'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$builtin = Join-Path $root 'src-tauri\plugins\builtin'
$hold = Join-Path $root 'src-tauri\plugins-hold'

$hide = if ($Profile -eq 'libusb') { 'uni.hal.ch34x_dll' } else { 'uni.hal.ch34x_libusb' }
$source = Join-Path $builtin $hide
$target = Join-Path $hold $hide

if (-not (Test-Path $source)) {
  throw "Plugin package not found: $source"
}

try {
  New-Item -ItemType Directory -Force -Path $hold | Out-Null
  if (Test-Path $target) { Remove-Item $target -Recurse -Force }
  Move-Item $source $target
  Write-Output "[build-release] profile=$Profile hidden=$hide"

  Push-Location $root
  try {
    if ($Profile -eq 'libusb') {
      npx tauri build --features hal-libusb --config src-tauri/tauri.libusb.conf.json
    } else {
      npx tauri build
    }
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  } finally {
    Pop-Location
  }
} finally {
  if (Test-Path $target) {
    if (Test-Path $source) { Remove-Item $source -Recurse -Force }
    Move-Item $target $source
  }
  if (Test-Path $hold) {
    Remove-Item $hold -Force -ErrorAction SilentlyContinue
  }
  Write-Output "[build-release] plugins restored"
}
