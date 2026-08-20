/* global require, process, __dirname, console */
/* eslint-disable @typescript-eslint/no-require-imports */
// Workaround for Tauri CLI selecting the wrong binary when the Rust package
// has additional bin targets (the sidecar mock). `tauri build` overwrites
// target/release/uniprog.exe with the mock binary on this machine,
// so this hook restores the real main binary right before bundling.
//
// The real main binary built by Cargo lives at:
//   target/release/deps/uniprog[.exe]
//
// Usage:
//   node scripts/fix-main-binary.cjs [--src-tauri <path>]

const fs = require('fs')
const path = require('path')

const exeName = process.platform === 'win32' ? 'uniprog.exe' : 'uniprog'
const mainName = process.platform === 'win32' ? 'uniprog.exe' : 'uniprog'

const argIndex = process.argv.indexOf('--src-tauri')
const srcTauri =
  argIndex >= 0
    ? path.resolve(process.argv[argIndex + 1])
    : path.resolve(__dirname, '..', 'src-tauri')

const roots = [srcTauri, path.resolve(srcTauri, '..')]

function firstExisting(...partsList) {
  for (const p of partsList) {
    if (fs.existsSync(p)) return p
  }
  return null
}

const src = firstExisting(...roots.map((r) => path.join(r, 'target', 'release', 'deps', exeName)))
const dst = firstExisting(...roots.map((r) => path.join(r, 'target', 'release', mainName)))

if (!src) {
  console.error('[fix-main-binary] real cargo binary not found:', exeName)
  process.exit(1)
}
if (!dst) {
  console.error('[fix-main-binary] main binary destination not found:', mainName)
  process.exit(1)
}

fs.copyFileSync(src, dst)
console.log(`[fix-main-binary] restored ${dst} from ${src}`)
