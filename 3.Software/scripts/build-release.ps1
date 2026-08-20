# UniProgrammer release build wrapper.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Profile dll
#   powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Profile libusb
#
# Profiles:
#   libusb  GPL-compliant public build: only upt.hal.ch34x_libusb is shipped.
#   dll     local/maintainer build: only upt.hal.ch34x_dll (+ CH34X.DLL) is
#           shipped for friends who use the vendor driver.
#
# The whole build happens inside `build/<profile>/`; the repository source is
# never modified. Final artifacts land in `dist/<profile>/` (installer,
# portable zip, .unipkg packages and manifest.json).

param(
  [ValidateSet('dll', 'libusb')]
  [string]$Profile = 'libusb'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

Push-Location $root
try {
  node tools/build.mjs --profile "desktop-tauri-$Profile"
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
  Pop-Location
}
