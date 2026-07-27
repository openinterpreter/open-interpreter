---
title: 记忆
description: 在会话之间复用个人偏好和持久上下文。
---

记忆是一个可选功能，用于在会话之间携带有用的个人上下文。默认情况下它是关闭的。

```toml
[features]
memories = true

[memories]
use_memories = true
generate_memories = true
```

## 控制

| 键 | 目的 |
| --- | ---- |
| `memories.use_memories` | 将相关记忆注入到后续会话中。 |
| `memories.generate_memories` | 从会话中生成新的记忆候选。 |
| `memories.extract_model` | 覆盖用于提取原始记忆的模型。 |
| `memories.consolidation_model` | 覆盖用于整合记忆的模型。 |
| `memories.disable_on_external_context` | 对使用外部上下文的会话跳过记忆生成。 |

## 使用时机

在稳定的个人偏好、重复出现的项目上下文或跨多个会话适用的工作流习惯中使用记忆。不要将其用于机密信息、临时任务状态或应存放在仓库文件中的信息。

## 检查

启用后，使用：

```text
/memories
```
