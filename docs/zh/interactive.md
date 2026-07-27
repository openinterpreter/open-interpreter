---
title: 交互模式
description: 在终端 UI 中使用提示、文件、图片、批准和斜杠命令进行工作。
---

在项目目录中运行 `interpreter` 以启动终端 UI：

```bash
cd my-project
interpreter
```

你也可以在命令行中直接传入首个提示：

```bash
interpreter "find the auth middleware and explain how it works"
```

## 输入框

Composer 是位于 TUI 底部的提示框。

| 操作 | 键或命令 |
| ---- | -------- |
| 发送消息 | `Enter` |
| 添加换行 | `Shift+Enter` |
| 打开斜杠命令 | `/` |
| 提及文件 | `@` 或 `/mention` |
| 在 `$VISUAL` 或 `$EDITOR` 中编辑提示 | `Ctrl+G` |
| 搜索提示历史 | `Ctrl+R` |
| 在工作运行时排队后续操作 | `Tab` |
| 取消或退出 | `Esc` |
| 退出 | `/exit` 或按两次 `Ctrl+C` |

## 文件和图片

使用 `@` 进行模糊搜索文件并将其添加为上下文。你也可以将图片附加到首个提示中：

```bash
interpreter -i screenshot.png "explain what is wrong in this UI"
interpreter -i before.png,after.png "compare these states"
```

## 批准

当命令或工具需要批准时，TUI 会在执行前显示请求。默认姿态旨在面向可信仓库的日常工作：允许工作区访问，超出活动策略的操作会先询问。

使用以下命令更改活动策略：

```text
/permissions
```

有关详细信息，请参阅 [沙箱与批准](/docs/sandbox) 和 [权限](/docs/permissions)。

## 模型和提供商

使用 `/model` 选择提供商、模型和推理力度。Open Interpreter 支持 OpenAI、Anthropic、本地提供商，以及来自生成模型目录的兼容自定义提供商。

常见的一次性覆盖：

```bash
interpreter -m gpt-5.1-codex "review this module"
interpreter --oss "try this with my local model"
```

## 评审与规划

当你希望代理在编辑前进行检查并提出建议时，使用 `/plan`。当你想对当前更改进行代码审查时，使用 `/review`。

```text
/plan
/review
```

评审模式侧重阅读。它会在摘要之前报告错误、回归、缺失的测试以及风险行为。

## 后台工作

长时间运行的命令可以在后台终端中保持活跃，而代理继续工作。

| 命令 | 用途 |
| ---- | ---- |
| `/ps` | 列出后台终端 |
| `/stop` | 停止后台终端 |

## 会话控制

| 命令 | 用途 |
| ---- | ---- |
| `/new` | 开始一个新的会话 |
| `/resume` | 选择一个旧的会话 |
| `/fork` | 分叉当前会话 |
| `/compact` | 压缩旧的上下文 |
| `/clear` | 清除屏幕 |
| `/copy` | 复制最新的助手输出 |
| `/theme` | 更改语法高亮主题 |
| `/status` | 检查模型、沙盒、批准和令牌状态 |

Open Interpreter 将会话状态本地保存在 `~/.openinterpreter/` 下。
