---
title: 配置参考
description: 重要的 config.toml 键和值表。
---

此参考列出了用户最常需要的 Open Interpreter 设置。每个字段的权威来源是生成的模式文件 `codex-rs/core/config.schema.json`。

## 顶层键

| 键 | 取值 | 用途 |
| --- | ------ | ------- |
| `model` | string | 默认模型 ID。 |
| `model_provider` | provider id | 用于 `model` 的提供商条目。 |
| `model_reasoning_effort` | `minimal`, `low`, `medium`, `high`, `xhigh` | 支持模型的推理预算。 |
| `model_reasoning_summary` | `auto`, `concise`, `detailed`, `none` | 显示多少推理摘要。 |
| `model_verbosity` | `low`, `medium`, `high` | 支持的 Responses API 模型的详细程度。 |
| `approval_policy` | `untrusted`, `on-request`, `never` | 在运行命令前何时询问。 |
| `approvals_reviewer` | `user`, `auto_review` | 谁审查符合条件的批准提示。 |
| `sandbox_mode` | `read-only`, `workspace-write`, `danger-full-access` | 本地命令沙盒。 |
| `default_permissions` | profile name | 选择一个 beta 权限配置文件。 |
| `personality` | `friendly`, `pragmatic`, `none` | TUI 通信风格。 |
| `web_search` | `cached`, `live`, `disabled` | 网络搜索行为。 |
| `log_dir` | path | 日志目录。 |
| `file_opener` | `vscode`, `vscode-insiders`, `windsurf`, `cursor`, `none` | 用于文件引用的编辑器。 |
| `harness` | string | Open Interpreter 框架兼容模式。 |
| `harness_guidance` | boolean | 在框架模式下允许 OI 指导。 |
| `check_for_update_on_startup` | boolean | 启用托管的独立更新检查。 |

## 功能标志

| 键 | 默认姿态 | 用途 |
| --- | --------------- | ------- |
| `features.apps` | off | 应用/连接器表面。 |
| `features.plugins` | off | 插件包。 |
| `features.hooks` | on | 生命周期钩子。 |
| `features.memories` | off | 持久记忆的生成/使用。 |
| `features.multi_agent` | on | 子代理工具和 `/agent`。 |
| `features.shell_tool` | on | 内置 shell 命令工具。 |
| `features.shell_snapshot` | on | Shell 环境快照。 |
| `features.unified_exec` | 默认开启，不支持时除外 | 基于 PTY 的 exec 工具。 |
| `features.undo` | off | 在支持的情况下提供撤销。 |
| `features.network_proxy` | off | 沙盒网络代理控制。 |

## 提供者表

```toml
[model_providers.example]
name = "Example"
base_url = "https://api.example.com/v1"
env_key = "EXAMPLE_API_KEY"
wire_api = "responses"
request_max_retries = 4
stream_max_retries = 5
stream_idle_timeout_ms = 300000
```

提供者认证也可以使用命令支持：

```toml
[model_providers.example.auth]
command = "example-token"
args = ["print"]
refresh_interval_ms = 300000
timeout_ms = 5000
```

## 沙盒表

```toml
[sandbox_workspace_write]
network_access = false
exclude_tmpdir_env_var = false
exclude_slash_tmp = false
writable_roots = ["/tmp/project-cache"]
```

如需更细粒度的访问控制，请使用权限配置文件，而不是在单个活动配置中混合两套系统。

## 权限配置文件

```toml
default_permissions = "project-edit"

[permissions.project-edit.filesystem]
":minimal" = "read"

[permissions.project-edit.filesystem.":workspace_roots"]
"." = "write"
"**/*.env" = "deny"

[permissions.project-edit.network]
enabled = true

[permissions.project-edit.network.domains]
"api.openai.com" = "allow"
"*.github.com" = "allow"
```

完整模型请参见[权限](/docs/permissions)。

## MCP 表

```toml
[mcp_servers.docs]
command = "docs-mcp"
args = ["--stdio"]
env = { DOCS_TOKEN = "env:DOCS_TOKEN" }
startup_timeout_sec = 10
tool_timeout_sec = 60
required = false
enabled_tools = ["search", "read"]
disabled_tools = ["delete"]
default_tools_approval_mode = "prompt"

[mcp_servers.docs.tools.search]
approval_mode = "approve"
```

HTTP MCP 服务器使用 `url`、`bearer_token_env_var`、`http_headers` 和 `env_http_headers`。

## 钩子

钩子可以内联配置：

```toml
[[hooks.PreToolUse]]
matcher = "^Bash$"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "python3 .openinterpreter/hooks/pre_tool_use.py"
timeout = 30
statusMessage = "Checking command"
```

或者在活动配置层旁的 `hooks.json` 中。参见[钩子](/docs/hooks)。

## 代理

```toml
[agents]
max_threads = 6
max_depth = 1
job_max_runtime_seconds = 1800

[agents.explorer]
description = "Investigate code and report findings without editing."
model_reasoning_effort = "medium"
sandbox_mode = "read-only"
```

参见[子代理](/docs/subagents)。
