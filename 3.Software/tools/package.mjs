/* global process, console, Buffer, setTimeout, clearTimeout */
import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'
import { spawn, spawnSync } from 'node:child_process'
import { parse as parseToml } from 'smol-toml'

// Stage 6: collect a finished `build/<profile>` into the OpenWRT-style
// `dist/<profile>/` tree.
//
//   node tools/package.mjs --profile desktop-tauri-libusb
//   node tools/package.mjs --profile desktop-tauri-dll --skip-smoke
//
// Layout produced:
//   dist/<profile>/
//     installer/<setup.exe>
//     portable/uniprog-<version>-<os>-<arch>.zip
//     packages/<plugin-name>-<plugin-version>.unipkg
//     manifest.json
//     failed/run-<timestamp>/        (only when a run fails)
//
// The run is staged first and only committed after every artifact and the
// smoke check succeed, so a failed run never overwrites the last good
// `dist/<profile>/` output; its partial files are archived to `failed/`.

const root = path.resolve(import.meta.dirname, '..')
const distRoot = path.join(root, 'dist')

let activeStaging = null
let activeFailedDir = null

function fail(message) {
  archiveActiveStaging()
  console.error(`[package] ${message}`)
  process.exit(1)
}

function archiveActiveStaging() {
  if (!activeStaging || !fs.existsSync(activeStaging)) return
  const failedDir = activeFailedDir || path.join(path.dirname(activeStaging), 'failed')
  fs.mkdirSync(failedDir, { recursive: true })
  let target = path.join(failedDir, `run-${runTag()}`)
  let index = 2
  while (fs.existsSync(target)) target = path.join(failedDir, `run-${runTag()}-${index++}`)
  try {
    fs.renameSync(activeStaging, target)
    console.error(`[package] partial output archived to ${target}`)
  } catch (error) {
    console.error(`[package] could not archive partial output to ${target}: ${error.message}`)
  }
}

function runTag() {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/g, '')
    .slice(0, 14)
  return `${stamp}-${process.pid}`
}

function cleanLeftoverStagings(profileDist, failedDir) {
  if (!fs.existsSync(profileDist)) return
  for (const entry of fs.readdirSync(profileDist, { withFileTypes: true })) {
    if (entry.isDirectory() && entry.name.startsWith('.staging-')) {
      const stale = path.join(profileDist, entry.name)
      fs.mkdirSync(failedDir, { recursive: true })
      let target = path.join(failedDir, `run-${runTag()}`)
      let index = 2
      while (fs.existsSync(target)) target = path.join(failedDir, `run-${runTag()}-${index++}`)
      fs.renameSync(stale, target)
      console.log(`[package] archived leftover staging to ${target}`)
    }
  }
}

function readJson(file, label) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'))
  } catch (error) {
    fail(`failed to read ${label} ${file}: ${error.message}`)
  }
}

function parseArgs(args) {
  const profileIndex = args.indexOf('--profile')
  const profileName = profileIndex >= 0 ? args[profileIndex + 1] : null
  const skipSmoke = args.includes('--skip-smoke')
  if (!profileName) fail('missing --profile <name>')
  return { profileName, skipSmoke }
}

function platformTag() {
  if (process.platform === 'win32') return 'win'
  if (process.platform === 'linux') return 'linux'
  if (process.platform === 'darwin') return 'macos'
  return process.platform
}

function archTag() {
  if (process.arch === 'x64') return 'x64'
  if (process.arch === 'arm64') return 'arm64'
  return process.arch
}

function executableName(base) {
  return process.platform === 'win32' ? `${base}.exe` : base
}

function sha256File(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')
}

function gitCommit() {
  const result = spawnSync('git', ['-C', root, 'rev-parse', 'HEAD'], {
    encoding: 'utf8',
    windowsHide: true,
  })
  return result.status === 0 ? result.stdout.trim() : null
}

function collectTree(sourceDir, destinationPrefix, output, relative = '') {
  for (const entry of fs
    .readdirSync(sourceDir, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))) {
    const source = path.join(sourceDir, entry.name)
    const destination = path.join(destinationPrefix, relative, entry.name).split(path.sep).join('/')
    if (entry.isDirectory()) {
      collectTree(source, destinationPrefix, output, path.join(relative, entry.name))
    } else if (entry.isFile()) {
      if (output.has(destination)) fail(`duplicate zip entry: ${destination}`)
      output.set(destination, source)
    }
  }
}

