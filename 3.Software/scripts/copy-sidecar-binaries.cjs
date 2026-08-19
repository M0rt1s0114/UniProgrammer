/* global require, process, __dirname, console */
/* eslint-disable @typescript-eslint/no-require-imports */
// Copies sidecar binaries from `src-tauri/target/<profile>/` into their
// built-in plugin package directories so the packages stay self-contained.
//
// - `npm run dev` calls this for the debug profile before starting Vite.
// - `scripts/prepare-bundle.cjs` calls it with `--release` after the cargo
//   release build and before Tauri bundles the `plugins/` resource directory.

const fs = require('node:fs')
const path = require('node:path')

const profile = process.argv.includes('--release') ? 'release' : 'debug'
const srcTauri = path.resolve(__dirname, '..', 'src-tauri')
const targetDir = path.join(srcTauri, 'target', profile)
const pluginsDir = path.join(srcTauri, 'plugins', 'builtin')

const exeSuffix = process.platform === 'win32' ? '.exe' : ''

const copies = [
  {
    name: 'uni_ch34x_sidecar_dll',
    packageName: 'uni.hal.ch34x_dll',
  },
  {
    name: 'uni_ch34x_sidecar_libusb',
    packageName: 'uni.hal.ch34x_libusb',
  },
]

let failures = 0

for (const { name, packageName } of copies) {
  const source = path.join(targetDir, `${name}${exeSuffix}`)
  const destination = path.join(pluginsDir, packageName, `${name}${exeSuffix}`)
  if (!fs.existsSync(source)) {
    console.log(`[copy-sidecars] skip ${name}: ${source} not built yet`)
    continue
  }
  try {
    fs.copyFileSync(source, destination)
    console.log(`[copy-sidecars] ${name} -> ${destination}`)
  } catch (error) {
    failures += 1
    console.error(`[copy-sidecars] failed to copy ${name}:`, error)
  }
}

// The vendor CH34X.DLL is only present on machines that already have it.
// It belongs to the dll-backend package only; the libusb package must stay
// DLL-free.
const dllSource = path.join(srcTauri, 'CH34X.DLL')
if (fs.existsSync(dllSource)) {
  const dllDestination = path.join(pluginsDir, 'uni.hal.ch34x_dll', 'CH34X.DLL')
  try {
    fs.copyFileSync(dllSource, dllDestination)
    console.log(`[copy-sidecars] CH34X.DLL -> ${dllDestination}`)
  } catch (error) {
    failures += 1
    console.error('[copy-sidecars] failed to copy CH34X.DLL:', error)
  }
}

if (failures > 0) {
  process.exitCode = 1
}
