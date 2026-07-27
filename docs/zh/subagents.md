---
title: 子代理
description: 使用专门的辅助代理进行调查、审查和并行工作。
---

子代理是独立的代理线程，可与主会话并行工作。
它们适用于隔离调查、广泛的代码搜索、审查过程或并行探索。

当前构建默认启用多代理特性：

```toml
[features]
multi_agent = true
```

## 在 TUI 中

使用：

```text
/agent
```

当任务受益于并行工作时，主代理也可以显式生成子代理。

## 内置角色

常见的内置角色包括：

| 角色 | 用途 |
| ---- | ------- |
| `default` | 通用辅助。 |
| `worker` | 专注执行或调查。 |
| `explorer` | 以阅读为主的发现和摘要。 |

可用角色可能因构建和配置而异。

## 设置

```toml
[agents]
max_threads = 6
max_depth = 1
job_max_runtime_seconds = 1800
```

| 键 | 含义 |
| --- | ------- |
| `max_threads` | 最大并发代理线程数。 |
| `max_depth` | 代理可以生成其他代理的最大深度。 |
| `job_max_runtime_seconds` | CSV/批处理工作者作业的默认超时时间（秒）。 |

## 自定义代理

在配置中定义角色：

```toml
[agents.explorer]
description = "Inspect code and report findings without editing."
developer_instructions = "Stay read-only. Prefer rg and direct file references."
model = "gpt-5.1-codex"
model_reasoning_effort = "medium"
sandbox_mode = "read-only"
```

有用的可选字段包括 `nickname_candidates`、`mcp_servers` 和技能配置。

## 权限

子代理继承当前的沙箱和批准姿态，除非其角色配置对其进行限制。即使子代理不是当前可见的线程，批准提示仍可能出现。
