---
title: GitHub 操作
description: 使用 interpreter exec 在 GitHub Actions 中运行 Open Interpreter。
---

当您希望 Open Interpreter 在 CI 中运行一次受限的自动化任务时，可在 GitHub Actions 中使用 `interpreter exec`。

## 基本工作流

```yaml
name: Open Interpreter Review

on:
  pull_request:

jobs:
  review:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: read
    steps:
      - uses: actions/checkout@v4

      - name: Install Open Interpreter
        run: curl -fsSL https://www.openinterpreter.com/install | sh

      - name: Review the patch
        env:
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
        run: |
          git diff origin/${{ github.base_ref }}...HEAD |
            interpreter exec --sandbox read-only \
              "Review this pull request diff for bugs, regressions, and missing tests."
```

## 提供者设置

使用您本地使用的相同提供者环境变量：

```yaml
env:
  OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
```

对于其他提供者，设置提供者的 API 密钥并传入配置覆盖：

```yaml
- name: Run with Kimi
  env:
    MOONSHOT_API_KEY: ${{ secrets.MOONSHOT_API_KEY }}
  run: |
    interpreter exec \
      -c 'model_provider="moonshotai"' \
      -c 'harness="kimi-code"' \
      -m kimi-k3 \
      "Summarize the risky parts of this change."
```

## 输出

对于机器可读的自动化，使用 JSON 事件：

```yaml
- name: Produce review events
  run: |
    interpreter exec --json \
      "List the files changed and the highest-risk issue." \
      > interpreter-events.jsonl
```

对于普通的最终答案，将最后的助手消息写入文件：

```yaml
- name: Write summary
  run: |
    interpreter exec \
      --output-last-message interpreter-summary.md \
      "Summarize the current diff for a release manager."
```

## 安全

除非工作流特意编辑文件，否则请使用 `--sandbox read-only` 启动 CI 作业。如果作业需要提交更改，请保持提示简洁，随后运行测试，并在合并结果前使用正常的 GitHub 评审保护措施。
