---
title: MCP 服务器
description: 将 Open Interpreter 作为工具暴露给另一个 MCP 客户端。
---

Open Interpreter 可以通过 stdio 运行为 MCP 服务器。这使得另一个 MCP 客户端可以将 Open Interpreter 会话作为工具启动和继续。

```bash
interpreter mcp-server
```

服务器提供用于启动会话和继续现有线程的工具。工具参数与普通 Open Interpreter 的配置界面相同：prompt、working directory、model、profile、sandbox、approval policy，以及每次运行的配置覆盖。

## 何时使用

当需要另一个代理框架或内部工具将 Open Interpreter 作为编码代理调用时，请使用 MCP 服务器模式。

如果只需要一次命令行任务，请使用 `interpreter exec`；如果在 Python 或 TypeScript 应用中直接嵌入 Open Interpreter，请使用 [SDK](/docs/sdk)。

## 客户端配置

MCP 客户端通常需要类似以下的 stdio 命令条目：

```json
{
  "mcpServers": {
    "open-interpreter": {
      "command": "interpreter",
      "args": ["mcp-server"]
    }
  }
}
```

保持 MCP 客户端和 Open Interpreter 处于与 CLI 相同的信任边界。被调用的会话仍然可以运行工具、请求批准，并根据您配置的 sandbox 和权限修改工作空间。
