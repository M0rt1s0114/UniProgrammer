# UniProgrammer

> 一款跨平台 NAND/NOR SPI Flash 编程器，具备可插拔的硬件抽象层（HAL）。

[![License: GPL v3+](https://img.shields.io/badge/License-GPLv3+-blue.svg)](LICENSE)
[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

[English](README.md)

> **⚠️ 项目未经实机验证，请谨慎使用 / This project has NOT been validated on
> real hardware; use with caution.**

> **Alpha 版本警告** — 本项目尚未经过真实硬件验证。不要用于内容无法承受丢失的芯片。

## 目录

- [背景](#背景)
- [功能特性](#功能特性)
- [安装](#安装)
- [使用方法](#使用方法)
- [硬件后端](#硬件后端)
- [芯片数据库](#芯片数据库)
- [固件](#固件)
- [开发](#开发)
- [维护者](#维护者)
- [贡献](#贡献)
- [许可证](#许可证)

## 背景

UniProgrammer 是对经典 CH341/CH347 编程工具的一次现代化重写。
协议层移植自 [flashrom](https://www.flashrom.org/) 与
[IMSProg](https://github.com/bigbigmdm/IMSProg)，在芯片命令与
USB/串口传输之间保持清晰分离。

## 功能特性

- 编程器支持
  - CH341A、CH347T、CH347F
  - Serprog（串口协议）
  - HIDProg（预留占位）
- 协议：SPI NOR、SPI NAND、I2C EEPROM、Microwire EEPROM、
  SPI EEPROM、DataFlash AT45
- 读 / 写 / 擦除 / 校验，实时进度显示
- 芯片数据库支持 JEDEC 自动识别与手动选型
- 深色跨平台界面（中英文切换）
- Hex 编辑器：编辑、撤销、搜索、跳转、填充、校验和
- Windows 原生文件对话框；Linux 支持开发中

## 安装

### 预构建产物

Alpha 构建产物会发布在 GitHub Releases，从 `v0.2.0-alpha.1` 开始。

- Windows：CI 生成 libusb 后端的便携版。出于许可证原因，官方
  `CH34X.DLL` 不随源码分发；Windows DLL 后端需要本地放置厂商 DLL 后构建。
- Linux：基于 Ubuntu 24.04 + WebKitGTK 构建的 `chip-validator`。

### 源码构建

Windows（PowerShell）：

```powershell
cd 3.Software
.\build-menu.ps1
```

Linux：

```bash
cd 3.Software
./build-menu.sh
```

## 使用方法

1. 连接编程器，在左侧面板选择对应型号。
2. 点击 **连接**，再点击 **检测**。JEDEC ID 会到芯片数据库里匹配。
3. 载入二进制文件，或将芯片读取到 Hex 编辑器。
4. 按需执行 **读取 / 写入 / 擦除 / 校验**。

没有 JEDEC ID 的芯片（I2C、Microwire）请手动选择
类型 → 厂商 → 型号。

## 硬件后端

`3.Software/src-tauri/src/ch34x.rs` 中的 HAL trait 是芯片协议与
硬件传输之间的边界。

- `hal-dll`：官方 CH34X.DLL 后端（Windows 默认）
- `hal-libusb`：rusb/libusb 后端（Linux 默认，Windows 可选）

后端选择是编译期 Cargo feature。构建命令见
[CONTRIBUTING.md](CONTRIBUTING.md)。

## 芯片数据库

`chiplib.bin` 是权威数据库；`chiplib.xml` 是可读源与回退方案。
数据库可从 `IMSProg.Dat` 补齐字段：

```bash
cargo run --example chipdb_tool -- \
  src-tauri/chiplib.bin IMSProg.Dat --backup
```

## 固件

`2.Firmware/serprog-bluepill` 提供面向 STM32F103 Blue Pill 的
裸机 serprog 固件，可将 Blue Pill 变成 USART1 串口 SPI 编程器。

## 开发

- Rust + Node.js 22
- 后端测试：`cargo test`
- 代码规范：在 `3.Software` 下运行 `.\lint.ps1`（Windows）或 `./lint.sh`
  （Linux），会依次执行 ESLint、Prettier、`cargo fmt` 与 `cargo clippy`；
  CI 会在 `main` 推送和 Pull Request 时自动执行同样检查
- 发布稳定版前必须完成真机验证；清单：
  [中文](4.Docs/hardware-validation-nand-45_CN.md) /
  [English](4.Docs/hardware-validation-nand-45.md)

## 维护者

- [M0rt1s0114](https://github.com/M0rt1s0114)

## 贡献

见 [CONTRIBUTING_CN.md](CONTRIBUTING_CN.md)（中文）/
[CONTRIBUTING.md](CONTRIBUTING.md)（English）。

## 许可证

[GPL-3.0-or-later](LICENSE)