// ---------------------------------------------------------------------------
// Minimal STORE-mode ZIP writer. The Rust side reads this format via the
// `zip` crate and the archives stay small enough that compression is not
// needed. Keeping it dependency-free makes the packaging stage usable on
// machines that never ran `npm install`.
// ---------------------------------------------------------------------------

function crc32(buffer) {
  let crc = 0xffffffff
  for (const byte of buffer) crc = (crc >>> 8) ^ CRC_TABLE[(crc ^ byte) & 0xff]
  return (crc ^ 0xffffffff) >>> 0
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256)
  for (let n = 0; n < 256; n += 1) {
    let c = n
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    table[n] = c >>> 0
  }
  return table
})()

function writeStoreZip(outputFile, files) {
  fs.mkdirSync(path.dirname(outputFile), { recursive: true })
  const descriptor = fs.openSync(outputFile, 'w')
  const central = []
  let offset = 0

  function write(buffer) {
    let written = 0
    while (written < buffer.length) {
      written += fs.writeSync(descriptor, buffer, written, buffer.length - written)
    }
    offset += buffer.length
  }

  for (const [entryName, sourceFile] of [...files.entries()].sort(([a], [b]) =>
    a.localeCompare(b),
  )) {
    const localOffset = offset
    const data = fs.readFileSync(sourceFile)
    const checksum = crc32(data)
    const name = Buffer.from(entryName, 'utf8')

    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4)
    local.writeUInt16LE(0, 6)
    local.writeUInt16LE(0, 8)
    local.writeUInt16LE(0, 10)
    local.writeUInt16LE(0, 12)
    local.writeUInt32LE(checksum, 14)
    local.writeUInt32LE(data.length, 18)
    local.writeUInt32LE(data.length, 22)
    local.writeUInt16LE(name.length, 26)
    local.writeUInt16LE(0, 28)
    write(local)
    write(name)
    write(data)

    const centralHeader = Buffer.alloc(46)
    centralHeader.writeUInt32LE(0x02014b50, 0)
    centralHeader.writeUInt16LE(20, 4)
    centralHeader.writeUInt16LE(20, 6)
    centralHeader.writeUInt16LE(0, 8)
    centralHeader.writeUInt16LE(0, 10)
    centralHeader.writeUInt16LE(0, 12)
    centralHeader.writeUInt16LE(0, 14)
    centralHeader.writeUInt32LE(checksum, 16)
    centralHeader.writeUInt32LE(data.length, 20)
    centralHeader.writeUInt32LE(data.length, 24)
    centralHeader.writeUInt16LE(name.length, 28)
    centralHeader.writeUInt16LE(0, 30)
    centralHeader.writeUInt16LE(0, 32)
    centralHeader.writeUInt16LE(0, 34)
    centralHeader.writeUInt16LE(0, 36)
    centralHeader.writeUInt32LE(0, 38)
    centralHeader.writeUInt32LE(localOffset, 42)
    central.push({ header: centralHeader, name })
  }

  const centralStart = offset
  for (const { header, name } of central) {
    write(header)
    write(name)
  }
  const centralSize = offset - centralStart

  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(0, 4)
  end.writeUInt16LE(0, 6)
  end.writeUInt16LE(central.length, 8)
  end.writeUInt16LE(central.length, 10)
  end.writeUInt32LE(centralSize, 12)
  end.writeUInt32LE(centralStart, 16)
  end.writeUInt16LE(0, 20)
  write(end)
  fs.closeSync(descriptor)
}

// ---------------------------------------------------------------------------
// Smoke checks. Both checks are timeout-bounded and never leave a process
// behind: the sidecar receives a handshake frame, the packaged main binary
// just has to stay alive for a few seconds without writing
// `uniprog-boot-error.txt`.
// ---------------------------------------------------------------------------

