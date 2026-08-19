# UniProgrammer 插件目录

把第三方插件文件夹放到这个目录下，例如：

```
plugins/
  vnd.example.my-programmer/
    unipkg.toml
    plugin.exe        # 或 plugin.js
    chips.json        # 可选
```

主程序启动时会扫描 `plugins/` 下一层文件夹中的 `unipkg.toml`（或旧版
`manifest.toml`），同时扫描 `plugins/builtin/` 下一层文件夹。同名插件以
`plugins/` 下的用户安装副本优先。

## 命名规则

插件 ID 采用 `upt.<层次/宿主>.<组件>`：

- `upt.tauri` — Tauri UI 壳（L1 必需）；
- `upt.tauri.hexview` — 依附于 Tauri 壳的 HexViewer（L1 必需）；
  未来 Slint 壳对应 `upt.slint` / `upt.slint.hexview`；
- `upt.hal` / `upt.proto` / `upt.chipdb` — L1 基础层；
- `upt.hal.ch34x_dll` — CH341A / CH347T / CH347F 官方 DLL 后端（L2）；
- `upt.hal.ch34x_libusb` — CH341A / CH347T / CH347F libusb 后端（L2）。

`plugins/builtin/` 存放 L1 必需插件（`upt.tauri`、`upt.hal`、
`upt.chipdb`、`upt.tauri.hexview`、`upt.proto`）。这些清单由主程序在启动
时检查，缺失或无效会导致启动失败；**请勿删除或改名该目录**。

## L2 冷启动插件

`layer = "cold"` 的编程器适配器在启用/禁用后**必须重启程序**才会生效。
用户的启用状态保存在 `plugin-state.toml`（位于 `plugins/` 同级目录），
启动时随插件目录一起加载。

`plugins/builtin/upt.hal.ch34x_dll/` 与 `upt.hal.ch34x_libusb/` 是随程序
发布的内置 L2 示例：同一个 sidecar 源码分别按 DLL 与 libusb 后端编译，
manifest 指向 `upt_ch34x_sidecar_dll` / `upt_ch34x_sidecar_libusb`，
由 upt-hal 在启动时拉起、探测并注册。内置插件默认启用，也可以在插件
管理器中显式禁用。构建流程会把两个 sidecar 二进制复制进各自插件目录，
保证 `plugins/` 资源树自包含；CH34X.DLL 只复制进 DLL 后端包。

Sidecar 调试/验证面板默认在插件管理器中隐藏；需要时到 设置 →
“显示 Sidecar 插件面板（实验性）” 打开。

## UI 插件与 `unipkg://` 协议

`kind = "ui"` 的插件是一个自包含的静态 Web 包。主程序注册了
`unipkg://localhost/<插件名>/<路径>` 自定义协议，并把请求映射到该插件目录：

- 根路径 `unipkg://localhost/<插件名>/` → `[package].entry` 指向的页面
  （内置插件约定为 `dist/index.html`）；
- 其它路径直接映射到插件目录下同名文件，例如
  `unipkg://localhost/upt.tauri.hexview/dist/assets/app.js`
  → `<插件目录>/dist/assets/app.js`。

主窗口本身加载 `unipkg://localhost/upt.tauri/`，因此 **整个 UI 壳是
一个普通 L1 插件**，用另一份 `upt.tauri` 包覆盖内置目录即可替换
（重启生效）。

> Windows/WebView2 的 iframe 里不能直接用自定义 scheme URL；wry 要求写成
> `http://unipkg.localhost/<插件名>/...`（请求到达 Rust 端前会被还原为
> `unipkg://localhost/<插件名>/...`，同时该 origin 被 Tauri 视为本地页面，
> IPC 命令才会放行）。macOS/Linux 仍使用 `unipkg://localhost/...` 原形。
> UI 插件自身的构建产物也应当以 `<插件名>/dist/` 为 base。

## `upt.tauri.hexview` 贡献点契约（v1）

UI 壳把 HexViewer 包放入 `<iframe>` 加载，插件页面内不能使用 Tauri IPC，
只能通过 `window.postMessage` 与壳通信。消息方向约定：

壳 → 插件：

| type                 | 字段                                  | 含义                                          |
| -------------------- | ------------------------------------- | --------------------------------------------- |
| `uniprog:hex:init`   | `locale`, `theme`, `baseAddr`, `data` | 插件就绪后的完整初始化                        |
| `uniprog:hex:update` | `baseAddr`, `data`                    | 外部数据变化（读芯片 / 打开文件等）           |
| `uniprog:hex:theme`  | `theme`                               | `dark` / `light`，建议设置 `html[data-theme]` |
| `uniprog:hex:locale` | `locale`                              | `zh` / `en`                                   |

插件 → 壳：

| type                  | 字段               | 含义                              |
| --------------------- | ------------------ | --------------------------------- |
| `uniprog:hex:ready`   | —                  | 页面已挂载，可以接收 `init`       |
| `uniprog:hex:edit`    | `offset`, `value`  | 单字节编辑，壳原位更新权威缓冲区  |
| `uniprog:hex:replace` | `data: Uint8Array` | 整块替换（填充 / 撤销等批量操作） |
| `uniprog:hex:log`     | `level`, `message` | 转发到主日志面板                  |

替换 `upt.tauri.hexview` 时只需保持入口页可达并实现上述消息；界面与实现
完全自由。
