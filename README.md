# UniProgrammer

> A cross-platform NAND/NOR SPI flash programmer with a pluggable hardware abstraction layer.

[![License: GPL v3+](https://img.shields.io/badge/License-GPLv3+-blue.svg)](LICENSE)
[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

[中文文档](README_CN.md)

> **⚠️ 项目尚未完成实机验证，请谨慎使用 / Most features have NOT been
> validated on real hardware; use with caution.**
>
> 2026-08-16: basic CH341A + SPI NOR operations have passed on one test setup;
> other programmer/chip combinations still require the validation checklist.

> **Alpha software warning** — do not use this project on chips whose contents
> you cannot afford to lose.

## Table of Contents

- [Background](#background)
- [Features](#features)
- [Install](#install)
- [Usage](#usage)
- [Hardware Backends](#hardware-backends)
- [Chip Database](#chip-database)
- [Firmware](#firmware)
- [Development](#development)
- [Maintainers](#maintainers)
- [Contributing](#contributing)
- [License](#license)

## Background

UniProgrammer is a modern rewrite of classic CH341/CH347 programming tools.
The protocol layer is ported from [flashrom](https://www.flashrom.org/) and
[IMSProg](https://github.com/bigbigmdm/IMSProg), with a clean separation
between chip commands and the USB/serial transport underneath.

## Features

- Programmer support
  - CH341A, CH347T, CH347F
  - Serprog over serial
  - HIDProg (reserved placeholder)
- Protocols: SPI NOR, SPI NAND, I2C EEPROM, Microwire EEPROM,
  SPI EEPROM, DataFlash AT45
- Read / write / erase / verify with live progress
- Chip database with JEDEC auto-detection and manual selection
  (lightly obfuscated on disk, see [Chip Database](#chip-database))
- Dark / light / follow-system themes
- Settings dialog persisted to `Setting.set` (INI) with migration from
  browser storage
- Voltage regulation panel with guarded power-on flow
- About dialog with dynamic version and chip database statistics
- SPI NAND bad-block modes (Skip / Bypass / Ignore), BBM LUT read/write,
  on-die ECC control, OTP and parameter-page read, per-chip dummy/plane/die
  configuration
- Hex editor: edit, undo, search, goto, fill, checksum
- Native file dialogs on Windows; Linux support in progress

## Install

### Prebuilt artifacts

Alpha builds are attached to GitHub Releases starting at `v0.3.0-alpha.2`.

- Windows: portable build (libusb backend) produced by CI.
  The official `CH34X.DLL` is not distributed with the source tree for
  licensing reasons; the Windows DLL backend is built locally with the
  vendor DLL installed next to the project.
- Linux: `chip-validator` built on Ubuntu 24.04 with WebKitGTK.

### Build from source

Windows (PowerShell):

```powershell
cd 3.Software
.\build-menu.ps1
```

Linux:

```bash
cd 3.Software
./build-menu.sh
```

## Usage

1. Connect the programmer and select its type in the left panel.
2. Click **Connect**, then **Detect**. The JEDEC ID is looked up in the chip
   database.
3. Load a binary file or read the chip into the hex editor.
4. Use **Read / Write / Erase / Verify** as needed.

For chips without a JEDEC ID (I2C, Microwire), select
Type → Vendor → Model manually.

## Hardware Backends

The HAL trait in `3.Software/src-tauri/src/ch34x.rs` is the boundary between
chip protocols and hardware transports.

- `hal-dll`: official CH34X.DLL backend (Windows default)
- `hal-libusb`: rusb/libusb backend (Linux default, Windows optional)

Backend selection is a compile-time Cargo feature. See
[CONTRIBUTING.md](CONTRIBUTING.md) for build commands.

## Chip Database

`chiplib.bin` is the authoritative database and is lightly obfuscated on disk
(FFW-style per-byte mask + rotate). `chiplib.xml` is an obfuscated fallback,
not a readable source file; both are decoded in memory only and no plaintext
database file is shipped or left in the working directory.

Maintenance tools (also see `cargo run --example chipdb_tool -- help`):

```bash
# Merge a TSV chip table (insert missing, enrich existing attributes)
cargo run --example chipdb_tool -- merge src-tauri/chiplib.bin chips.tsv

# Add or replace one chip by JEDEC ID
cargo run --example chipdb_tool -- add src-tauri/chiplib.bin 5E3213 \
  Zbit ZB25D40B SPI_NOR page=256 size=524288 sector=4096 block=65536

# Enrich from IMSProg.Dat fields (fills missing values only)
cargo run --example chipdb_tool -- \
  src-tauri/chiplib.bin IMSProg.Dat --backup
```

## Firmware

A bare-metal serprog firmware for the STM32F103 Blue Pill is available under
`2.Firmware/serprog-bluepill`. It turns a Blue Pill into a serprog SPI
programmer over USART1.

## Development

- Rust + Node.js 22
- `cargo test` for backend tests
- Code quality: run `.\lint.ps1` (Windows) or `./lint.sh` (Linux) inside
  `3.Software` to execute ESLint, Prettier, `cargo fmt` and `cargo clippy`;
  CI enforces the same checks on `main` and pull requests
- Hardware validation is required before declaring a release stable;
  checklist: [EN](4.Docs/hardware-validation-nand-45.md) /
  [中文](4.Docs/hardware-validation-nand-45_CN.md)

## Maintainers

- [M0rt1s0114](https://github.com/M0rt1s0114)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) (English) /
[CONTRIBUTING_CN.md](CONTRIBUTING_CN.md) (中文).

## License

[GPL-3.0-or-later](LICENSE)
