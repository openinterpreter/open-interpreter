---
title: 贡献
description: 如何报告问题、提出更改并在 Open Interpreter 上工作。
---

Open Interpreter 是 Codex 的一个分支，专注于更窄的产品范围。我们接受影响此分支所有权的贡献：Open Interpreter 的低内存多标签运行时、模型/提供商适配层行为、Open Interpreter 特有的 TUI 与引导流程更改、安装程序/更新行为以及产品文档。

对于非 Open Interpreter 特有的通用 Codex CLI 行为，请在上游的 [OpenAI Codex](https://github.com/openai/codex) 贡献。将通用修复保持在上游有助于两个项目，并降低长期的分叉漂移。

## 首次贡献步骤

在打开拉取请求之前，请打开或加入一个议题，以确保行为和范围清晰。对以下内容的更改尤其需要提前讨论：

- 低内存多标签行为和共享运行时工作；
- 适配层选择、适配层兼容性以及提供商特定的 coding‑agent 行为；
- Open Interpreter 特有的 TUI、引导流程、模型选择器或状态 UI；
- 安装程序、独立布局和更新行为；
- 为 Open Interpreter 支持的提供商生成的提供商/模型目录；
- Open Interpreter 文档、示例以及迁移指南。

主要属于上游 Codex 关注的更改应在上游 Codex 仓库中进行。示例包括通用沙箱内部实现、通用 MCP 协议行为、通用 app‑server 协议更改，或与 Open Interpreter 产品方向无关的广泛 CLI 功能。

## 拉取请求

保持更改聚焦且易于审查。如果更改影响用户行为，请在本文件夹中更新相关文档，并在适当时更新 CLI 帮助信息。

代码更改时：

- 对你修改的区域运行格式化工具；
- 首先运行最窄的有意义的测试目标；
- 在实际可行的情况下，为行为更改加入回归测试；
- 避免在同一拉取请求中进行无关的重构。

## 模型与提供商元数据

提供商/模型的成员关系是自动生成的。不要在 Rust 中手动修改模型列表作为产品修复。

当模型目录行为更改时，更新生成器的输入或覆盖文件并重新生成目录工件。主要生成器位于：

```text
codex-rs/scripts/write_provider_catalog.py
codex-rs/scripts/write_model_compatibility_catalog.py
```

`write_provider_catalog.py --provider <id>` 只会刷新已有生成工件中指定的提供商。重复该选项可在不需要获取不相关实时提供商源凭证的情况下刷新相关集合。

当面向用户的设置发生变化时，请更新 [模型](/docs/models)、[提供商](/docs/providers) 或 [配置参考](/docs/config-reference)。

## 安全

不要在公开的议题线程中报告漏洞。请使用 Open Interpreter 项目或仓库列出的安全联系方式。
