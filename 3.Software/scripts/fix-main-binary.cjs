/* global require, process, __dirname, console */
/* eslint-disable @typescript-eslint/no-require-imports */
// Workaround for Tauri CLI selecting the wrong binary when the Rust package
// has additional bin targets (the sidecar mock). `tauri build` overwrites
// target/release/uniprog.exe with the mock binary on this machine,
// so this hook restores the real main binary right before bundling.
//
// The real main binary built by Cargo lives at:
//   target/release/deps/uniprog[.exe]

const fs = require('fs')
const path = require('path')

const exeName = process.platform === 'win32' ? 'uniprog.exe' : 'uniprog'
const mainName = process.platform === 'win32' ? 'uniprog.exe' : 'uniprog'

const roots = [
  path.resolve(process.cwd()),
  path.resolve(__dirname, '..'),
  path.resolve(__dirname, '..', 'src-tauri'),
]

function firstExisting(...partsList) {
  for (const p of partsList) {
    if (fs.existsSync(p)) return p
  }
  return null
}

const src = firstExisting(
  ...roots.map((r) => path.join(r, 'target', 'release', 'deps', exeName)),
  ...roots.map((r) => path.join(r, 'src-tauri', 'target', 'release', 'deps', exeName)),
)

const dst = firstExisting(
  ...roots.map((r) => path.join(r, 'target', 'release', mainName)),
  ...roots.map((r) => path.join(r, 'src-tauri', 'target', 'release', mainName)),
)

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
