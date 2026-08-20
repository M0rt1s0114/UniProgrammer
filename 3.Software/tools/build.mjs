/* global process, console */
import fs from 'node:fs'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

// Full generated release build.
//
//   node tools/build.mjs --profile desktop-tauri-libusb
//   node tools/build.mjs --profile desktop-tauri-dll
//
// Pipeline:
//   frontend build -> assemble -> cargo release -> prepare sidecars ->
//   tauri bundle (NSIS + resources) inside build/<profile>.

const root = path.resolve(import.meta.dirname, '..')

function run(command, args, cwd, shell = false) {
  console.log(`[build] $ ${command} ${args.join(' ')}`)
  const result = spawnSync(command, args, { cwd, stdio: 'inherit', shell })
  if (result.error) console.error(`[build] spawn error: ${result.error.message}`)
  return result.status ?? 1
}

function fail(message) {
  console.error(`[build] ${message}`)
  process.exit(1)
}

const args = process.argv.slice(2)
const profileIndex = args.indexOf('--profile')
const profileName = profileIndex >= 0 ? args[profileIndex + 1] : null
if (!profileName) fail('missing --profile <name>')

const profileFile = path.join(root, 'profiles', `${profileName}.toml`)
if (!fs.existsSync(profileFile)) fail(`profile not found: ${profileFile}`)

// The backend value is needed for cargo feature selection and Tauri config.
const tomlText = fs.readFileSync(profileFile, 'utf8')
const backendMatch = /^backend = "([^"]+)"/m.exec(tomlText)
const backend = backendMatch?.[1] ?? 'libusb'
const features = backend === 'libusb' ? ['--features', 'hal-libusb'] : []
const configArg = backend === 'libusb' ? ['--config', 'src-tauri/tauri.libusb.conf.json'] : []

const profileRoot = path.join(root, 'build', profileName)
const srcTauri = path.join(profileRoot, 'src-tauri')

// 1. Frontend bundles land in the module package dirs.
const npmCommand = process.platform === 'win32' ? 'npm' : 'npm'
if (run(npmCommand, ['run', 'build'], root, process.platform === 'win32') !== 0) {
  fail('frontend build failed')
}

// 2. Generate the workspace.
if (
  run(
    process.execPath,
    [path.join(root, 'tools', 'assemble.mjs'), '--profile', profileName],
    root,
  ) !== 0
) {
  fail('assembler failed')
}

// 3. Cargo release build.
const cargoArgs = [
  'build',
  '--release',
  '--manifest-path',
  path.join(srcTauri, 'Cargo.toml'),
  ...features,
]
if (run('cargo', cargoArgs, srcTauri) !== 0) fail('cargo release build failed')

// 4. Tauri bundler. Its beforeBundle hook builds/copies sidecars and restores
//    the real main binary in the generated workspace.
const tauriArgs = [
  path.join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js'),
  'build',
  ...configArg,
  ...features,
  '--ci',
]
if (run(process.execPath, tauriArgs, profileRoot) !== 0) fail('tauri bundle failed')

// 5. Stage 6: collect the finished build into `dist/<profile>/`.
if (
  run(
    process.execPath,
    [path.join(root, 'tools', 'package.mjs'), '--profile', profileName],
    root,
  ) !== 0
) {
  fail('packaging failed')
}

console.log(`[build] profile ${profileName} finished in ${profileRoot}`)
