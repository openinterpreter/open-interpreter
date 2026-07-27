---
title: Agent Client Protocol
description: Run Open Interpreter from ACP-compatible editors and clients.
---

Open Interpreter can run as an [Agent Client Protocol](https://agentclientprotocol.com/)
agent. ACP lets an editor or another UI start an agent process over stdio,
create sessions, send prompts, stream assistant messages, show tool progress,
and request permissions without scraping a terminal.

Choose from the current directory of
[ACP-compatible clients](https://agentclientprotocol.com/get-started/clients),
then configure the client to launch Open Interpreter as shown below.

Use ACP when you want Open Interpreter inside an editor agent panel or another
structured agent UI. Use [Non-interactive mode](/docs/exec) for scripts, the
[SDK](/docs/sdk) for app-server integrations, and [MCP server](/docs/mcp-server)
when another agent needs to call Open Interpreter as a tool.

## Start the ACP Agent

Most ACP clients start the agent for you. Configure the client to launch:

```bash
interpreter acp
```

The process speaks ACP over stdin/stdout. Do not wrap it in an interactive
terminal UI.

## Environment

The ACP agent uses the same Open Interpreter home, config, auth, provider, MCP,
skills, sandbox, and approval settings as the terminal CLI.

Common environment variables:

```bash
INTERPRETER_HOME="$HOME/.openinterpreter"
OPENAI_API_KEY="..."
ANTHROPIC_API_KEY="..."
KIMI_API_KEY="..."
MOONSHOT_API_KEY="..."
```

If the client lets you pass environment variables, prefer setting provider keys
there or in your shell profile. You can also configure providers in
`$INTERPRETER_HOME/config.toml`; see [Configuration](/docs/config) and
[Providers](/docs/providers).

## Zed

[Zed](https://zed.dev/docs/ai/external-agents) has native ACP support in the
agent panel. Add Open Interpreter as a custom agent server in Zed settings:

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

Open the agent panel, create a new external-agent thread, and select
`Open Interpreter`.

## JetBrains IDEs

[JetBrains AI Assistant](https://www.jetbrains.com/help/ai-assistant/acp.html)
supports ACP agents from AI Chat. Add a custom agent, or edit
`~/.jetbrains/acp.json`:

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

Enable the IntelliJ MCP settings if you want Open Interpreter to receive IDE
context and tools through the client.

## VS Code

VS Code does not ship native ACP support, but community ACP clients such as the
[ACP extension](https://marketplace.visualstudio.com/items?itemName=strato-space.acp-plugin)
use the same `agent_servers` shape:

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

Use the extension's agent picker to start a session after saving the config.

## What the Client Gets

Open Interpreter exposes:

- Session creation, closing, loading, and listing.
- Sandbox modes: `read-only`, `workspace-write`, and `danger-full-access`.
- Model and reasoning controls where the configured provider supports them.
- Streaming assistant messages and reasoning summaries.
- Tool progress for shell commands, file changes, MCP calls, web search, image
  operations, subagents, and other Open Interpreter tools.
- Permission requests for command execution and file changes.

The ACP client owns the UI. Open Interpreter owns model selection, provider
transport, instructions, tools, approvals, sandboxing, and session state.

## Troubleshooting

If the client cannot find `interpreter`, use an absolute command path:

```json
{
  "command": "/usr/local/bin/interpreter",
  "args": ["acp"]
}
```

If authentication fails, start `interpreter` once in a terminal with the same
`INTERPRETER_HOME`, finish provider login or API-key setup, then restart the ACP
client.

If the client hangs, make sure it is launching `interpreter acp` directly over
stdio. ACP clients should not launch `interpreter` without the `acp` subcommand,
because that starts the terminal UI instead of the protocol server.
