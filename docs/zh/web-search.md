---
title: 网络搜索
description: 控制缓存、实时或禁用的网络搜索行为。
---

网络搜索允许在活动提供者和策略允许的情况下，让代理查询外部信息。

## 模式

```toml
web_search = "cached"   # default-style search behavior
web_search = "live"     # request live browsing
web_search = "disabled" # no web search tool
```

使用一次性实时搜索：

```bash
interpreter --search "check the current release notes and summarize changes"
```

## 网络沙盒是独立的

`web_search` 控制模型/工具的搜索范围。它与 shell 命令的网络访问是分离的。即使是受沙箱限制的 `curl` 或包管理器，也仍需通过沙箱或 permission‑profile 设置获得网络权限。

## 何时禁用

在敏感仓库、离线环境，或需要所有答案仅来自本地文件和已配置工具的任务中，禁用网络搜索。
