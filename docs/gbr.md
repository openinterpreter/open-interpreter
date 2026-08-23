---
title: Build Remote Agent
description: Pair a phone to Open Interpreter with Build Remote Agent (gbr/1). Spectator only. Loopback attach.
---

# Pair a phone with Build Remote Agent

Open Interpreter can use **Build Remote Agent** as a pairing device: the paid
iOS/Android app spectates (and can inject into) this desktop `interpreter`
session through the free MIT `gbr-agent`. Phone and PC never open ports to
each other.

Website: https://grokbuildremote.com/
Agent: https://github.com/LinespottingOrg/GrokBuildRemote-Agents (MIT)
Protocol: `gbr/1` · need agent **v0.6.0+**

Independent product by Linespotting AB. Not affiliated with xAI or SpaceX.

## Install + pair

```bash
# macOS / Linux
curl -fsSL https://grokbuildremote.com/install.sh | bash
gbr-agent version          # must print v0.6.0 or newer
gbr-agent pair             # QR in browser + printed 8-char code
gbr-agent run              # leave running
```

```powershell
# Windows
irm https://grokbuildremote.com/install.ps1 | iex
gbr-agent version
gbr-agent pair
gbr-agent run
```

Phone: open Build Remote Agent → **Scan QR from computer** (or type the 8-char
code). Sessions appear in the app. **Unpair** in Settings before changing PCs.
Force-close is not enough.

## Attach Open Interpreter

After `gbr-agent run`, attach only through loopback:

- HTTP Bot API: `http://127.0.0.1:8788`
- MCP stdio: `gbr-mcp`

```bash
curl -sS http://127.0.0.1:8788/health
curl -sS http://127.0.0.1:8788/v1/sessions
```

Stdio MCP (clone once, then point Open Interpreter at it):

```bash
git clone https://github.com/LinespottingOrg/GrokBuildRemote-Agents.git
cd GrokBuildRemote-Agents/mcp/gbr-mcp && npm install
```

`~/.openinterpreter/config.toml`:

```toml
[mcp_servers.gbr]
command = "node"
args = ["GrokBuildRemote-Agents/mcp/gbr-mcp/bin/gbr-mcp.js"]
```

Use an absolute path to `gbr-mcp.js`. CLI equivalent:

```bash
interpreter mcp add gbr -- node /absolute/path/to/gbr-mcp.js
```

Phone is spectator + veto, not orchestrator. Do not commit mailbox keys.
Phone **Settings → Bot API** is the only place a relay key is copied.

See also [MCP](/docs/mcp) and [Remote app server](/docs/remote).
