---
title: 认证
description: 使用 ChatGPT 登录、API 密钥，或连接本地及兼容的提供者。
---

Open Interpreter 与提供者无关。首次运行时会询问您希望如何进行认证，随后可通过 `/model` 更改。

## ChatGPT 登录

启动 TUI 并选择 ChatGPT 登录：

```bash
interpreter
```

这会打开基于浏览器的登录流程，并将可刷新的凭证存储在已配置的凭证存储中。

## API 密钥

API 密钥最适合用于 CI、无头机器以及显式的提供者计费。

```bash
export OPENAI_API_KEY=sk-...
interpreter
```

其他提供者使用各自的环境变量，例如 `ANTHROPIC_API_KEY`，或使用其提供者条目中配置的变量。

## 本地提供者

当您希望模型流量保持在本机时使用本地运行器：

| Provider | 备注 |
| -------- | ----- |
| Ollama | 启动 Ollama 并在 `/model` 中选择它，或使用 `--oss`。 |
| LM Studio | 启动本地服务器并在 `/model` 中选择 LM Studio。 |

```bash
interpreter --oss "summarize this repo with my local model"
```

## 兼容的提供者

在配置中添加 OpenAI 兼容的提供者：

```toml
model_provider = "acme"
model = "acme-coder"

[model_providers.acme]
name = "Acme"
base_url = "https://api.acme.example/v1"
env_key = "ACME_API_KEY"
wire_api = "responses"
```

随后：

```bash
export ACME_API_KEY=...
interpreter
```

## 凭证存储

```toml
cli_auth_credentials_store = "auto" # "auto" | "keyring" | "file"
```

Open Interpreter 将用户状态存储在 `~/.openinterpreter/` 下。如果使用文件存储，请将 `auth.json` 视作密码。

## 登出

在 TUI 中：

```text
/logout
```

或在兼容的登录界面中，使用可用的登出命令。
