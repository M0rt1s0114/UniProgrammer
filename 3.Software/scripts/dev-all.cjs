/* global require, process, __dirname, console */
/* eslint-disable @typescript-eslint/no-require-imports */
// Runs the Vite dev servers for every built-in UI plugin in one command:
//   - upt.tauri         -> http://localhost:1420 (main window in `tauri dev`)
//   - upt.tauri.hexview -> http://localhost:1421 (loaded through the unipkg://
//     protocol, which redirects to this server in debug builds)
const { spawn, spawnSync } = require('node:child_process')
const path = require('node:path')

const root = path.resolve(__dirname, '..')
const viteCli = path.join(root, 'node_modules', 'vite', 'bin', 'vite.js')
const pluginsDir = path.join(root, 'src-tauri', 'plugins', 'builtin')

// Build and stage the built-in sidecar plugins first. HalRouter looks for the
// executables in their package directories when `npm run dev` is used.
const cargoManifest = path.join(root, 'src-tauri', 'Cargo.toml')
const sidecarTargets = [
  { feature: 'hal-dll', bin: 'upt_ch34x_sidecar_dll' },
  { feature: 'hal-libusb', bin: 'upt_ch34x_sidecar_libusb' },
]
for (const { feature, bin } of sidecarTargets) {
  const result = spawnSync(
    'cargo',
    [
      'build',
      '--manifest-path',
      cargoManifest,
      '-p',
      'upt-devices',
      '--features',
      feature,
      '--bin',
      bin,
    ],
    { cwd: root, stdio: 'inherit' },
  )
  if (result.error) {
    console.error(`[dev-all] failed to launch cargo for ${bin}:`, result.error)
    process.exit(1)
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}
const copySidecars = spawnSync(
  process.execPath,
  [path.join(root, 'scripts', 'copy-sidecar-binaries.cjs')],
  { cwd: root, stdio: 'inherit' },
)
if (copySidecars.status !== 0) {
  process.exit(copySidecars.status ?? 1)
}

const targets = [
  { name: 'upt.tauri', config: 'upt.tauri/vite.config.js' },
  { name: 'upt.tauri.hexview', config: 'upt.tauri.hexview/vite.config.js' },
]

const children = targets.map((target) => {
  const child = spawn(
    process.execPath,
    [viteCli, '--config', path.join(pluginsDir, target.config)],
    {
      cwd: root,
      stdio: 'inherit',
      env: { ...process.env, FORCE_COLOR: '1' },
    },
  )
  child.on('exit', (code, signal) => {
    if (signal) {
      console.log(`[dev-all] ${target.name} stopped (${signal})`)
    } else if (code !== 0) {
      console.error(`[dev-all] ${target.name} exited with code ${code}`)
    }
  })
  return { target, child }
})

function shutdown() {
  for (const { child } of children) {
    if (!child.killed) child.kill()
  }
}

process.on('SIGINT', () => {
  shutdown()
  process.exit(130)
})
process.on('SIGTERM', () => {
  shutdown()
  process.exit(143)
})
process.on('exit', shutdown)

console.log(`[dev-all] started ${targets.map((t) => t.name).join(', ')}`)
