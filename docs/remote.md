---
title: Remote App Server
description: Connect a TUI client to an app-server WebSocket endpoint.
---

Open Interpreter's public launcher starts a local app-server-backed runtime for
the TUI. Advanced integrations can connect a TUI client to a remote app-server
WebSocket endpoint.

```bash
interpreter --remote ws://127.0.0.1:9000
```

If the endpoint requires a bearer token, store it in an environment variable:

```bash
export INTERPRETER_REMOTE_TOKEN=...
interpreter --remote wss://agent.example.com \
  --remote-auth-token-env INTERPRETER_REMOTE_TOKEN
```

Tokens are intended for secure transports. For plain `ws://`, use localhost or
another explicitly trusted local endpoint.

Remote mode is an advanced integration surface. Most users should run
`interpreter` normally; the TUI starts an embedded local runtime and does not
require a daemon. If you explicitly start the optional shared daemon, the TUI
can reuse its Unix socket when the launch settings are compatible. See
[Daemon](/docs/daemon).
