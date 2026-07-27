---
title: 应用服务器
description: 在本地 app-server 协议中嵌入 Open Interpreter。
---

Open Interpreter 使用基于 app-server 的运行时来提供交互式 TUI。相同的 app-server 协议可用于需要线程、流式事件、批准、会话状态、模型选择以及在其他应用中获取 MCP 状态的高级集成。

大多数自动化应从 [Non-interactive mode](/docs/exec) 或 [SDK](/docs/sdk) 开始。在构建更丰富的客户端时使用 app server。当您希望一个兼容 ACP 的编辑器或 UI 将 Open Interpreter 作为其编码代理启动时，请改用 [Agent Client Protocol](/docs/acp)。

## 启动本地服务器

使用 stdio 作为 SDK 风格的子进程：

```bash
interpreter app-server --listen stdio://
```

当需要单独客户端连接时使用 WebSocket 传输：

```bash
interpreter app-server --listen ws://127.0.0.1:9000
```

然后连接 TUI 客户端：

```bash
interpreter --remote ws://127.0.0.1:9000
```

## 安全远程访问

如果服务器不是严格本地的，请在其前端终止 TLS 并要求使用 Bearer 令牌：

```bash
interpreter --remote wss://agent.example.com \
  --remote-auth-token-env INTERPRETER_REMOTE_TOKEN
```

在客户端将令牌存储在指定的环境变量中。不要在公共网络上暴露未经身份验证的 app-server 监听器。

## 协议

该协议继承自 Codex app-server。Open Interpreter 保持该接口兼容，以便现有的 Codex app-server 客户端可以将 `interpreter app-server` 用作启动的进程。

使用 OpenAI 的 [Codex app-server 文档](https://developers.openai.com/codex/app-server) 了解完整的协议结构，然后使用上面 Open Interpreter 特定的启动命令。
