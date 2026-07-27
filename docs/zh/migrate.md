---
title: 迁移
description: 将 Codex 或其他兼容的代理设置迁移到 Open Interpreter。
---

Open Interpreter 可以复用大量 Codex 风格的本地设置，因为 CLI 界面和配置模型紧密相关。

## 通常需要迁移的内容

| 源项目 | Open Interpreter 目标 |
| ----------- | ---------------------------- |
| 项目说明 | `AGENTS.md` |
| 配置 | `~/.openinterpreter/config.toml` 或 `.openinterpreter/config.toml` |
| 技能 | `.agents/skills/` 或 `~/.agents/skills/` |
| MCP 配置 | `[mcp_servers]` |
| 钩子 | `hooks.json` 或内联 `[hooks]` |
| 斜杠命令工作流 | 技能或项目说明 |
| 子代理 | `[agents]` 配置 |
| 最近会话 | 本地会话历史（若支持） |

## 导入后审查

在依赖迁移后的设置之前，请审查以下内容：

- 具有自定义身份验证、头部或传输方式的 MCP 服务器
- 运行本地命令的钩子
- 技能脚本及其引用
- 代理权限和工具限制
- 依赖 shell 插值或路径占位符的提示模板

## Codex 主目录

Open Interpreter 使用 `~/.openinterpreter/` 作为其用户状态存储位置。如果您之前使用过 Codex，请在迁移期间检查两个主目录：

```text
~/.codex/
~/.openinterpreter/
```

不要盲目复制机密信息。建议重新进行身份验证或使用环境变量。
