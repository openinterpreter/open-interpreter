---
title: Open Interpreter 维护的 Codex 差异
description: 跟踪上游 Codex 时保留的明确产品与兼容性差异。
---

# Open Interpreter 维护的 Codex 差异

Open Interpreter 跟踪上游 Codex CLI 的稳定版本。我们保留一组精简且明确的
产品差异，确保审核上游更新时不会意外丢失兼容性行为。

维护的差异包括：

- Open Interpreter 的产品标识、打包、安装程序、更新和发布工作流。
- 与模型服务商无关的模型配置和托管服务商目录。
- 一等支持兼容 OpenAI 的 Chat Completions 传输；单次调用可使用
  `--chat-completions`，服务商配置可使用 `wire_api = "chat"`。
- 为提供该 API 的服务商支持兼容 Anthropic Messages 的传输。
- 保持服务商中立的原生服务商 harness 和 agent 界面。
- Open Interpreter 桌面端和 app-server 集成点。

当互操作性需要时，内部会保留与 Codex 兼容的 crate、协议、配置和环境变量
名称。除非明确说明上游兼容性，面向用户的帮助、错误、补全、安装程序和更新
界面必须使用 Open Interpreter 的产品标识。

每次同步上游稳定版本时，都必须审核此列表、测试受影响的每一项，并在发布
拉取请求中记录所有新增或删除的差异。
