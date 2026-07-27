---
title: OpenAI SDK
description: 通过 OpenAI 的 Codex SDK 使用 Open Interpreter。
---

Open Interpreter 并未维护单独的 SDK。它是 Codex 接口的直接替代品，因此 SDK 集成应使用 OpenAI 的 Codex SDK，并将启动的代理进程指向 Open Interpreter。

对于 TypeScript SDK，只需在现有集成中更改一行：

```diff
-const codex = new Codex();
+const codex = new Codex({ codexPathOverride: "interpreter" });
```

SDK 仍然使用 Codex 执行协议；唯一变化的是启动的二进制文件。可在源码检出后运行 `scripts/test-codex-sdk-compat.sh` 对已安装的 `interpreter` 二进制进行无供应商的冒烟测试。

完整的 API 请参阅上游 SDK 文档：

- [Codex SDK](https://developers.openai.com/codex/sdk)
- [Codex app server](https://developers.openai.com/codex/app-server)
- [Agent Client Protocol](/docs/acp)

## Python App-Server SDK

对于使用 `codex_app_server` 的 Python 集成，启动 `interpreter app-server` 而不是 `codex app-server`。

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

关键在于进程覆盖。SDK 仍然遵循 Codex app‑server 协议；Open Interpreter 提供兼容的应用服务器。

## TypeScript SDK

对于 TypeScript 自动化，安装 OpenAI 的 SDK 并将二进制覆盖指向 `interpreter`：

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

使用 OpenAI 模型时，省略 harness 覆盖，并使用与 CLI 相同的模型名称。

## CI

如果只需要一次作业运行到完成，建议使用 `interpreter exec` 而不是嵌入 SDK：

```bash
interpreter exec \
  -c 'model_provider="moonshotai"' \
  -c 'harness="kimi-code"' \
  -m "kimi-k3" \
  "Review this pull request and report blocking issues."
```

当需要保持线程存活、将事件流式传输到应用、以编程方式处理批准，或在更大的工作流中嵌入代理时，才使用 SDK。
