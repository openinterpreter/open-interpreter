---
title: 沙盒与批准
description: 控制本地命令的行为以及 Open Interpreter 何时先询问。
---

Open Interpreter 有两种独立的安全控制：

- 沙盒模式控制本地命令执行的技术边界。
- 批准策略控制代理何时暂停并向您提问。

在 TUI 中使用 `/permissions` 检查或更改当前的姿态。

## 沙盒模式

| 模式 | 行为 |
| ---- | -------- |
| `read-only` | 命令可以检查允许的文件，但不能写入。 |
| `workspace-write` | 命令可以在活动工作区根目录内写入。除非启用，否则网络关闭。 |
| `danger-full-access` | 没有本地沙盒边界。仅在您有意信任的环境中使用。 |

设置默认值：

```toml
sandbox_mode = "workspace-write"
```

一次性覆盖：

```bash
interpreter --sandbox read-only "audit the auth flow"
```

## 批准策略

| 策略 | 行为 |
| ------ | -------- |
| `untrusted` | 在可能更改状态的操作之前询问。 |
| `on-request` | 在沙盒内运行，并在升级前询问。 |
| `never` | 不询问。沙盒是唯一的防护措施。 |

```toml
approval_policy = "on-request"
```

`--yolo` 和 `--dangerously-bypass-approvals-and-sandbox` 会移除批准提示和沙盒。仅在外部沙盒中使用，例如一次性虚拟机或隔离容器。

## 工作区写入

为会话授予额外的可写根目录：

```bash
interpreter --add-dir ../shared-lib
```

为旧的 workspace-write 沙盒启用网络：

```toml
[sandbox_workspace_write]
network_access = true
```

如需精确的网络白名单，请使用[Permissions](/docs/permissions)。

## 受保护路径

即使在可写根目录内，诸如 `.git/` 和代理配置目录等敏感控制目录也应视为受保护。如果代理需要更改它们，请仔细审查请求。

## 操作系统强制执行

Open Interpreter 使用与 Codex CLI 相同的本地沙盒架构：

| 平台 | 强制模型 |
| -------- | ----------------- |
| macOS | Seatbelt 配置文件。 |
| Linux / WSL | Bubblewrap、seccomp 以及可用时的相关内核沙盒。 |
| Windows | 在已配置的情况下使用原生 Windows 沙盒；WSL 使用 Linux 模型。 |

当请求的策略无法强制执行时，Open Interpreter 应该关闭失败，而不是静默地在未沙盒的环境中运行。

## 推荐默认设置

| 场景 | 建议设置 |
| --------- | ------------------ |
| 审查不熟悉的代码 | `sandbox_mode = "read-only"`, `approval_policy = "on-request"` |
| 日常受信任仓库工作 | `workspace-write` 加 `on-request` |
| 在隔离的运行器中进行 CI | `workspace-write` 加 `never` |
| 一次性全访问环境 | `danger-full-access` 加 `never` |

如果不确定，请从 `workspace-write` 和 `on-request` 开始。
