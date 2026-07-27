---
title: AGENTS.md
description: 持久的项目说明，Open Interpreter 会自动读取。
---

`AGENTS.md` 是项目说明文件。把稳定的指导放在这里，而不是在每个提示中重复。

用于：

- 构建、测试、lint 和格式化命令
- 项目架构说明
- 代码风格和 API 约定
- 需要注意的文件或目录
- 发布、迁移或审查的期望

## 创建一个

在 TUI 中：

```text
/init
```

Open Interpreter 会检查仓库并起草一个初始的 `AGENTS.md`。把它编辑为应在会话间长期保留的规则。

## 范围与优先级

Open Interpreter 会加载：

| 作用域 | 路径 |
| ------ | ---- |
| 全局 | `~/.openinterpreter/AGENTS.md` |
| 项目 | 从仓库根目录到当前工作目录的 `AGENTS.md` 文件 |

更具体的文件会覆盖或补充更广泛的文件。更靠近当前目录的文件通常比接近根目录的文件更相关。

## 临时覆盖

创建此文件以在本地测试时替换全局说明：

```text
~/.openinterpreter/AGENTS.override.md
```

删除它即可恢复为正常的全局文件。

## 大小

合并后的项目说明受 `project_doc_max_bytes` 限制。目录特定的文件优先，以便在达到限制时附近的指导能保留。

## 示例

```markdown
# Project Instructions

## Commands
- `pnpm test` runs unit tests.
- `pnpm lint` must pass before final changes.
- Use `pnpm typecheck` after editing TypeScript types.

## Conventions
- Keep server code under `src/server`.
- Keep UI components small and colocated with their tests.
- Prefer existing helpers in `src/lib`.

## Cautions
- Do not edit generated files under `src/generated`.
- Ask before changing database migrations.
```

好的 `AGENTS.md` 文件简短、具体且持久。把临时任务细节放在提示中，而不是放在项目说明中。
