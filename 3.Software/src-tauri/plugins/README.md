# UniProgrammer 插件目录

把第三方插件文件夹放到这个目录下，例如：

```
plugins/
  vnd.example.my-programmer/
    manifest.toml
    plugin.exe        # 或 plugin.js
    chips.json        # 可选
```

主程序启动时会扫描 `plugins/` 下一层文件夹中的 `unipkg.toml`（或旧版
`manifest.toml`），同时扫描 `plugins/builtin/` 下一层文件夹。

`plugins/builtin/` 存放 L1 必需插件清单（`uni.ui.webview`、`uni.hal`、
`uni.chipdb`、`uni.hexview`、`uni.proto`）。这些清单由主程序在启动时检查，
缺失或无效会导致启动失败；**请勿删除或改名该目录**。

新插件默认不启用；请在 设置 → 插件 中启用并确认其能力声明。
必需插件始终启用，不能禁用。
