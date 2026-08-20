# UniProgrammer full lint check (Windows)
# Usage: powershell -ExecutionPolicy Bypass -File lint.ps1

$root = Split-Path -Parent $MyInvocation.MyCommand.Path

Push-Location $root
try {
    npm run lint
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run format:check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    # Assemble the GPL profile and run fmt/check/clippy/test there.
    node tools/verify.mjs --profile desktop-tauri-libusb
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
