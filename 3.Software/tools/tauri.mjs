/* global process, console */
import fs from 'node:fs'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

// Tauri CLI wrapper that always works against an assembled profile, so the
// repository no longer needs a hand-written `src-tauri/` directory.
//
//   npm run tauri -- dev [--profile desktop-tauri-dll]
//   npm run tauri -- build [--profile desktop-tauri-libusb]

const root = path.resolve(import.meta.dirname, '..')
const cli = path.join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, stdio: 'inherit' })
  if (result.error) console.error(`[tauri] spawn error: ${result.error.message}`)
  return result.status ?? 1
}

function fail(message) {
  console.error(`[tauri] ${message}`)
  process.exit(1)
}

const args = process.argv.slice(2)
const sub = args[0]
if (sub !== 'dev' && sub !== 'build') fail('usage: npm run tauri -- <dev|build> [--profile name]')

const profileIndex = args.indexOf('--profile')
const profile =
  profileIndex >= 0
    ? args[profileIndex + 1]
    : process.platform === 'win32'
      ? 'desktop-tauri-dll'
      : 'desktop-tauri-libusb'

const profileFile = path.join(root, 'profiles', `${profile}.toml`)
if (!fs.existsSync(profileFile)) fail(`profile not found: ${profile}`)
const tomlText = fs.readFileSync(profileFile, 'utf8')
const backend = /^backend = "([^"]+)"/m.exec(tomlText)?.[1] ?? 'libusb'

const profileRoot = path.join(root, 'build', profile)
if (
  run(process.execPath, [path.join(root, 'tools', 'assemble.mjs'), '--profile', profile], root) !==
  0
) {
  fail('assembler failed')
}

const cliArgs = [cli, sub]
if (sub === 'build') cliArgs.push('--ci')
if (backend === 'libusb') {
  cliArgs.push('--config', 'src-tauri/tauri.libusb.conf.json', '--features', 'hal-libusb')
}
if (run(process.execPath, cliArgs, profileRoot) !== 0) fail(`tauri ${sub} failed`)
