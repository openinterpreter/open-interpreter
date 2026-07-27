---
title: GitHub Action
description: Run Open Interpreter from GitHub Actions with interpreter exec.
---

Use `interpreter exec` in GitHub Actions when you want Open Interpreter to run
one bounded automation task in CI.

## Basic Workflow

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

## Provider Setup

Use the same provider environment variables you use locally:

```yaml
env:
  OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
```

For another provider, set the provider's API key and pass config overrides:

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

## Outputs

For machine-readable automation, use JSON events:

```yaml
- name: Produce review events
  run: |
    interpreter exec --json \
      "List the files changed and the highest-risk issue." \
      > interpreter-events.jsonl
```

For a plain final answer, write the last assistant message to a file:

```yaml
- name: Write summary
  run: |
    interpreter exec \
      --output-last-message interpreter-summary.md \
      "Summarize the current diff for a release manager."
```

## Safety

Start CI jobs with `--sandbox read-only` unless the workflow intentionally
edits files. If a job should commit changes, keep the prompt narrow, run tests
afterward, and use normal GitHub review protections before merging the result.