function frame(payload) {
  const body = Buffer.from(JSON.stringify(payload), 'utf8')
  const header = Buffer.alloc(4)
  header.writeUInt32LE(body.length, 0)
  return Buffer.concat([header, body])
}

function killProcessTree(pid) {
  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/pid', String(pid), '/t', '/f'], {
      stdio: 'ignore',
      windowsHide: true,
    })
    return
  }
  try {
    process.kill(pid, 'SIGKILL')
  } catch {
    // The process already exited.
  }
}

function sidecarHandshake(binary) {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'ignore'], windowsHide: true })
    let settled = false
    let received = Buffer.alloc(0)

    const timer = setTimeout(() => {
      if (settled) return
      settled = true
      killProcessTree(child.pid)
      reject(new Error(`sidecar handshake timed out: ${binary}`))
    }, 10_000)

    function finish(error) {
      if (settled) return
      settled = true
      clearTimeout(timer)
      killProcessTree(child.pid)
      if (error) reject(error)
      else resolve()
    }

    child.stdout.on('data', (chunk) => {
      received = Buffer.concat([received, chunk])
      while (!settled && received.length >= 4) {
        const length = received.readUInt32LE(0)
        if (received.length < 4 + length) return
        const payload = received.subarray(4, 4 + length)
        received = received.subarray(4 + length)
        let response
        try {
          response = JSON.parse(payload.toString('utf8'))
        } catch (error) {
          finish(new Error(`sidecar returned invalid JSON: ${error.message}`))
          return
        }
        if (response.error) {
          finish(
            new Error(
              `sidecar handshake failed: ${response.error.message || JSON.stringify(response.error)}`,
            ),
          )
        } else {
          finish()
        }
        return
      }
    })
    child.stdin.on('error', () => {})
    child.on('error', (error) => finish(new Error(`failed to start ${binary}: ${error.message}`)))
    child.on('exit', (code) => {
      if (!settled) finish(new Error(`sidecar exited before handshake (code ${code})`))
    })

    child.stdin.write(frame({ id: 1, method: 'handshake', params: {} }))
  })
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function removeRuntimeArtifacts(dir) {
  for (const name of ['logs', 'Setting.set', 'plugin-state.toml', 'uniprog-boot-error.txt']) {
    fs.rmSync(path.join(dir, name), { recursive: true, force: true })
  }
}

