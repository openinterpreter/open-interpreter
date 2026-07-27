---
title: OpenAI SDK
description: Use Open Interpreter through OpenAI's Codex SDK.
---

Open Interpreter does not maintain a separate SDK. It is a drop-in replacement
for Codex surfaces, so SDK integrations should use OpenAI's Codex SDK and point
the launched agent process at Open Interpreter.

For the TypeScript SDK, an existing integration needs one changed line:

```diff
-const codex = new Codex();
+const codex = new Codex({ codexPathOverride: "interpreter" });
```

The SDK continues to speak the Codex exec protocol; only the launched binary
changes. Run `scripts/test-codex-sdk-compat.sh` from a source checkout for a
provider-free smoke test against an installed `interpreter` binary.

Use the upstream SDK docs for the complete API:

- [Codex SDK](https://developers.openai.com/codex/sdk)
- [Codex app server](https://developers.openai.com/codex/app-server)
- [Agent Client Protocol](/docs/acp)

## Python App-Server SDK

For Python integrations that use `codex_app_server`, launch
`interpreter app-server` instead of `codex app-server`.

```python
from codex_app_server import AppServerConfig, Codex

oi_server = AppServerConfig(
    launch_args_override=(
        "interpreter",
        "app-server",
        "--listen",
        "stdio://",
    ),
)

with Codex(config=oi_server) as codex:
    thread = codex.thread_start(
        model_provider="moonshotai",
        model="kimi-k3",
        config={"harness": "kimi-code"},
    )
    result = thread.run("Review this repo and list the first migration step.")
    print(result.final_response)
```

The important part is the process override. The SDK still speaks the Codex
app-server protocol; Open Interpreter supplies the compatible app server.

## TypeScript SDK

For TypeScript automation, install OpenAI's SDK and point the binary override at
`interpreter`:

```ts
import { Codex } from "@openai/codex-sdk";

const codex = new Codex({
  codexPathOverride: "interpreter",
  config: {
    model_provider: "moonshotai",
    harness: "kimi-code",
  },
});

const thread = codex.startThread({ model: "kimi-k3" });
const result = await thread.run(
  "Review this repo and list the first migration step.",
);

console.log(result);
```

For OpenAI models, omit the harness override and use the same model names you
would use from the CLI.

## CI

If you only need one job to run to completion, prefer `interpreter exec` over
embedding the SDK:

```bash
interpreter exec \
  -c 'model_provider="moonshotai"' \
  -c 'harness="kimi-code"' \
  -m "kimi-k3" \
  "Review this pull request and report blocking issues."
```

Use the SDK when you need to keep a thread alive, stream events into an
application, handle approvals programmatically, or embed the agent in a larger
workflow.
