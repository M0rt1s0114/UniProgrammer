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

`plugins/builtin/` 存放 L1 必需插件（`uni.ui.webview`、`uni.hal`、
`uni.chipdb`、`uni.hexview`、`uni.proto`）。这些清单由主程序在启动时检查，
缺失或无效会导致启动失败；**请勿删除或改名该目录**。

新插件默认不启用；请在 设置 → 插件 中启用并确认其能力声明。
必需插件始终启用，不能禁用。

## UI 插件与 `unipkg://` 协议

`kind = "ui"` 的插件是一个自包含的静态 Web 包。主程序注册了
`unipkg://<插件名>/<路径>` 自定义协议，并把请求映射到该插件目录：

- 根路径 `unipkg://<插件名>/` → `[package].entry` 指向的页面
  （内置插件约定为 `dist/index.html`）；
- 其它路径直接映射到插件目录下同名文件，例如
  `unipkg://uni.hexview/assets/app.js` → `<插件目录>/assets/app.js`。

主窗口本身加载 `unipkg://uni.ui.webview/`，因此 **整个 UI 壳是一个普通
L1 插件**，用另一份 `uni.ui.webview` 包覆盖内置目录即可替换（重启生效）。

内置 UI 插件在开发模式（`cargo tauri dev`）下由 `npm run dev` 提供热更新：
`uni.ui.webview` 使用 1420 端口，`uni.hexview` 使用 1421 端口。

## `uni.hexview` 贡献点契约（v1）

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

替换 `uni.hexview` 时只需保持入口页可达并实现上述消息；界面与实现完全自由。