async function smokePortable(mainBinary, portableDir) {
  const bootError = path.join(portableDir, 'uniprog-boot-error.txt')
  if (fs.existsSync(bootError)) fail(`found ${bootError} before the portable smoke check`)

  const pluginRoot = path.join(portableDir, 'plugins', 'builtin')
  const exeSuffix = process.platform === 'win32' ? '.exe' : ''
  const sidecars = []
  if (fs.existsSync(pluginRoot)) {
    for (const entry of fs.readdirSync(pluginRoot, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue
      const packageDir = path.join(pluginRoot, entry.name)
      for (const file of fs.readdirSync(packageDir)) {
        if (file.startsWith('upt_ch34x_sidecar_') && (!exeSuffix || file.endsWith(exeSuffix))) {
          sidecars.push(path.join(packageDir, file))
        }
      }
    }
  }
  for (const sidecar of sidecars) {
    console.log(`[package] smoke: sidecar handshake ${path.basename(sidecar)}`)
    await sidecarHandshake(sidecar)
  }

  console.log(`[package] smoke: starting ${path.basename(mainBinary)} for 5s`)
  const child = spawn(mainBinary, [], { cwd: portableDir, stdio: 'ignore', windowsHide: true })
  let exitInfo = null
  child.on('exit', (code, signal) => {
    exitInfo = { code, signal }
  })
  await sleep(5000)
  if (exitInfo) {
    const report = fs.existsSync(bootError) ? fs.readFileSync(bootError, 'utf8') : 'no boot report'
    fail(`portable main binary exited during smoke check (${JSON.stringify(exitInfo)})\n${report}`)
  }
  killProcessTree(child.pid)
  await sleep(1000)
  if (fs.existsSync(bootError)) {
    fail(`smoke check produced ${bootError}:\n${fs.readFileSync(bootError, 'utf8')}`)
  }
  console.log('[package] smoke: portable layout passed')
}

// ---------------------------------------------------------------------------
// Plugin package collection.
// ---------------------------------------------------------------------------

function addFile(files, packageDir, relative) {
  const source = path.join(packageDir, relative)
  if (!fs.existsSync(source) || !fs.statSync(source).isFile()) {
    fail(`plugin payload not found: ${source}`)
  }
  const zipName = relative.split(path.sep).join('/')
  if (files.has(zipName)) return
  files.set(zipName, source)
}

function addPattern(files, packageDir, pattern) {
  const normalized = pattern.replaceAll('\\', '/')
  if (normalized.startsWith('/') || normalized.includes('..')) {
    fail(`unsafe [packaging] include pattern: ${pattern}`)
  }
  const direct = path.join(packageDir, normalized)
  if (fs.existsSync(direct)) {
    if (fs.statSync(direct).isDirectory()) {
      collectTree(direct, '', files)
    } else {
      addFile(files, packageDir, normalized)
    }
    return
  }
  const withExe = `${normalized}.exe`
  if (fs.existsSync(path.join(packageDir, withExe))) {
    addFile(files, packageDir, withExe)
    return
  }
  fail(`[packaging] include pattern not found in ${packageDir}: ${pattern}`)
}

function addEntryFile(files, packageDir, entry) {
  if (!entry || entry === 'builtin') return
  const direct = path.join(packageDir, entry)
  const withExe = `${entry}.exe`
  if (fs.existsSync(direct) && fs.statSync(direct).isFile()) addFile(files, packageDir, entry)
  else if (fs.existsSync(path.join(packageDir, withExe))) addFile(files, packageDir, withExe)
  else fail(`plugin entry not found in ${packageDir}: ${entry}`)
}

function packageFiles(packageDir, manifest) {
  const files = new Map()
  addFile(files, packageDir, 'unipkg.toml')

  const include = manifest.packaging?.include
  if (Array.isArray(include)) {
    for (const pattern of include) addPattern(files, packageDir, String(pattern))
  } else if (manifest.kind === 'ui') {
    if (fs.existsSync(path.join(packageDir, 'dist'))) addPattern(files, packageDir, 'dist')
  } else if (manifest.kind === 'adapter') {
    for (const entry of fs.readdirSync(packageDir, { withFileTypes: true })) {
      if (entry.isFile() && entry.name.toLowerCase().endsWith('.dll')) {
        addFile(files, packageDir, entry.name)
      }
    }
  }

  // Entry is part of the install contract for process/UI packages. The
  // compile-time modules (`upt.hal`, `upt.proto`, `upt.chipdb`) are
  // non-distributable anyway, and legacy manifests may still reference a
  // sidecar binary that is intentionally absent from their package.
  if (manifest.kind === 'ui' || manifest.kind === 'adapter') {
    addEntryFile(files, packageDir, manifest.entry)
  }
  return files
}

function collectPluginPackages(pluginsBuiltinDir) {
  const packages = []
  for (const entry of fs
    .readdirSync(pluginsBuiltinDir, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))) {
    if (!entry.isDirectory()) continue
    const packageDir = path.join(pluginsBuiltinDir, entry.name)
    const manifestPath = path.join(packageDir, 'unipkg.toml')
    if (!fs.existsSync(manifestPath)) continue
    let document
    try {
      document = parseToml(fs.readFileSync(manifestPath, 'utf8'))
    } catch (error) {
      fail(`invalid plugin manifest ${manifestPath}: ${error.message}`)
    }
    const manifest = {
      name: document.package?.name,
      version: document.package?.version,
      kind: document.package?.kind,
      entry: document.package?.entry,
      packaging: document.packaging || {},
    }
    if (!manifest.name || !manifest.version || !manifest.kind || !manifest.entry) {
      fail(`incomplete plugin manifest: ${manifestPath}`)
    }
    if (manifest.packaging.distributable === false) {
      console.log(`[package] skip non-distributable plugin ${manifest.name}`)
      continue
    }
    const files = packageFiles(packageDir, manifest)
    packages.push({
      manifest,
      directory: packageDir,
      archive: `${manifest.name}-${manifest.version}.unipkg`,
      files,
    })
  }
  return packages
}

