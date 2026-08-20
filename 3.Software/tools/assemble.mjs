/* global process, console */
import fs from 'node:fs'
import path from 'node:path'
import { parse as parseToml } from 'smol-toml'

// makeconfig-style source assembler.
//
// Reads `profiles/<name>.toml`, resolves the selected modules from
// `modules/<module>/module.toml` and copies them into a clean
// `build/<profile>/src-tauri` workspace. The source tree is never modified.
//
// Usage:
//   node tools/assemble.mjs --profile desktop-tauri-libusb

const root = path.resolve(import.meta.dirname, '..')
const profilesDir = path.join(root, 'profiles')
const modulesDir = path.join(root, 'modules')

function fail(message) {
  console.error(`[assemble] ${message}`)
  process.exit(1)
}

function copyDir(source, target) {
  fs.cpSync(source, target, { recursive: true })
}

function copyFile(source, target) {
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.copyFileSync(source, target)
}

function cleanDir(target) {
  fs.rmSync(target, { recursive: true, force: true })
  fs.mkdirSync(target, { recursive: true })
}

function readModule(name) {
  const file = path.join(modulesDir, name, 'module.toml')
  if (!fs.existsSync(file)) fail(`module manifest not found: ${file}`)
  const parsed = parseToml(fs.readFileSync(file, 'utf8'))
  const module = parsed.module
  if (!module?.name || !module?.source || !module?.target) {
    fail(`invalid module manifest: ${file}`)
  }
  return module
}

const args = process.argv.slice(2)
const profileIndex = args.indexOf('--profile')
const profileName = profileIndex >= 0 ? args[profileIndex + 1] : null
if (!profileName) fail('missing --profile <name>')

const profileFile = path.join(profilesDir, `${profileName}.toml`)
if (!fs.existsSync(profileFile)) fail(`profile not found: ${profileFile}`)
const profile = parseToml(fs.readFileSync(profileFile, 'utf8')).build
if (!profile?.name || !Array.isArray(profile.modules)) fail(`invalid profile: ${profileFile}`)
const requiredTargets = Array.isArray(profile.required) ? profile.required : []

const buildDir = path.join(root, 'build', profile.name, 'src-tauri')
cleanDir(buildDir)

// Proprietary vendor DLL only belongs to the local dll profile. It lives in
// `vendor/` (gitignored) and is copied into the generated workspace here.
if (profile.backend === 'dll') {
  const dll = path.join(root, 'vendor', 'CH34X.DLL')
  if (!fs.existsSync(dll)) fail(`vendor DLL not found: ${dll}`)
  copyFile(dll, path.join(buildDir, 'CH34X.DLL'))
}

// Resolve and copy selected modules.
const copied = []
const seenTargets = new Set()
for (const name of profile.modules) {
  const module = readModule(name)
  const source = path.join(root, module.source)
  const target = path.join(buildDir, module.target)
  if (seenTargets.has(module.target)) fail(`duplicate module target: ${module.target}`)
  seenTargets.add(module.target)
  if (!fs.existsSync(source)) fail(`module source not found for ${name}: ${source}`)
  if (fs.statSync(source).isDirectory()) {
    copyDir(source, target)
  } else {
    copyFile(source, target)
  }
  copied.push({ name, source: module.source, target: module.target })
  console.log(`[assemble] ${name} -> ${module.target}`)
}

const missingRequired = requiredTargets.filter(
  (target) => !fs.existsSync(path.join(buildDir, target)),
)
if (missingRequired.length > 0) {
  fail(`profile requires missing targets: ${missingRequired.join(', ')}`)
}

fs.writeFileSync(
  path.join(buildDir, 'build-manifest.json'),
  `${JSON.stringify({ profile: profileName, backend: profile.backend, modules: copied }, null, 2)}\n`,
)

// Generate a minimal npm shim so `npx tauri build` can run from the profile
// root. Frontend compilation delegates to the repository root; the beforeBundle
// hook is rewritten to the repository's parameterized prepare-bundle script.
const profileRoot = path.join(root, 'build', profile.name)
fs.mkdirSync(profileRoot, { recursive: true })
fs.writeFileSync(
  path.join(profileRoot, 'package.json'),
  `${JSON.stringify(
    {
      private: true,
      type: 'module',
      scripts: {
        build: 'npm --prefix ../../ run build',
        dev: `node ../../scripts/dev-all.cjs --profile ${profile.name}`,
        tauri: 'node ../../node_modules/@tauri-apps/cli/tauri.js',
      },
    },
    null,
    2,
  )}\n`,
)

const tauriConfPath = path.join(buildDir, 'tauri.conf.json')
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'))
tauriConf.build.beforeDevCommand = `node ../../scripts/dev-all.cjs --profile ${profile.name} --skip-assemble`
tauriConf.build.beforeBuildCommand = 'npm run build'
tauriConf.build.beforeBundleCommand =
  'node ../../scripts/prepare-bundle.cjs --release --src-tauri src-tauri'
fs.writeFileSync(tauriConfPath, `${JSON.stringify(tauriConf, null, 2)}\n`)

console.log(`[assemble] generated ${buildDir}`)
