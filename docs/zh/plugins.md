---
title: 插件
description: 捆绑技能、MCP 服务器、钩子以及相关扩展文件。
---

插件打包可重用的 Open Interpreter 扩展。插件可以包含技能、MCP 服务器定义、钩子以及其他应一起迁移的配置。

插件仍在实验阶段，默认可能被禁用：

```toml
[features]
plugins = true
```

## 插件结构

```text
my-plugin/
├── .codex-plugin/
│   └── plugin.json
├── skills/
├── mcp/
└── hooks/
```

清单描述插件及其打包的扩展点。

## 市场

兼容 Codex 的插件市场命令可能出现在底层工具中。在公共 Open Interpreter 启动器中，优先使用已安装或本地的、针对当前发行版有文档说明的插件。

## 信任

在启用插件前请审查其内容。插件可能携带作为钩子、MCP 服务器或技能脚本运行的代码，这些代码会通过正常的信任、沙箱和批准控制机制运行。
