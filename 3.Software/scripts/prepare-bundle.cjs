/* global require, process, __dirname, console */
/* eslint-disable @typescript-eslint/no-require-imports */
// Tauri `beforeBundleCommand`: runs after the cargo release build and before
// bundling. It restores the real main binary (see fix-main-binary.cjs) and
// copies sidecar binaries into their plugin packages so `plugins/` can be
// bundled as one self-contained resource tree.

const { spawnSync } = require('node:child_process')
const path = require('node:path')

const scripts = [
  [process.execPath, [path.join(__dirname, 'fix-main-binary.cjs')]],
  [process.execPath, [path.join(__dirname, 'copy-sidecar-binaries.cjs'), '--release']],
]

for (const [command, args] of scripts) {
  const result = spawnSync(command, args, { stdio: 'inherit' })
  if (result.error) {
    console.error(`[prepare-bundle] failed to run ${path.basename(args[0])}:`, result.error)
    process.exit(1)
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}
