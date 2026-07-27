---
title: 提示
description: 编写能够为代理提供足够上下文以安全行动的提示。
---

好的 Open Interpreter 提示应具体。包括观察到的问题、期望的行为、约束条件以及验证命令。

## 缺陷修复模板

```text
Bug: Clicking Save shows success but does not persist the setting.
Repro:
1. npm run dev
2. Open /settings
3. Toggle Enable alerts
4. Click Save
5. Refresh; the toggle resets

Constraints:
- Do not change the API shape.
- Keep the patch minimal.
- Add a regression test if practical.

Start by reproducing, then patch, then rerun the repro and tests.
```

## 优于模糊请求

与其只写“修复身份验证”，不如写“运行 `pnpm test -- auth` 并修复失败的 refresh-token 测试”。与其假设代理知道约束，不如明确写“不要修改数据库迁移”。

## 使用文件

在 TUI 中使用 `@` 提及文件，或在命令行中附加相关文件/图片。保持上下文聚焦；过多无关的上下文会使任务变得更困难。
