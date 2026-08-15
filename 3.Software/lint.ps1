# UniProgrammer full lint check (Windows)
# Usage: powershell -ExecutionPolicy Bypass -File lint.ps1

$root = Split-Path -Parent $MyInvocation.MyCommand.Path

Push-Location $root
try {
    npm run lint
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run format:check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Push-Location (Join-Path $root 'src-tauri')
    try {
        cargo fmt --check
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        cargo lint
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }
} finally {
    Pop-Location
}
