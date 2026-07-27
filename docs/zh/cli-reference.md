---
title: CLI 参考
description: 主要的 Open Interpreter 命令和标志。
---

公共命令是 `interpreter`。不带子命令时它会启动
基于 app‑server 的终端 UI。

## 全局标志

| 标志 | 用途 |
| ---- | ---- |
| `PROMPT` | 可选的初始提示。 |
| `--image, -i <path[,path...]>` | 将图像文件附加到初始提示。 |
| `--model, -m <model>` | 覆盖配置的模型。 |
| `--oss` | 使用配置的本地开源提供商。 |
| `--local-provider <provider>` | 在使用 `--oss` 时使用 `ollama` 或 `lmstudio`。 |
| `--profile, -p <name>` | 加载配置文件。 |
| `--sandbox, -s <mode>` | 选择 `read-only`、`workspace-write` 或 `danger-full-access`。 |
| `--ask-for-approval, -a <policy>` | 选择 `untrusted`、`on-request` 或 `never`。 |
| `--cd, -C <path>` | 在其他工作目录中启动。 |
| `--add-dir <path>` | 添加一个可写的工作区根目录。 |
| `--search` | 请求实时网页搜索。 |
| `--enable <feature>` | 为本次运行启用功能。 |
| `--disable <feature>` | 为本次运行禁用功能。 |
| `--config, -c key=value` | 为本次运行覆盖配置。 |
| `--remote <ws-url>` | 将 TUI 连接到远程 app‑server 端点。 |
| `--remote-auth-token-env <var>` | 从环境变量读取远程 Bearer 令牌。 |
| `--no-alt-screen` | 禁用备用屏幕 TUI 模式。 |
| `--yolo` | 绕过批准和沙箱。危险操作。 |

## 命令

| 命令 | 用途 |
| ---- | ---- |
| `interpreter` | 启动 TUI。 |
| `interpreter resume` | 恢复交互式会话。 |
| `interpreter fork` | 分叉交互式会话。 |
| `interpreter exec` | 非交互式运行。 |
| `interpreter acp` | 通过 stdio 以 Agent Client Protocol 代理运行。 |
| `interpreter app-server daemon` | 管理可选的共享 [daemon](/docs/daemon)（`start`/`stop`/`restart`）。 |
| `interpreter mcp` | 管理 MCP 服务器配置。 |
| `interpreter mcp-server` | 通过 stdio 将 Open Interpreter 暴露为 MCP 服务器。 |
| `interpreter update` | 运行配置的安装程序以获取最新版本。 |

## 恢复

```bash
interpreter resume
interpreter resume --last
interpreter resume --all
interpreter resume <SESSION_ID>
```

## 分叉

```bash
interpreter fork
interpreter fork --last
interpreter fork --all
interpreter fork <SESSION_ID>
```

## Exec

```bash
interpreter exec "fix the failing test"
interpreter exec --json "summarize this repo"
interpreter exec resume --last "continue"
interpreter exec review --uncommitted
```

参见 [Non-interactive mode](/docs/exec)。

## MCP

```bash
interpreter mcp list
interpreter mcp get <name>
interpreter mcp add <name> -- <command...>
interpreter mcp add <name> --url https://mcp.example.com
interpreter mcp remove <name>
interpreter mcp login <name>
interpreter mcp logout <name>
```

参见 [MCP](/docs/mcp)。

## MCP Server

```bash
interpreter mcp-server
```

参见 [MCP server](/docs/mcp-server)。

## ACP

```bash
interpreter acp
```

参见 [Agent Client Protocol](/docs/acp)。

## 更新

```bash
interpreter update
```

交互式启动期间的自动检查由 `check_for_update_on_startup` 控制。

## 关于 Codex 兼容性的说明

Open Interpreter 保留了 Codex CLI 交互模型用于本地编码：
交互式 TUI、`exec`、会话、斜杠命令、MCP、沙箱、批准、技能、钩子和子代理。OpenAI 托管的 Codex Cloud 命令并不属于 Open Interpreter 的公开接口，除非未来的 Open Interpreter 发行版添加了等价功能。
