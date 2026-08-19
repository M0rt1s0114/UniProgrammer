# Contributing to UniProgrammer

[中文](CONTRIBUTING_CN.md)

Thank you for your interest in contributing! UniProgrammer is a cross-platform
SPI flash programmer with a pluggable hardware abstraction layer (HAL). This
document describes how to set up the project, make changes, and submit them.

## Ground rules

- **Commit messages must be written in English.**
- Follow [Conventional Commits](https://www.conventionalcommits.org/).
- Discuss large features in an issue before opening a pull request.
- This project can erase and program real flash chips. Never test write or
  erase operations on a chip whose contents you cannot afford to lose.
- Do not commit modified vendor binaries (for example `CH34X.DLL`). Vendor
  binaries are only updated by the maintainers from official releases.

## Development environment

Required tooling:

- Rust (stable) with `cargo`, `rustfmt`, `clippy`
- Node.js 22+ and npm
- Platform dependencies
  - Windows: WebView2 (usually preinstalled)
  - Linux: WebKitGTK 4.1, GTK 3, libusb-1.0, libudev, plus the standard
    Tauri Linux dependencies

## Building

The build menu scripts select the HAL backend and run the full release build.

Windows (PowerShell):

```powershell
.\build-menu.ps1
```

Linux / macOS:

```bash
./build-menu.sh
```

Backend selection rules:

- No feature flag: platform default (`Windows` -> `hal-dll`, others -> `hal-libusb`)
- `--features hal-libusb`: force the rusb/libusb backend
- `--features hal-dll`: force the CH34X.DLL backend (Windows only)

## Commit messages

All commits use Conventional Commits in English:

```
type(scope): summary
```

Types:

| Type | Purpose |
| --- | --- |
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation |
| `style` | Formatting only |
| `refactor` | Code restructuring without behavior change |
| `perf` | Performance improvement |
| `test` | Tests |
| `build` | Build system or dependencies |
| `ci` | CI configuration |
| `chore` | Maintenance tasks |
| `revert` | Revert a previous commit |

Examples:

```
feat(hal): add Windows DLL backend
fix(serprog): correct S_BUSTYPE and O_SPIOP opcodes
refactor(chiplib): replace XML fallback with typed loader
chore(release): 0.4.0-alpha.2
```

Breaking changes use `!` or a `BREAKING CHANGE:` footer:

```
feat(hal)!: split backends into separate compile features
```

## Branches and pull requests

- Branch from `main` with a short English name:
  `feat/...`, `fix/...`, `docs/...`, `chore/...`
- Keep pull requests focused on one change.
- Add or update tests for changed logic.
- Make sure `cargo check`, `cargo test`, and the lint suite from
  [Code style](#code-style) pass locally.

## Code style

Run the full lint suite from `3.Software` before pushing:

Windows (PowerShell):

```powershell
.\lint.ps1
```

Linux / macOS:

```bash
./lint.sh
```

The suite checks:

- Frontend: `npm run lint` (ESLint 10 + typescript-eslint +
  eslint-plugin-vue) and `npm run format:check` (Prettier)
- Backend: `cargo fmt --check` and `cargo lint`
  (`cargo clippy --all-targets -- -D warnings`; see
  `3.Software/src-tauri/.cargo/config.toml`)

CI runs the same checks on every push to `main` and every pull request via
`.github/workflows/lint.yml`.

Individual commands:

```bash
# Frontend (run inside 3.Software)
npm run lint
npm run lint:fix
npm run format:check
npm run format

# Backend (run inside 3.Software/src-tauri)
cargo fmt --check
cargo lint
```

Frontend style notes:

- Vue 3 `<script setup lang="ts">`
- Keep user-visible strings in `3.Software/src/i18n/index.ts`
- New reusable UI widgets belong in `3.Software/src/components`

## Testing

At minimum:

```bash
cargo test
```

Hardware changes require manual validation. Use the table below as a checklist:

| Backend | Chip | Read ID | Erase | Read | Write | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| CH341A DLL | SPI NOR | | | | | |
| CH347T DLL | SPI NOR | | | | | |
| CH347F DLL | SPI NOR | | | | | |
| libusb | SPI NOR | | | | | |
| Serprog | SPI NOR | | | | | |

- 2026-08-16: CH341A DLL + SPI NOR basic read/write/erase/verify passed on
  one test setup. Keep filling the other cells as hardware testing progresses.

## Chip database

- `chiplib.bin` is the authoritative database and is lightly obfuscated on disk.
- `chiplib.xml` is an obfuscated fallback, not a readable source file.
- Decoding happens in memory only; never commit or leave a plaintext database
  file in the working directory.
- Prefer `chipdb_tool merge <bin> <chips.tsv>` for batch updates and
  `chipdb_tool add ...` for single chips so existing enriched attributes are
  not overwritten.

## Versioning

- Semantic Versioning 2.0.0
- Git tags use a `v` prefix: `v0.4.0-alpha.2`
- Pre-releases: `-alpha.N`, `-beta.N`, `-rc.N`
- Pushing a `v*` tag triggers the release workflow

## License

UniProgrammer is licensed under
[GPL-3.0-or-later](https://www.gnu.org/licenses/gpl-3.0.html).
By contributing, you agree that your contributions will be licensed under
the same terms.
