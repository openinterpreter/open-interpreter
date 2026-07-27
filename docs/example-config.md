---
title: Example Config
description: A starting point for ~/.openinterpreter/config.toml.
---

Drop this into `~/.openinterpreter/config.toml` and edit it to match your
workflow. Every key here is optional.

```toml
# ---------------------------------------------------------------
# Model and provider
# ---------------------------------------------------------------

model_provider = "openai"
model = "gpt-5.1-codex"
model_reasoning_effort = "medium"
model_reasoning_summary = "auto"
personality = "pragmatic"
web_search = "cached"

# ---------------------------------------------------------------
# Harness compatibility
# ---------------------------------------------------------------

# Leave unset to choose the recommended harness for the selected provider.
# harness = "kimi-code"
harness_guidance = true

# ---------------------------------------------------------------
# Sandbox and approvals
# ---------------------------------------------------------------

sandbox_mode = "workspace-write"
approval_policy = "on-request"

[sandbox_workspace_write]
network_access = false
writable_roots = []

# ---------------------------------------------------------------
# Logging and history
# ---------------------------------------------------------------

log_dir = "~/.openinterpreter/log"

[history]
persistence = "save-all"
max_bytes = 104857600

# ---------------------------------------------------------------
# Profiles
# ---------------------------------------------------------------

[profiles.fast]
model = "gpt-5.1-codex-mini"
model_reasoning_effort = "low"

[profiles.review]
model = "gpt-5.1-codex"
model_reasoning_effort = "high"
sandbox_mode = "read-only"

# ---------------------------------------------------------------
# Custom provider
# ---------------------------------------------------------------

[model_providers.example]
name = "Example"
base_url = "https://api.example.com/v1"
env_key = "EXAMPLE_API_KEY"
wire_api = "responses"

# ---------------------------------------------------------------
# MCP servers
# ---------------------------------------------------------------

[mcp_servers.docs]
command = "docs-server"
default_tools_approval_mode = "prompt"

[mcp_servers.docs.tools.search]
approval_mode = "approve"

# ---------------------------------------------------------------
# Features
# ---------------------------------------------------------------

[features]
hooks = true
multi_agent = true
shell_tool = true
shell_snapshot = true
unified_exec = true
memories = false
apps = false
plugins = false
undo = false
```

Run with a profile:

```bash
interpreter --profile review
```

Override one value for a single run:

```bash
interpreter -c approval_policy='"never"' "fix the failing tests"
```

See [Configuration](/docs/config) and [Config reference](/docs/config-reference).
