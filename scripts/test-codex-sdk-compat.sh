#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
interpreter_bin="${INTERPRETER_BIN:-}"
pnpm_command=(pnpm)

if command -v corepack >/dev/null 2>&1; then
  pnpm_command=(corepack pnpm)
fi

if [[ -z "${interpreter_bin}" ]]; then
  interpreter_bin="$(command -v interpreter || true)"
fi

if [[ -z "${interpreter_bin}" || ! -x "${interpreter_bin}" ]]; then
  echo "Set INTERPRETER_BIN to an executable Open Interpreter binary." >&2
  exit 1
fi

cd "${repo_root}"

if [[ ! -d node_modules || ! -d sdk/typescript/node_modules ]]; then
  "${pnpm_command[@]}" install --frozen-lockfile --filter @openai/codex-sdk...
fi

"${pnpm_command[@]}" --filter @openai/codex-sdk run build

CODEX_EXEC_PATH="${interpreter_bin}" "${pnpm_command[@]}" \
  --dir sdk/typescript \
  exec jest \
  --runInBand \
  tests/run.test.ts \
  --testNamePattern "resumes thread by id"

echo "Codex SDK compatibility passed with ${interpreter_bin}"
