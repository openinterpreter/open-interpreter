---
title: 会话
description: 恢复、分叉、重命名、压缩并管理本地对话历史。
---

Open Interpreter 将对话本地存储在 `~/.openinterpreter/`，因此您可以稍后继续工作。

## 恢复

恢复当前目录下最新的会话：

```bash
interpreter resume --last
```

打开选择器：

```bash
interpreter resume
```

包含来自其他目录的会话：

```bash
interpreter resume --all
```

恢复已知的会话 ID：

```bash
interpreter resume <SESSION_ID>
```

## 分叉

分叉会从旧会话创建一个新线程。原始会话保持不变。

```bash
interpreter fork --last
interpreter fork <SESSION_ID>
interpreter fork --all
```

在 TUI 中，使用 `/fork` 或 `/side`。

## 执行会话

非交互式会话也可以恢复：

```bash
interpreter exec resume --last "continue with the implementation"
```

在交互式恢复时使用 `--include-non-interactive`，当您希望执行会话出现在选择器中时。

## 压缩

长对话可以压缩为更短的摘要：

```text
/compact
```

当活跃模型接近其上下文限制时，也可以自动压缩。如果需要显式阈值，请在配置中设置 `model_auto_compact_token_limit`。

## 历史控制

禁用已保存的记录历史：

```toml
[history]
persistence = "none"
```

限制历史大小：

```toml
[history]
max_bytes = 104857600
```

## 守护进程

`interpreter` 默认在前台运行。要在多个客户端之间共享同一运行时，请将 app-server 作为后台守护进程运行，并使用以下命令停止它：

```bash
interpreter app-server daemon stop
```

详见 [守护进程](/docs/daemon) 页面。
