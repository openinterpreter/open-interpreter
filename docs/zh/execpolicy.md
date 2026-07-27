---
title: 执行策略
description: 将命令分类为安全、风险或阻止的规则。
---

Execution policy 是沙箱下面的规则层。它在每个命令运行前检查并标记该命令。

| 标签   | 含义                                             |
| ------- | ------------------------------------------------- |
| `safe`  | 常规、低风险。无需提示直接通过。                 |
| `unsafe`| 可能改变系统状态。需要审批。                     |
| `forbid`| 始终阻止。                                        |

Open Interpreter 随附一个合理的默认策略。大多数用户从不编辑它。只有当你想要更严格的控制或在共享系统上运行时才需要查看此页面。

## 所在位置

策略从 `config.toml` 加载。每条规则都是一个匹配命令的模式以及对应的动作。

```toml
[[execpolicy.rules]]
match = "ls"
action = "safe"

[[execpolicy.rules]]
match = "rm *"
action = "unsafe"

[[execpolicy.rules]]
match = "rm -rf /"
action = "forbid"
```

规则从上到下评估。第一个匹配的规则生效。

## 与审批的交互方式

Execution policy 是代理的第一道过滤。策略标记命令后：

1. `forbid` 直接阻止命令。
2. `safe` 在不提示的情况下运行。
3. `unsafe` 交由你的审批模式处理（参见 [Sandbox & approvals](/docs/sandbox)）。

因此，`safe` 规则可以减少你的一天中的审批次数，而 `forbid` 规则则保证即使你意外按下 `y`，命令也永远不会执行。

## 常见模式

让你信任的 lint 与 test 命令完全不触发提示：

```toml
[[execpolicy.rules]]
match = "pnpm test*"
action = "safe"

[[execpolicy.rules]]
match = "pnpm lint*"
action = "safe"
```

对所有破坏性操作强制确认：

```toml
[[execpolicy.rules]]
match = "git push --force*"
action = "unsafe"

[[execpolicy.rules]]
match = "drop database*"
action = "forbid"
```

<Tip>
模式使用类 shell 的通配符。在自动化使用之前，可使用 `interpreter execpolicy check '<command>'` 测试规则。
</Tip>
