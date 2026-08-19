# UniProgrammer 插件目录

把第三方插件文件夹放到这个目录下，例如：

```
plugins/
  vnd.example.my-programmer/
    manifest.toml
    plugin.exe        # 或 plugin.js
    chips.json        # 可选
```

主程序启动时只扫描 `plugins/` 下一层文件夹中的 `manifest.toml`。
新插件默认不启用；请在 设置 → 插件 中启用并确认其能力声明。
