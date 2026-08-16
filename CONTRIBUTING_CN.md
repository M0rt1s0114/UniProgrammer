# 为 UniProgrammer 做贡献

[English](CONTRIBUTING.md)

感谢你对 UniProgrammer 的关注！UniProgrammer 是一个跨平台 SPI Flash 编程器，
带有可插拔的硬件抽象层（HAL）。本文档说明如何搭建项目、修改代码以及提交变更。

## 基本规则

- **提交信息必须使用英文。**
- 遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范。
- 大型功能请先在 issue 中讨论，再提交 Pull Request。
- 本项目会擦除和写入真实 Flash 芯片。永远不要对内容无法承受丢失的芯片
  执行写入或擦除测试。
- 不要提交修改过的厂商二进制文件（例如 `CH34X.DLL`）。厂商二进制文件只能
  由维护者基于官方发布版本更新。

## 开发环境

需要的工具：

- Rust（stable）以及 `cargo`、`rustfmt`、`clippy`
- Node.js 22+ 与 npm
- 平台依赖
  - Windows：WebView2（通常已预装）
  - Linux：WebKitGTK 4.1、GTK 3、libusb-1.0、libudev，以及 Tauri 所需的
    标准 Linux 依赖

## 构建

构建菜单脚本会选择 HAL 后端并执行完整 release 构建。

Windows（PowerShell）：

```powershell
.\build-menu.ps1
```

Linux / macOS：

```bash
./build-menu.sh
```

后端选择规则：

- 不启用任何 feature：按平台默认（`Windows` -> `hal-dll`，其他 -> `hal-libusb`）
- `--features hal-libusb`：强制使用 rusb/libusb 后端
- `--features hal-dll`：强制使用 CH34X.DLL 后端（仅 Windows）

## 提交信息

所有提交使用英文 Conventional Commits：

```
type(scope): summary
```

类型：

| Type | 用途 |
| --- | --- |
| `feat` | 新功能 |
| `fix` | 缺陷修复 |
| `docs` | 文档 |
| `style` | 仅格式调整 |
| `refactor` | 不改变行为的代码重构 |
| `perf` | 性能优化 |
| `test` | 测试 |
| `build` | 构建系统或依赖 |
| `ci` | CI 配置 |
| `chore` | 维护性任务 |
| `revert` | 回滚某次提交 |

示例：

```
feat(hal): add Windows DLL backend
fix(serprog): correct S_BUSTYPE and O_SPIOP opcodes
refactor(chiplib): replace XML fallback with typed loader
chore(release): 0.2.1-alpha.5
```

破坏性变更使用 `!` 或 `BREAKING CHANGE:` 脚注：

```
feat(hal)!: split backends into separate compile features
```

## 分支与 Pull Request

- 从 `main` 创建短英文分支：
  `feat/...`、`fix/...`、`docs/...`、`chore/...`
- 每个 Pull Request 只关注一个变更。
- 为变更的逻辑添加或更新测试。
- 确保本地 `cargo check`、`cargo test` 以及
  [代码风格](#代码风格) 中的 lint 套件通过。

## 代码风格

推送前请在 `3.Software` 下运行完整 lint 套件：

Windows（PowerShell）：

```powershell
.\lint.ps1
```

Linux / macOS：

```bash
./lint.sh
```

该套件检查：

- 前端：`npm run lint`（ESLint 10 + typescript-eslint +
  eslint-plugin-vue）以及 `npm run format:check`（Prettier）
- 后端：`cargo fmt --check` 与 `cargo lint`
  （`cargo clippy --all-targets -- -D warnings`；参见
  `3.Software/src-tauri/.cargo/config.toml`）

CI 通过 `.github/workflows/lint.yml` 在每次 `main` 推送和 Pull Request 上
运行同样的检查。

单独命令：

```bash
# 前端（在 3.Software 下运行）
npm run lint
npm run lint:fix
npm run format:check
npm run format

# 后端（在 3.Software/src-tauri 下运行）
cargo fmt --check
cargo lint
```

前端风格说明：

- Vue 3 `<script setup lang="ts">`
- 用户可见字符串放在 `3.Software/src/i18n/index.ts`
- 新的可复用 UI 组件放在 `3.Software/src/components`

## 测试

至少运行：

```bash
cargo test
```

硬件相关改动需要手动验证，可使用下表作为检查清单：

| 后端 | 芯片 | 读 ID | 擦除 | 读取 | 写入 | 校验 |
| --- | --- | --- | --- | --- | --- | --- |
| CH341A DLL | SPI NOR | | | | | |
| CH347T DLL | SPI NOR | | | | | |
| CH347F DLL | SPI NOR | | | | | |
| libusb | SPI NOR | | | | | |
| Serprog | SPI NOR | | | | | |

- 2026-08-16：CH341A DLL + SPI NOR 基础读/写/擦/校验已在单台测试环境通过；
  后续按实际测试继续填写。

## 芯片数据库

- `chiplib.bin` 是权威数据库，磁盘上使用轻量混淆。
- `chiplib.xml` 是同样混淆的回退文件，不是可读源文件。
- 解码只发生在内存中；不得提交或在工作目录留下明文芯片库。
- 批量更新优先使用 `chipdb_tool merge <bin> <chips.tsv>`，单颗芯片使用
  `chipdb_tool add ...`，避免覆盖已有补全字段。

## 版本管理

- 语义化版本 2.0.0
- Git tag 使用 `v` 前缀：`v0.2.1-alpha.5`
- 预发布版本：`-alpha.N`、`-beta.N`、`-rc.N`
- 推送 `v*` tag 会触发 release workflow

## 许可证

UniProgrammer 采用
[GPL-3.0-or-later](https://www.gnu.org/licenses/gpl-3.0.html) 许可。
参与贡献即表示你同意你的贡献以相同条款发布。
