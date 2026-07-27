---
title: 快速入门
description: 安装 Open Interpreter，打开一个项目，并运行您的第一个编码代理会话。
---

Open Interpreter 是一个基于 Codex CLI 表面的终端编码代理，已适配为提供商无关的本地使用。它可以检查您的代码库、编辑文件、运行命令、审查差异，并在之后继续工作。

<Steps>
  <Step title="安装">
    ```bash
    curl -fsSL https://www.openinterpreter.com/install | sh
    ```

    在 Windows 上，运行：

    ```powershell
    irm https://www.openinterpreter.com/install.ps1 | iex
    ```
  </Step>
  <Step title="打开项目">
    ```bash
    cd my-project
    i
    ```

    `i` 是 `interpreter` 的简写；这两个命令都可启动交互式会话。
  </Step>
  <Step title="选择提供商">
    第一次运行时会引导您完成提供商设置。您可以使用 ChatGPT 登录、使用 API 密钥、通过 Ollama 或 LM Studio 连接本地模型，或配置其他兼容的提供商。

    您可以稍后使用 `/model` 更换提供商。
  </Step>
  <Step title="提出修改请求">
    输入具体请求：

    ```text
    add a /health endpoint that returns the build sha
    ```

    Open Interpreter 会读取项目，提出工作计划，编辑文件，并在活动沙箱和授权设置下运行命令。
  </Step>
  <Step title="批准操作">
    在默认的本地工作流中，Open Interpreter 可以在当前工作区内工作，并在需要更高权限的操作前请求确认。使用 `/permissions` 可在会话期间查看或更改此设置。
  </Step>
  <Step title="稍后继续">
    ```bash
    interpreter resume --last
    ```

    `resume` 会保留之前的对话、上下文和工作目录。
  </Step>
</Steps>

## 常用首个命令

| 任务 | 命令 |
| ---- | ------- |
| 启动 TUI | `i` 或 `interpreter` |
| 带提示启动 | `interpreter "explain this repo"` |
| 不使用 TUI 运行一次 | `interpreter exec "summarize the current diff"` |
| 继续上一次会话 | `interpreter resume --last` |
| 查看当前更改 | `/review` 或 `interpreter exec review --uncommitted` |
| 选择模型/提供商 | `/model` |
| 更改权限 | `/permissions` |

## 常见设置答案

| 如果您需要… | 前往 |
| --------------- | ------- |
| 验证已安装的版本、更新或卸载 | [Install](/docs/install) |
| 连接 Ollama 或 LM Studio，包括远程本地服务器 | [Local models](/docs/models#local-models) |
| 连接托管的模型提供商 | [Providers](/docs/providers) |
| 设置 API 密钥或了解凭证存储位置 | [Authentication](/docs/authentication) |
| 配置命令、文件系统和网络边界 | [Sandbox & approvals](/docs/sandbox) |
| 在应用中嵌入 Open Interpreter | [SDK](/docs/sdk) |

## 下一页

<CardGroup cols={2}>
  <Card title="交互模式" href="/docs/interactive">
    Composer、快捷键、图片、提示、批准、审查和后台工作。
  </Card>
  <Card title="配置" href="/docs/config">
    默认值、配置文件、提供商、模型设置、功能标志和项目配置。
  </Card>
  <Card title="沙箱与批准" href="/docs/sandbox">
    本地命令执行的约束方式以及 Open Interpreter 何时请求确认。
  </Card>
  <Card title="AGENTS.md" href="/docs/agents_md">
    用于构建命令、约定和注意事项的持久化项目指令。
  </Card>
</CardGroup>
