---
title: 守护进程
description: 为多个客户端运行共享的 Open Interpreter 应用服务器守护进程。
---

默认情况下，`interpreter` 在前台进程中运行所有内容：终端 UI 和运行时位于同一进程中，退出 UI 会干净地结束会话。正常使用时不需要守护进程。

对于需要多个客户端共享同一运行时的场景——例如编辑器集成和同一机器上的终端——Open Interpreter 也可以将其应用服务器以后台 **守护进程** 的形式运行，供客户端连接。

## 管理守护进程

```bash
interpreter app-server daemon start
interpreter app-server daemon restart
interpreter app-server daemon stop
interpreter app-server daemon version
```

`start` 启动一个监听本地 unix socket 的守护进程，并在其健康后返回。再次调用 `start` 时会复用已健康运行的守护进程，而不是生成第二个实例。

## 连接客户端

将任意 app-server 客户端（包括终端 UI）指向守护进程的端点：

```bash
interpreter --remote <ws-url-or-unix-socket>
```

有关远程端点请参阅 [Server deployments](/docs/remote)，有关编程客户端请参阅 [SDK](/docs/sdk)。

## 守护进程的存放位置

守护进程的运行时文件位于你的 Open Interpreter 主目录下，默认是 `~/.openinterpreter`，也可以通过 `INTERPRETER_HOME` 环境变量覆盖。守护进程日志是排查启动问题的首要检查位置。

## 故障排查

- **`start` 报告应用服务器正在运行但未被管理** — 另一个进程已在守护进程 socket 上监听。请停止该进程或使用不同的主目录。
- **更新或崩溃后出现卡顿** — 运行 `interpreter app-server daemon stop` 然后重新启动。
