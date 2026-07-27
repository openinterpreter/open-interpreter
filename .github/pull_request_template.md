## Before opening this pull request

Open Interpreter is built on [OpenAI Codex](https://github.com/openai/codex)
and deliberately keeps its changes to Codex small. We generally do not accept
changes to core Codex behavior in this repository.

Before submitting, ask whether the change would also make sense in Codex
without Open Interpreter. If it would, please contribute it to
[`openai/codex`](https://github.com/openai/codex) instead. Once accepted there,
it will reach Open Interpreter through an upstream sync.

Changes belong here when they are specifically about an Open Interpreter-owned
surface, such as its additional harnesses, provider/model support, branding,
Open Interpreter endpoints, ACP support, compatibility/import behavior,
standalone packaging and updates, or product documentation.

- [ ] I considered whether this change belongs in `openai/codex`.
- [ ] This change is specific to an Open Interpreter-owned surface, or I have
      linked the upstream Codex contribution below.

## Why this belongs in Open Interpreter

<!--
Required, especially if this changes code inherited from Codex.

Explain what makes the change Open Interpreter-specific and which
Open Interpreter-owned surface it supports. If the core change has been or
will be proposed upstream, link that issue or pull request instead.

Pull requests that change general Codex behavior without a clear
Open Interpreter-specific reason will usually be redirected upstream.
-->

## Summary

<!-- Describe the change and its user-visible effect. -->

## Related issue

<!-- Link the bug report, feature request, or upstream Codex contribution. -->

## Test plan

<!-- List the checks you ran and any platform-specific validation. -->
