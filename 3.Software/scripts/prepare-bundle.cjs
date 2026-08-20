/* global require, process, __dirname, console */
/* eslint-disable @typescript-eslint/no-require-imports */
// Tauri `beforeBundleCommand`: runs after the cargo release build and before
// bundling.
//
// 1. Builds both CH34X sidecar backends (vendor DLL + libusb).
// 2. Restores the real main binary (see fix-main-binary.cjs).
// 3. Copies sidecar binaries into the plugin packages that are about to be
//    bundled.
//
// Usage:
//   node scripts/prepare-bundle.cjs --release [--src-tauri <path>]

const { spawnSync } = require('node:child_process')
const path = require('node:path')

const root = path.resolve(__dirname, '..')
const argIndex = process.argv.indexOf('--src-tauri')
const srcTauri =
  argIndex >= 0 ? path.resolve(process.argv[argIndex + 1]) : path.resolve(root, 'src-tauri')
const cargoManifest = path.join(srcTauri, 'Cargo.toml')
const release = process.argv.includes('--release')

const cargoTargets = [
  { feature: 'hal-dll', bin: 'upt_ch34x_sidecar_dll' },
  { feature: 'hal-libusb', bin: 'upt_ch34x_sidecar_libusb' },
]

for (const { feature, bin } of cargoTargets) {
  const result = spawnSync(
    'cargo',
    [
      'build',
      ...(release ? ['--release'] : []),
      '--manifest-path',
      cargoManifest,
      '-p',
      'upt-devices',
      '--features',
      feature,
      '--bin',
      bin,
    ],
    { cwd: srcTauri, stdio: 'inherit' },
  )
  if (result.error) {
    console.error(`[prepare-bundle] failed to run cargo for ${bin}:`, result.error)
    process.exit(1)
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

const scripts = [
  [process.execPath, [path.join(__dirname, 'fix-main-binary.cjs'), '--src-tauri', srcTauri]],
  [
    process.execPath,
    [
      path.join(__dirname, 'copy-sidecar-binaries.cjs'),
      '--src-tauri',
      srcTauri,
      ...(release ? ['--release'] : []),
    ],
  ],
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
