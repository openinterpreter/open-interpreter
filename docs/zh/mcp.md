---
title: MCP
description: 连接模型上下文协议（MCP）服务器，以便使用外部工具和数据。
---

模型上下文协议（MCP）使 Open Interpreter 能够调用本地或远程服务器公开的外部工具。MCP 适用于问题追踪器、私有文档、数据库、内部 CLI 以及其他应当显式提供而非通过 Shell 命令即兴实现的功能。

## 添加 Stdio 服务器

在 `~/.openinterpreter/config.toml` 中配置服务器：

```toml
[mcp_servers.linear]
command = "npx"
args = ["-y", "@linear/mcp-server"]
env = { LINEAR_API_KEY = "env:LINEAR_API_KEY" }
```

或使用 CLI：

```bash
interpreter mcp add linear -- npx -y @linear/mcp-server
```

## 添加 HTTP 服务器

```toml
[mcp_servers.docs]
url = "https://mcp.example.com"
bearer_token_env_var = "DOCS_MCP_TOKEN"
```

CLI 形式：

```bash
interpreter mcp add docs --url https://mcp.example.com \
  --bearer-token-env-var DOCS_MCP_TOKEN
```

## 管理服务器

```bash
interpreter mcp list
interpreter mcp get docs
interpreter mcp remove docs
interpreter mcp login docs
interpreter mcp logout docs
```

OAuth 登录适用于支持 OAuth 的可流式 HTTP 服务器。

在 TUI 中，使用 `/mcp` 查看已加载的服务器，使用 `/mcp verbose` 查看工具详情。

## 审批模式

设置服务器默认：

```toml
[mcp_servers.docs]
command = "docs-mcp"
default_tools_approval_mode = "prompt"
```

覆盖单个工具：

```toml
[mcp_servers.docs.tools.search]
approval_mode = "approve"
```

常用模式：

| 模式   | 行为                                   |
| ------ | -------------------------------------- |
| `prompt` | 在工具运行前询问用户。                 |
| `approve` | 自动允许该工具运行。                   |
| `auto`   | 让活动策略自行决定。                   |

## 工具过滤

仅公开选定的工具：

```toml
[mcp_servers.docs]
command = "docs-mcp"
enabled_tools = ["search", "read"]
disabled_tools = ["delete"]
```

`disabled_tools` 在 `enabled_tools` 之后生效。

## 超时与启动

```toml
[mcp_servers.docs]
command = "docs-mcp"
startup_timeout_sec = 10
tool_timeout_sec = 60
required = true
enabled = true
```

如果 `required = true`，当该服务器无法初始化时，启动或恢复会失败。

## 环境

对于 stdio 服务器：

```toml
[mcp_servers.local]
command = "my-mcp"
args = ["--stdio"]
cwd = "/Users/me/project"
env = { TOKEN = "env:MY_TOKEN" }
env_vars = ["PATH", "HOME"]
```

对于 HTTP 服务器：

```toml
[mcp_servers.remote]
url = "https://mcp.example.com"
http_headers = { "X-Client" = "open-interpreter" }
env_http_headers = { "Authorization" = "MCP_AUTH_HEADER" }
```

## 安全注意事项

将 MCP 工具视为任何其他能够读取、写入或调用外部系统的工具。对具破坏性的工具使用 `prompt`，避免在代码中内联存储机密，并在为项目启用之前审查服务器配置。
