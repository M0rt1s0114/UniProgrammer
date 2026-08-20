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
const legacySrcTauri = path.join(root, 'src-tauri')

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

// Static pieces of the current Tauri app crate. Modules own the rest.
const staticDirs = ['.cargo', 'capabilities', 'examples', 'gen', 'icons', 'src/bin', 'tests']
for (const dir of staticDirs) {
  const source = path.join(legacySrcTauri, dir)
  if (fs.existsSync(source)) copyDir(source, path.join(buildDir, dir))
}

const staticFiles = [
  'build.rs',
  'Cargo.lock',
  'Cargo.toml',
  'tauri.conf.json',
  'tauri.libusb.conf.json',
  'tauri.linux.conf.json',
  'app-icon.png',
  'app-icon.svg',
  'chiplib.bin',
  'chiplib.xml',
  'src/main.rs',
]
for (const file of staticFiles) {
  const source = path.join(legacySrcTauri, file)
  if (fs.existsSync(source)) copyFile(source, path.join(buildDir, file))
}

// Proprietary vendor DLL only belongs to the local dll profile.
if (profile.backend === 'dll') {
  const dll = path.join(legacySrcTauri, 'CH34X.DLL')
  if (fs.existsSync(dll)) copyFile(dll, path.join(buildDir, 'CH34X.DLL'))
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

const missingRequired = requiredTargets.filter((target) => !fs.existsSync(path.join(buildDir, target)))
if (missingRequired.length > 0) {
  fail(`profile requires missing targets: ${missingRequired.join(', ')}`)
}

// The plugin resource root is assembled entirely from module packages.
copyFile(
  path.join(legacySrcTauri, 'plugins', 'README.md'),
  path.join(buildDir, 'plugins', 'README.md'),
)

fs.writeFileSync(
  path.join(buildDir, 'build-manifest.json'),
  `${JSON.stringify({ profile: profileName, backend: profile.backend, modules: copied }, null, 2)}\n`,
)

console.log(`[assemble] generated ${buildDir}`)
