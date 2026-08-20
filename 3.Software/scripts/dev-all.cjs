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
const profileArgIndex = process.argv.indexOf('--profile')
const profile =
  profileArgIndex >= 0
    ? process.argv[profileArgIndex + 1]
    : process.platform === 'win32'
      ? 'desktop-tauri-dll'
      : 'desktop-tauri-libusb'
const srcTauri = path.join(root, 'build', profile, 'src-tauri')

// When invoked by `tauri dev` the workspace was already assembled by
// `tools/tauri.mjs` (the CLI needs the generated tauri.conf.json before it
// can run beforeDevCommand). Re-assembling here would delete the directory
// the CLI made its working directory, so the generated config passes
// `--skip-assemble`. The standalone `npm run dev` path still assembles first.
const skipAssemble = process.argv.includes('--skip-assemble')
if (skipAssemble) {
  console.log(`[dev-all] workspace already assembled (${profile})`)
} else {
  const assemble = spawnSync(
    process.execPath,
    [path.join(root, 'tools', 'assemble.mjs'), '--profile', profile],
    { cwd: root, stdio: 'inherit' },
  )
  if (assemble.status !== 0) process.exit(assemble.status ?? 1)
}

// Build and stage the built-in sidecar plugins first. HalRouter looks for the
// executables in their package directories when `tauri dev` is used.
const cargoManifest = path.join(srcTauri, 'Cargo.toml')
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
  [path.join(root, 'scripts', 'copy-sidecar-binaries.cjs'), '--src-tauri', srcTauri],
  { cwd: root, stdio: 'inherit' },
)
if (copySidecars.status !== 0) {
  process.exit(copySidecars.status ?? 1)
}

const targets = [
  { name: 'upt.tauri', config: 'modules/upt-shell-tauri/package/vite.config.js' },
  { name: 'upt.tauri.hexview', config: 'modules/upt-shell-tauri-hexview/package/vite.config.js' },
]

const children = targets.map((target) => {
  const child = spawn(process.execPath, [viteCli, '--config', path.join(root, target.config)], {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, FORCE_COLOR: '1' },
  })
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
