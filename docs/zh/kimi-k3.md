---
title: Kimi K3
description: 在 Open Interpreter 中使用 Kimi Code（供应商推荐）的 Kimi K3。
---

[Kimi K3](https://www.kimi.com/blog/kimi-k3) 是 Kimi 面向代理式编码和知识工作的旗舰模型。Open Interpreter 使用 Rust 重新实现了供应商推荐的 [Kimi Code](https://www.kimi.com/coding/en) harness，使 K3 能够在类似 Codex 的界面中获得请求结构、工具、思考历史和默认设置。

## 使用 Kimi Code 订阅开始

安装 Open Interpreter，打开你的项目，并启动终端 UI：

```bash
curl -fsSL https://www.openinterpreter.com/install | sh
cd my-project
i
```

在 Windows 上，使用 PowerShell 安装，然后在项目中运行 `i`：

```powershell
irm https://www.openinterpreter.com/install.ps1 | iex
```

首次启动时，选择 **Kimi For Coding**，在浏览器中完成 Kimi 登录，然后选择 **Kimi K3**。在已有会话中，打开 `/model` 并进行相同的选择。切换到 K3 时请启动新会话，以便使用全新的提示缓存和思考历史。

如果你已经拥有兼容的 Kimi Code API 密钥，可以直接启动：

```bash
KIMI_API_KEY="..." interpreter \
  -c 'model_provider="kimi-for-coding"' \
  -m k3
```

Open Interpreter 会自动为 Kimi 供应商选择 `kimi-code`。无需安装或运行外部的 Kimi Code CLI。

## 使用 Moonshot Platform API 密钥

Kimi K3 也可以通过 Moonshot Platform API 以 `kimi-k3` 形式使用：

```bash
MOONSHOT_API_KEY="..." interpreter \
  -c 'model_provider="moonshotai"' \
  -m kimi-k3
```

针对单个非交互式任务：

```bash
MOONSHOT_API_KEY="..." interpreter exec \
  -c 'model_provider="moonshotai"' \
  -m kimi-k3 \
  "Review this repository and fix the highest-impact bug."
```

此供应商同样会推断使用 Kimi Code harness。你可以使用 `/harness` 确认或更改当前的 harness。

## 上下文与推理

Kimi 目前建议在选择 K3 时使用全新会话，因为切换模型会使现有的提示缓存失效。K3 在启动时使用最大推理力度。Kimi Code 订阅在 Moderato 方案提供 256K 上下文窗口，在 Allegretto 以及更高级别方案上提供最高 1M 令牌的上下文窗口。

请参阅 Kimi 当前的 [模型配置](https://www.kimi.com/code/docs/en/kimi-code/models.html)了解授权和推理细节。

## 价格

截至 2026 年 7 月 16 日，Kimi 列出了以下 Kimi Code 订阅价格：

| 方案 | 月付 | 年付（按月计） | Kimi Code 积分 | K3 上下文 |
| --- | ---: | ---: | ---: | --- |
| Moderato | $19 | $15 | 1× | 256K |
| Allegretto | $39 | $31 | 5× | 最高 100 万 |
| Allegro | $99 | $79 | 15× | 最高 100 万 |
| Vivace | $199 | $159 | 30× | 最高 100 万 |

Kimi 对 K3 的直接 API 定价为：每百万次缓存命中输入令牌 $0.30，每百万次缓存未命中输入令牌 $3.00，及每百万输出令牌 $15.00。价格和方案授权可能会变动；购买前请查看 Kimi 的 [当前会员定价](https://www.kimi.com/help/membership/membership-pricing) 与 [K3 推出公告](https://www.kimi.com/blog/kimi-k3)。

## 计算机使用

Kimi K3 可以使用 Open Interpreter 捆绑的 QA 技能来操作和测试界面。让它测试网页应用时，它可以在真实浏览器中使用 [`agent-browser`](https://github.com/vercel-labs/agent-browser)。让它测试原生桌面应用时，它可以使用 [`trycua`](https://github.com/trycua/cua) 进行计算机操作。

例如：

```text
Run this app, test the sign-in flow like a user, and fix anything that breaks.
```

普通的沙箱和审批设置仍然适用。请保持请求具体，并审查涉及账户或外部系统的操作。

## ACP 与 Codex SDK

通过配置启动 `interpreter acp`，即可在任何受支持的编辑器或客户端中使用 Kimi K3；请参阅 [ACP 指南](/docs/acp) 和当前的 [ACP 客户端目录](https://agentclientprotocol.com/get-started/clients)。

现有的 Codex SDK 集成只需将其二进制覆盖指向 Open Interpreter：

```diff
-const codex = new Codex();
+const codex = new Codex({ codexPathOverride: "interpreter" });
```

将 `model_provider` 设置为 `kimi-for-coding` 并使用模型 `k3`，或设置为 `moonshotai` 并使用模型 `kimi-k3`。完整的 TypeScript 与 Python 示例请参阅 [Codex SDK 指南](/docs/sdk)。
