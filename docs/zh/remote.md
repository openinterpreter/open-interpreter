---
title: 远程应用服务器
description: 将 TUI 客户端连接到 app-server 的 WebSocket 端点。
---

Open Interpreter 的公共启动器为 TUI 启动一个本地基于 app-server 的运行时。高级集成可以将 TUI 客户端连接到远程 app-server 的 WebSocket 端点。

```bash
interpreter --remote ws://127.0.0.1:9000
```

如果该端点需要 bearer token，请将其存储在环境变量中：

```bash
export INTERPRETER_REMOTE_TOKEN=...
interpreter --remote wss://agent.example.com \
  --remote-auth-token-env INTERPRETER_REMOTE_TOKEN
```

Token 旨在用于安全传输。对于普通的 `ws://`，请使用 localhost 或其他明确受信任的本地端点。

远程模式是一个高级集成接口。大多数用户应正常运行 `interpreter`；TUI 会启动嵌入式本地运行时，且不需要守护进程。如果你显式启动可选的共享守护进程，且启动设置兼容，TUI 可以复用其 Unix 套接字。参见 [Daemon](/docs/daemon).
