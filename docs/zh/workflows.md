---
title: 工作流
description: 在实际开发任务中使用 Open Interpreter 的可重复方法。
---

## 修复 Bug

1. 从仓库根目录开始。
2. 提供复现步骤和限制条件。
3. 在编辑之前，让 Open Interpreter 先复现问题。
4. 审核补丁。
5. 让它重新运行复现步骤并进行项目检查。

## 审核 Diff

```bash
interpreter exec review --uncommitted
```

或在 TUI 中：

```text
/review
```

审查输出应优先关注 Bug、回归、缺失的测试以及风险行为。

## 安全重构

先请求一个计划：

```text
/plan
Split the oversized parser module without changing public behavior.
```

然后分阶段执行，并在阶段之间运行测试。

## 保持文档更新

将 Open Interpreter 指向已修改的文件并让它更新面向用户的文档。保持私有工作区的细节不出现在产品文档中。
