---
title: Harness
description: Harness 标识、路由兼容性以及每种模式的更改内容。
---

Harness 模式是 Open Interpreter 的一个新增功能。它会修改面向模型的提示、工具模式、消息转换以及响应处理，同时保持原生 Open Interpreter 运行时不变。

公共运行时不会调用真实的外部代理 CLI。

## 配置

```toml
harness = "kimi-code"
harness_guidance = true
```

一次运行示例：

```bash
interpreter -c harness='"kimi-code"' "solve this task"
```

## Harness 标识

| Harness 标识 | Wire API | 请求路由 | 参考 |
| --- | --- | --- | --- |
| unset or `""` | `responses` | Responses API | 原生 Open Interpreter/Codex 兼容表面 |
| unset or `""` | `chat` | Chat Completions 兼容性 | 通用 OpenAI 兼容聊天提供商 |
| `claude-code` | `responses`, `chat`, or `messages` | Claude Code 在提供商传输层上的塑形 | [Claude Code](https://docs.anthropic.com/en/docs/claude-code/overview) 完整代理表面 |
| `claude-code-bare` | `responses`, `chat`, or `messages` | Claude Code 在提供商传输层上的塑形 | [Claude Code](https://docs.anthropic.com/en/docs/claude-code/overview) 裸配置 |
| `zcode` | `messages` | Anthropic Messages harness | ZCode 形态的 GLM 编码代理表面 |
| `deepseek-tui` | `chat` | Chat harness | [DeepSeek TUI](https://github.com/DeepSeek-TUI/DeepSeek-TUI) / [CodeWhale](https://www.codewhale.ai/) |
| `kimi-code` | `chat` | Chat harness | 当前的 [Kimi Code](https://www.kimi.com/code/docs/en/) / [GitHub](https://github.com/MoonshotAI/kimi-code) 配置 |
| `kimi-cli` | `chat` | Chat harness | 旧版 [Kimi CLI](https://moonshotai.github.io/kimi-cli/) / [GitHub](https://github.com/MoonshotAI/kimi-cli) 配置 |
| `qwen-code` | `chat` | Chat harness | [Qwen Code](https://qwenlm.github.io/qwen-code-docs/en/cli/index) / [GitHub](https://github.com/QwenLM/qwen-code) |
| `swe-agent` | `chat` | Chat harness | [SWE-agent](https://swe-agent.com/) / [GitHub](https://github.com/SWE-agent/SWE-agent) |
| `minimal` | `chat` | Chat harness | Open Interpreter 最小化聊天工具表面 |
| any other string | `chat` | Chat Completions 兼容性 | 自定义标记；无内建 harness 请求构造器 |

## 参考项目

以下链接指向外部产品或仓库，其面向用户的表面为对应的 harness 模式提供了参考：

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code/overview) 来自 Anthropic。
- ZCode，使用于 Anthropic Messages 兼容端点（如 Z.AI 的 Coding Plan 端点）的 GLM 编码代理表面。
- [DeepSeek TUI](https://github.com/DeepSeek-TUI/DeepSeek-TUI)，产品背景见 [CodeWhale](https://www.codewhale.ai/)。
- 当前的 [Kimi Code](https://www.kimi.com/code/docs/en/) 及其 [MoonshotAI/kimi-code](https://github.com/MoonshotAI/kimi-code) 仓库。
- 旧版 Python [Kimi CLI](https://moonshotai.github.io/kimi-cli/) 及其 [MoonshotAI/kimi-cli](https://github.com/MoonshotAI/kimi-cli) 仓库。
- [Qwen Code CLI](https://qwenlm.github.io/qwen-code-docs/en/cli/index) 及其 [QwenLM/qwen-code](https://github.com/QwenLM/qwen-code) 仓库。
- [SWE-agent](https://swe-agent.ai/) 及其 [SWE-agent/SWE-agent](https://github.com/SWE-agent/SWE-agent) 仓库。

## 路由兼容性

Harness 路由具有严格的匹配要求：

| Provider `wire_api` | 兼容的 harness |
| --- | --- |
| `responses` | 原生 Responses、`claude-code` 与 `claude-code-bare`。 |
| `chat` | 原生聊天兼容性、`claude-code`、`claude-code-bare`、`deepseek-tui`、`kimi-code`、`kimi-cli`、`qwen-code`、`swe-agent` 与 `minimal`。 |
| `messages` | `claude-code`、`claude-code-bare` 与 `zcode`。原生模式被拒绝，因为 Messages 需要 harness 本地的传输层。 |

这意味着 Anthropic 风格的提供商通常需要：

```toml
model_provider = "anthropic"
harness = "claude-code"
```

大多数兼容 OpenAI 的托管提供商使用 `wire_api = "chat"`，可以通过通用聊天兼容性或匹配的聊天 harness 运行。

## 自动 Harness 默认值

当配置中未设置 `harness` 时，Open Interpreter 可能会根据所选提供商/模型推断出一个默认值：

| 检测到的家族 | 默认 harness |
| --- | --- |
| Anthropic、Claude 模型 ID、Anthropic 基础 URL，或任何 `messages` 提供商 | `claude-code` |
| Kimi/Moonshot 提供商 ID、名称、基础 URL 或模型 ID | `kimi-code` |
| Qwen/QwQ/DashScope 提供商 ID、名称、基础 URL 或模型 ID | `qwen-code` |
| DeepSeek 提供商 ID、名称、基础 URL 或模型 ID | `claude-code-bare` |

显式的 `harness = "..."` 总是优先。

## 每种 Harness 的更改内容

### `claude-code`

在所选提供商的 Responses、Chat 或 Anthropic Messages 传输层上构建 Claude Code 形态的请求。它会添加 Claude Code 系统提示、思考配置、上下文管理设置、标题生成请求以及 Claude 形态的工具表面。针对特定传输层的头部和消息转换会在提供商需要时应用。

它将支持的工具映射为 Anthropic 工具定义，并包括 Bash/PowerShell、Read、Write、Edit、TodoWrite、Glob、Grep、网页搜索/获取、LSP、计划唤醒以及 Claude 风格子代理的处理程序。

### `claude-code-bare`

使用与 `claude-code` 相同的提供商传输层，但采用裸 Claude Code 配置。裸配置使用更小的提示/配置形态以及不同的输出默认设置。DeepSeek 会自动选择此配置。

### `zcode`

使用兼容 Anthropic Messages 的请求，并配以 ZCode 形态的系统提示、头部、工具、todo 与计划行为、技能、会话上下文以及子代理表面。工具调用仍在 Open Interpreter 的原生 Rust 运行时中执行。

该 harness 需要 `wire_api = "messages"`。内置的 Z.AI 与 Z.AI Coding Plan 提供商使用 `wire_api = "chat"`，因此在想使用实际 ZCode 路由时，请在 [Z.AI, GLM, and ZCode](/docs/zai-glm) 中配置 Messages 端点。

### `kimi-code`

使用兼容 Chat Completions 的请求，搭配当前 Kimi Code 提示、工具定义、prompt‑cache 键、思考配置和消息格式。支持的工具调用由 Open Interpreter 的原生 Rust 运行时执行；不会调用外部 Kimi 可执行文件。

这是 Kimi 与 Moonshot 提供商（包括 Kimi K3）的默认配置。Kimi Code 订阅使用 `kimi-for-coding` 提供商，并可通过内置的 Kimi 登录流程进行身份验证。Moonshot Platform API 密钥使用 `moonshotai` 提供商。

### `kimi-cli`

使用兼容 Chat Completions 的请求，配以旧版 Python Kimi CLI 形态的系统提示。包括工作目录列表、AGENTS.md 加载、Kimi 技能发现、prompt‑cache 键、推理力度映射以及 Kimi 工具模式。

Kimi 工具处理程序包括 Shell、ReadFile、WriteFile、StrReplaceFile、Glob、Grep、ReadMediaFile、SearchWeb、FetchUrl、SetTodoList、plan‑mode 控制、后台任务列表/输出/停止、AskUserQuestion 与 Agent。

仅在需要兼容已退役的 Python CLI 配置时使用此模式。新的 Kimi 会话应使用 `kimi-code`。

### `deepseek-tui`

使用兼容 Chat Completions 的请求，配以 DeepSeek TUI/CodeWhale 形态的系统提示。它会添加回合元数据、仓库上下文、在未找到项目说明时生成的项目指令，以及 DeepSeek TUI 的工具模式。

DeepSeek TUI 工具处理程序包括 shell、apply patch、edit/write/read file、list directory、grep/file search、git status/diff、diagnostics、checklist、plan 与 tool search。

### `qwen-code`

使用兼容 Chat Completions 的请求，并加入 Qwen Code 启动上下文。它会在用户对话前插入一个合成的设置交换，包含日期、操作系统、当前目录以及一个小的文件夹列表。

Qwen 处理程序包括 read file、write file、edit、shell command、glob、grep、todo write、ask user question、plan exit 与 agent。

### `swe-agent`

采用类似 SWE‑agent 的讨论/指令循环，而不是工具模式。助手的响应会被解析为 shell 命令，Open Interpreter 注入相应的动作，并将命令输出作为观察返回。默认命令超时为 30 秒。

### `minimal`

使用紧凑的软件代理系统提示，并将函数工具映射为普通的 Chat Completions 工具列表。当提供商支持聊天工具但不需要特定的 harness 表面时，此模式非常有用。

## 指导

`harness_guidance = true` 默认启用。目前它仅为 `kimi-cli` 添加额外指导；其他 harness 会忽略此设置。

若想进行更严格的 harness 运行，可禁用它：

```toml
harness_guidance = false
```

## 相关配置

```toml
model_provider = "moonshotai"
model = "kimi-k3"
harness = "kimi-code"

[model_providers.moonshotai]
name = "Moonshot AI"
base_url = "https://api.moonshot.ai/v1"
env_key = "MOONSHOT_API_KEY"
wire_api = "chat"
```

有关提供商和 wire‑api 细节，请参见 [Providers](/docs/providers)。
