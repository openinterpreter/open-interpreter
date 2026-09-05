# Open Interpreter's maintained Codex delta

Open Interpreter tracks stable releases of the upstream Codex CLI. We keep a
small, explicit product delta so upstream updates can be reviewed without
accidentally dropping compatibility behavior.

The maintained delta is:

- Open Interpreter product identity, packaging, installers, updates, and
  release workflow.
- Provider-neutral model configuration and the hosted-provider catalog.
- First-class OpenAI-compatible Chat Completions transport, selectable with
  `--chat-completions` for one invocation or `wire_api = "chat"` in a provider
  configuration.
- Anthropic Messages-compatible transport for providers that expose that API.
- Native provider harnesses and agent surfaces that remain provider-neutral.
- Open Interpreter desktop and app-server integration points.

Internal Codex-compatible crate, protocol, configuration, and environment
names are retained when interoperability requires them. User-facing help,
errors, completions, installers, and update surfaces must use Open Interpreter
identity unless they are explicitly describing upstream compatibility.

Every stable upstream reconciliation must review this list, test each affected
bucket, and record any addition or removal in the release pull request.
