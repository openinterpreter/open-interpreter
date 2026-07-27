---
title: Agent 客户端协议（ACP）
description: 从兼容 ACP 的编辑器和客户端中运行 Open Interpreter。
---

Open Interpreter 可以作为一个 [Agent Client Protocol](https://agentclientprotocol.com/) agent 运行。ACP 允许编辑器或其他 UI 通过 stdio 启动 agent 进程、创建会话、发送提示、流式传输 assistant 消息、显示工具进度并请求权限，而无需抓取终端输出。

从当前目录的 [兼容 ACP 的客户端](https://agentclientprotocol.com/get-started/clients) 中选择一个，然后按下面所示配置客户端以启动 Open Interpreter。

当你希望在编辑器的 agent 面板或其他结构化 agent UI 中使用 Open Interpreter 时，请使用 ACP。对于脚本请使用 [非交互模式](/docs/exec)，对于应用-服务器集成请使用 [SDK](/docs/sdk)，当另一个 agent 需要将 Open Interpreter 作为工具调用时请使用 [MCP 服务器](/docs/mcp-server)。

## 启动 ACP 代理

大多数 ACP 客户端会为你启动代理。将客户端配置为启动：

```bash
interpreter acp
```

该进程通过 stdin/stdout 使用 ACP 通信。不要将其包装在交互式终端 UI 中。

## 环境

ACP 代理使用与终端 CLI 相同的 Open Interpreter home、配置、认证、provider、MCP、skills、sandbox 和审批设置。

常见环境变量：

```bash
INTERPRETER_HOME="$HOME/.openinterpreter"
OPENAI_API_KEY="..."
ANTHROPIC_API_KEY="..."
KIMI_API_KEY="..."
MOONSHOT_API_KEY="..."
```

如果客户端允许你传递环境变量，优先在客户端或你的 shell 配置文件中设置 provider 密钥。你也可以在 `$INTERPRETER_HOME/config.toml` 中配置 providers；详见 [Configuration](/docs/config) 和 [Providers](/docs/providers)。

## Zed

[Zed](https://zed.dev/docs/ai/external-agents) 在 agent 面板中对 ACP 提供原生支持。将 Open Interpreter 作为 Zed 设置中的自定义 agent server 添加：

```json
{
  "agent_servers": {
    "Open Interpreter": {
      "type": "custom",
      "command": "interpreter",
      "args": ["acp"],
      "env": {
        "INTERPRETER_HOME": "/Users/you/.openinterpreter"
      }
    }
  }
}
```

打开 agent 面板，创建一个新的 external-agent 线程，然后选择 `Open Interpreter`。

## JetBrains IDEs

[JetBrains AI Assistant](https://www.jetbrains.com/help/ai-assistant/acp.html) 在 AI Chat 中支持 ACP agents。添加自定义 agent，或编辑 `~/.jetbrains/acp.json`：

```json
{
  "default_mcp_settings": {
    "use_idea_mcp": true,
    "use_custom_mcp": true
  },
  "agent_servers": {
    "Open Interpreter": {
      "command": "interpreter",
      "args": ["acp"],
      "env": {
        "INTERPRETER_HOME": "/Users/you/.openinterpreter"
      }
    }
  }
}
```

如果你希望 Open Interpreter 通过客户端接收 IDE 上下文和工具，请启用 IntelliJ MCP 设置。

## VS Code

VS Code 本身不内置 ACP 支持，但社区的 ACP 客户端（例如 [ACP extension](https://marketplace.visualstudio.com/items?itemName=strato-space.acp-plugin)）使用相同的 `agent_servers` 结构：

```json
{
  "agent_servers": {
    "Open Interpreter": {
      "command": "interpreter",
      "args": ["acp"],
      "env": {
        "INTERPRETER_HOME": "/Users/you/.openinterpreter"
      }
    }
  }
}
```

保存配置后，使用扩展的 agent 选择器启动会话。

## 客户端将获得的内容

Open Interpreter 暴露：

- 会话的创建、关闭、加载和列出。
- 沙箱模式：`read-only`、`workspace-write` 和 `danger-full-access`。
- 在已配置的 provider 支持的情况下的模型和推理控制。
- 流式 assistant 消息和推理摘要。
- 对 shell 命令、文件更改、MCP 调用、网页搜索、图像操作、子 agent 以及其他 Open Interpreter 工具的工具进度显示。
- 针对命令执行和文件更改的权限请求。

ACP 客户端负责 UI。Open Interpreter 负责模型选择、provider 传输、指令、工具、审批、沙箱以及会话状态。

## 故障排查

如果客户端找不到 `interpreter`，请使用绝对命令路径：

```json
{
  "command": "/usr/local/bin/interpreter",
  "args": ["acp"]
}
```

如果认证失败，请在具有相同 `INTERPRETER_HOME` 的终端中先启动 `interpreter` 一次，完成 provider 登录或 API-key 设置，然后重启 ACP 客户端。

如果客户端挂起，请确保它是直接通过 stdio 启动 `interpreter acp`。ACP 客户端不应在没有 `acp` 子命令的情况下启动 `interpreter`，因为那会启动终端 UI 而不是协议服务器。
