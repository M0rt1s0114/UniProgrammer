/* global require, process, __dirname, console */
/* eslint-disable @typescript-eslint/no-require-imports */
// Tauri `beforeBundleCommand`: runs after the cargo release build and before
// bundling.
//
// 1. Builds both CH34X sidecar backends (vendor DLL + libusb).
// 2. Restores the real main binary (see fix-main-binary.cjs).
// 3. Copies sidecar binaries into their plugin packages so `plugins/` can be
//    bundled as one self-contained resource tree.

const { spawnSync } = require('node:child_process')
const path = require('node:path')

const root = path.resolve(__dirname, '..')
const cargoManifest = path.join(root, 'src-tauri', 'Cargo.toml')

const cargoTargets = [
  { feature: 'hal-dll', bin: 'uni_ch34x_sidecar_dll' },
  { feature: 'hal-libusb', bin: 'uni_ch34x_sidecar_libusb' },
]

for (const { feature, bin } of cargoTargets) {
  const result = spawnSync(
    'cargo',
    [
      'build',
      '--release',
      '--manifest-path',
      cargoManifest,
      '-p',
      'uni-devices',
      '--features',
      feature,
      '--bin',
      bin,
    ],
    { cwd: root, stdio: 'inherit' },
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
