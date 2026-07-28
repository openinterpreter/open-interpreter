# Security Policy

Thank you for helping us keep Open Interpreter secure.

## Reporting Security Issues

Please do not open a public issue for a suspected vulnerability.

For a vulnerability in an Open Interpreter-owned surface—such as harness
emulation, provider compatibility, Open Interpreter packaging and updates,
branding endpoints, ACP support, or compatibility/import behavior—email
[killian@openinterpreter.com](mailto:killian@openinterpreter.com) with the
subject `Open Interpreter security report`.

Open Interpreter is built on OpenAI Codex. If the vulnerability also exists in
unmodified Codex, follow the
[OpenAI vulnerability disclosure program](https://bugcrowd.com/engagements/openai)
instead. This lets the upstream security team fix the source and allows Open
Interpreter to receive the correction through an upstream release.

When uncertain, report privately through the Open Interpreter address above. We
will route inherited issues upstream without publishing sensitive details.

## Operating the inherited Codex runtime safely

For details on Codex security boundaries, including sandboxing, approvals, and network controls, see [Agent approvals & security](https://developers.openai.com/codex/agent-approvals-security).
