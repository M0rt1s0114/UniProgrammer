# UniProgrammer

> A cross-platform NAND/NOR SPI flash programmer with a pluggable hardware abstraction layer.

[![License: GPL v3+](https://img.shields.io/badge/License-GPLv3+-blue.svg)](LICENSE)
[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

[中文文档](README_CN.md)

> **Alpha software warning** — this project has not been validated on real
> hardware yet. Do not use it on chips whose contents you cannot afford to lose.

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
- Dark cross-platform UI (English / Chinese switch)
- Hex editor: edit, undo, search, goto, fill, checksum
- Native file dialogs on Windows; Linux support in progress

## Install

### Prebuilt artifacts

Alpha builds are attached to GitHub Releases starting at `v0.1.0-alpha.1`.

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

`chiplib.bin` is the authoritative database; `chiplib.xml` is the readable
source and fallback. The database is enriched from `IMSProg.Dat` fields:

```bash
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
- Hardware validation is required before declaring a release stable

## Maintainers

- [M0rt1s0114](https://github.com/M0rt1s0114)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[GPL-3.0-or-later](LICENSE)
