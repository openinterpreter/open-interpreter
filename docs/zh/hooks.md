---
title: 钩子
description: 在代理生命周期的特定时点运行受信任的脚本。
---

钩子让你能够在 Open Interpreter 事件的前后运行确定性的命令。可用于策略检查、日志记录、提示扫描、自定义上下文注入或运行后验证。

钩子默认是启用的：

```toml
[features]
hooks = true
```

仅在你明确希望没有生命周期脚本时才禁用它们：

```toml
[features]
hooks = false
```

## 钩子所在位置

Open Interpreter 在活动配置层旁边发现钩子：

| Location | Scope |
| -------- | ----- |
| `~/.openinterpreter/hooks.json` | 用户 |
| `~/.openinterpreter/config.toml` | 用户内联钩子 |
| `.openinterpreter/hooks.json` | 受信任的项目 |
| `.openinterpreter/config.toml` | 受信任的项目内联钩子 |
| Enabled plugins | 插件捆绑的钩子 |

如果匹配到多个来源，它们全部运行。高优先级配置不会替代低优先级层的钩子。

## 信任

非受管理的命令钩子必须在运行前经过审查并获得信任。Open Interpreter 会针对精确的钩子定义记录信任状态，因此钩子一旦更改需要重新审查。

使用：

```text
/hooks
```

已经在外部验证钩子的自动化流程可以传入 `--dangerously-bypass-hook-trust`，但这应当很少使用。

## 事件

| Event | 触发时机 |
| ----- | -------- |
| `SessionStart` | 会话启动、恢复、清除或压缩时。 |
| `UserPromptSubmit` | 用户提示即将发送时。 |
| `PreToolUse` | 在支持的 shell、patch 或 MCP 工具运行之前。 |
| `PermissionRequest` | 在显示批准提示之前。 |
| `PostToolUse` | 在支持的 shell、patch 或 MCP 工具完成之后。 |
| `PreCompact` | 在上下文压缩之前。 |
| `PostCompact` | 在上下文压缩之后。 |
| `SubagentStart` | 子代理启动时。 |
| `SubagentStop` | 子代理停止时。 |
| `Stop` | 回合即将结束时。 |

## JSON Form

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "^Bash$",
        "hooks": [
          {
            "type": "command",
            "command": "python3 .openinterpreter/hooks/pre_tool_use.py",
            "timeout": 30,
            "statusMessage": "Checking command"
          }
        ]
      }
    ]
  }
}
```

## TOML Form

```toml
[[hooks.PreToolUse]]
matcher = "^Bash$"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "python3 .openinterpreter/hooks/pre_tool_use.py"
timeout = 30
statusMessage = "Checking command"
```

## 匹配器

匹配器是正则表达式。支持的匹配目标包括工具事件的工具名称、`SessionStart` 的 `startup|resume|clear|compact`，以及压缩事件的 `manual|auto`。

示例：

```text
Bash
^apply_patch$
Edit|Write
mcp__filesystem__read_file
startup|resume
manual|auto
```

## 钩子输入与输出

命令钩子会在 stdin 接收一个 JSON 对象，包含 `session_id`、`cwd`、`hook_event_name`、`model` 等字段以及与事件相关的字段。

某些事件可以向模型可见的上下文中添加内容。某些事件可以阻止或拒绝工具调用。例如，`PreToolUse` 钩子可以拒绝一次命令：

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Blocked by repository policy."
  }
}
```

钩子是安全防护措施，而不是沙箱和批准的替代方案。