// ---------------------------------------------------------------------------
// Main pipeline.
// ---------------------------------------------------------------------------

async function main() {
  const { profileName, skipSmoke } = parseArgs(process.argv.slice(2))

  const profileFile = path.join(root, 'profiles', `${profileName}.toml`)
  if (!fs.existsSync(profileFile)) fail(`profile not found: ${profileFile}`)
  const profile = parseToml(fs.readFileSync(profileFile, 'utf8')).build
  if (!profile?.name || profile.name !== profileName) {
    fail(`profile name mismatch in ${profileFile}`)
  }
  const backend = profile.backend ?? 'libusb'

  const buildRoot = path.join(root, 'build', profile.name)
  const srcTauri = path.join(buildRoot, 'src-tauri')
  if (!fs.existsSync(srcTauri)) fail(`generated workspace not found: ${srcTauri}`)

  const tauriConfig = readJson(path.join(srcTauri, 'tauri.conf.json'), 'Tauri config')
  const version = tauriConfig.version || '0.0.0'
  const productName = tauriConfig.productName || 'UniProgrammer'
  const buildManifestPath = path.join(srcTauri, 'build-manifest.json')
  const buildManifest = fs.existsSync(buildManifestPath)
    ? readJson(buildManifestPath, 'build manifest')
    : null

  const releaseDir = path.join(srcTauri, 'target', 'release')
  const mainBinary = path.join(releaseDir, executableName('uniprog'))
  const chipdb = path.join(srcTauri, 'chiplib.bin')
  const pluginsRoot = path.join(srcTauri, 'plugins')
  const pluginsBuiltinDir = path.join(pluginsRoot, 'builtin')
  const sidecarBase = `upt_ch34x_sidecar_${backend}`
  const sidecar = path.join(
    pluginsBuiltinDir,
    `upt.hal.ch34x_${backend}`,
    executableName(sidecarBase),
  )
  const vendorDll = path.join(srcTauri, 'CH34X.DLL')
  const adapterPackageDll = path.join(pluginsBuiltinDir, `upt.hal.ch34x_${backend}`, 'CH34X.DLL')

  for (const [label, file] of [
    ['main binary', mainBinary],
    ['chip database', chipdb],
    ['plugins tree', pluginsRoot],
    ['adapter sidecar', sidecar],
  ]) {
    if (!fs.existsSync(file)) fail(`${label} missing: ${file}`)
  }
  if (backend === 'dll') {
    for (const [label, file] of [
      ['vendor DLL', vendorDll],
      ['adapter package DLL', adapterPackageDll],
    ]) {
      if (!fs.existsSync(file)) fail(`${label} missing: ${file}`)
    }
  }

  const bundleDir = path.join(releaseDir, 'bundle')
  const nsisDir = path.join(bundleDir, 'nsis')
  const installers = fs.existsSync(nsisDir)
    ? fs.readdirSync(nsisDir).filter((name) => name.toLowerCase().endsWith('-setup.exe'))
    : []
  if (installers.length !== 1) {
    fail(
      installers.length === 0
        ? `no NSIS installer found in ${nsisDir}`
        : `ambiguous NSIS installers in ${nsisDir}: ${installers.join(', ')}`,
    )
  }
  const installerSource = path.join(nsisDir, installers[0])

  const profileDist = path.join(distRoot, profile.name)
  const failedDir = path.join(profileDist, 'failed')
  activeFailedDir = failedDir
  fs.mkdirSync(profileDist, { recursive: true })
  cleanLeftoverStagings(profileDist, failedDir)

  const staging = path.join(profileDist, `.staging-${process.pid}`)
  activeStaging = staging
  fs.mkdirSync(path.join(staging, 'installer'), { recursive: true })
  fs.mkdirSync(path.join(staging, 'portable'), { recursive: true })
  fs.mkdirSync(path.join(staging, 'packages'), { recursive: true })

  const installerName = path.basename(installerSource)
  fs.copyFileSync(installerSource, path.join(staging, 'installer', installerName))
  console.log(`[package] installer: ${installerName}`)

  const portableName = `uniprog-${version}-${platformTag()}-${archTag()}.zip`
  const portableSrc = path.join(staging, '.portable-src')
  fs.mkdirSync(portableSrc, { recursive: true })
  const mainBinaryName = executableName('uniprog')
  const portableMainBinary = path.join(portableSrc, mainBinaryName)
  fs.copyFileSync(mainBinary, portableMainBinary)
  fs.copyFileSync(chipdb, path.join(portableSrc, 'chiplib.bin'))
  fs.cpSync(pluginsRoot, path.join(portableSrc, 'plugins'), { recursive: true })
  if (backend === 'dll') fs.copyFileSync(vendorDll, path.join(portableSrc, 'CH34X.DLL'))
  fs.writeFileSync(
    path.join(portableSrc, 'README.txt'),
    [
      `UniProgrammer ${version}（便携版）`,
      `架构：${platformTag()}-${archTag()}，后端：${backend}`,
      '',
      '直接运行 uniprog.exe。',
      'plugins/ 是插件目录，chiplib.bin 是内置芯片数据库。',
      backend === 'dll'
        ? 'CH34X.DLL 为厂商闭源 DLL，仅限本地/朋友间分发，不得随 GitHub Release 发布。'
        : '本包不含厂商闭源 DLL（libusb 后端）。',
      '',
    ].join('\r\n'),
  )

  if (!skipSmoke) await smokePortable(portableMainBinary, portableSrc)
  else console.log('[package] smoke checks skipped (--skip-smoke)')

  // The smoke check runs the real app, so it creates runtime files (logs,
  // plugin state, settings). Those must not ship inside the portable zip.
  removeRuntimeArtifacts(portableSrc)

  // Zip exactly the tree that was just smoke-tested.
  const portableFiles = new Map()
  collectTree(portableSrc, '', portableFiles)
  const portableZip = path.join(staging, 'portable', portableName)
  writeStoreZip(portableZip, portableFiles)
  console.log(`[package] portable: ${portableName}`)

  const pluginPackages = collectPluginPackages(pluginsBuiltinDir)
  for (const plugin of pluginPackages) {
    const output = path.join(staging, 'packages', plugin.archive)
    writeStoreZip(output, plugin.files)
    console.log(`[package] plugin: ${plugin.archive}`)
  }

  const artifacts = {
    installer: {
      file: `installer/${installerName}`,
      size: fs.statSync(path.join(staging, 'installer', installerName)).size,
      sha256: sha256File(path.join(staging, 'installer', installerName)),
    },
    portable: {
      file: `portable/${portableName}`,
      size: fs.statSync(portableZip).size,
      sha256: sha256File(portableZip),
    },
    packages: pluginPackages.map((plugin) => {
      const file = `packages/${plugin.archive}`
      return {
        name: plugin.manifest.name,
        version: plugin.manifest.version,
        file,
        size: fs.statSync(path.join(staging, file)).size,
        sha256: sha256File(path.join(staging, file)),
      }
    }),
  }

  const manifest = {
    profile: profile.name,
    productName,
    version,
    backend,
    builtAt: new Date().toISOString(),
    gitCommit: gitCommit(),
    smokeChecked: !skipSmoke,
    modules: buildManifest?.modules ?? [],
    artifacts,
  }
  const manifestPath = path.join(staging, 'manifest.json')
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`)

  // Commit: replace only the three artifact directories, keep `failed/` and
  // never touch the previous success before the new staging is complete.
  for (const category of ['installer', 'portable', 'packages']) {
    const destination = path.join(profileDist, category)
    fs.rmSync(destination, { recursive: true, force: true })
    fs.renameSync(path.join(staging, category), destination)
  }
  fs.copyFileSync(manifestPath, path.join(profileDist, 'manifest.json'))
  fs.rmSync(portableSrc, { recursive: true, force: true })
  fs.rmSync(staging, { recursive: true, force: true })
  activeStaging = null

  console.log(`[package] profile ${profile.name} published to ${profileDist}`)
  console.log(JSON.stringify({ profile: profile.name, version, artifacts }, null, 2))
}

try {
  await main()
} catch (error) {
  fail(error instanceof Error ? error.message : String(error))
}
