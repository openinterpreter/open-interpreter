---
name: update-config
description: Inspect or edit kimi-code's own config — `config.toml` (model, provider, permission, hooks) and `tui.toml` (theme, editor, notifications, auto-update). Use when the user asks what a setting does or wants to change one.
---

# Configure kimi-code (update-config)

Help the user inspect, change, and validate kimi-code's configuration files. The files are **TOML** with **snake_case** keys.

## The two config files

kimi-code has two TOML config files, both under `<KIMI_CODE_HOME>/`, both snake_case, but with different ownership — decide which one the user means before doing anything.

The runtime resolves the data directory as `KIMI_CODE_HOME` first, falling back to `~/.kimi-code`. Before doing anything, resolve the actual directory with Bash so you don't write to the wrong place. Check whether `KIMI_CODE_HOME` is set and fall back to `~/.kimi-code` when it is empty:

```bash
echo "$KIMI_CODE_HOME"
echo "$HOME/.kimi-code"
```

Use the first line when it is non-empty; otherwise use the second line. In the rest of this skill, `<KIMI_CODE_HOME>` means that resolved root — **never assume `~/.kimi-code`**.

- **`config.toml`** — agent / runtime settings: `default_model`, `providers`, `models`, `thinking`, `permission`, `hooks`, `loop_control`, etc.
- **`tui.toml`** — terminal-UI / client preferences: `theme`, `[editor].command`, `[notifications]`, `[upgrade].auto_install` (auto-update). These can usually also be changed with the interactive commands `/config`, `/theme`, `/editor`, which is easier — prefer pointing the user at those.

The "read → copy → Edit → validate → back up → overwrite" flow below applies to both files; only **which reload command applies** differs (see Capability 4).

## Prerequisite 1: the official docs are the single source of truth

Before touching any config, use **FetchURL** to fetch the official config docs as the one authoritative reference for fields (key names, types, allowed values, owning section):

```
https://moonshotai.github.io/kimi-code/en/configuration/config-files.html
```

- Use the **snake_case key names and sections exactly as documented** — don't invent them, don't guess camelCase.
- If FetchURL is unavailable or the fetch fails, tell the user plainly that you can't reach the online docs, and ask them to paste the relevant section or confirm whether to proceed from what you already know. **Never edit blindly without an authoritative reference.**

## Prerequisite 2: read the target file before any change

Before any modification, use **Read** on the target config file (decide whether it's `config.toml` or `tui.toml` per the above):

- Location: `<KIMI_CODE_HOME>/config.toml` or `<KIMI_CODE_HOME>/tui.toml`. For other scopes/files, defer to the official docs.
- A missing or empty file is fine — you'll create a minimal skeleton later.
- If the file exists but **fails to parse as TOML**, report the error verbatim and **stop** — never overwrite a broken file in place (it could destroy the user's existing config).

---

## Capability 1: explain configuration (read-only, no file changes)

When the user asks "what config is there", "what does this setting do", or "how do I use it":

1. Fetch the official docs (Prerequisite 1).
2. Read the current `config.toml` (Prerequisite 2).
3. Answer against both: list the relevant sections / keys, what each is for, **current value vs default**, and the allowed-value range; say which file and section each lives in.
4. Present it as a compact grouped list or table. **Stay read-only — write no files.**

## Capability 2: make changes for the user (copy → Edit → validate → back up → overwrite)

Don't edit the target file in place, and **don't rewrite it from scratch** — instead copy it, Edit the copy, and keep the original out of any broken state the whole time:

1. **Clarify intent**: which key, what value, and which file (`config.toml` or `tui.toml`). Ask in one line if ambiguous; for discrete choices (e.g. scope) AskUserQuestion is fine, but use plain questions for free-form input.
2. **Read the target file** (Prerequisite 2): Read it to understand the current state and confirm it parses.
3. **Copy out a candidate (do not create from scratch)**: use **Bash** to copy the target verbatim — `cp config.toml config-new.toml` (same directory, `-new` suffix; for tui.toml, `cp tui.toml tui-new.toml`). **Leave the original untouched for now.**
   - Only when the target doesn't exist (nothing to copy) should you use **Write** to create a minimal skeleton candidate (e.g. just the comment line `# <KIMI_CODE_HOME>/config.toml`).
4. **Edit the candidate**: use the **Edit** tool on the candidate to **change/add only the target key** — never rewrite the whole file. That way every existing section, entry, comment, and bit of formatting stays exactly as-is; only what should change changes. The candidate is identical to the original, so use the content you read in step 2 to locate the Edit anchor. Check the change against the official docs (key / section / value type / allowed values, snake_case).
5. **Validate the candidate** (see Capability 3, via `kimi doctor`). **If anything fails, keep Editing the candidate and re-validate, looping until it all passes.**
6. **Back up and overwrite** (only after validation fully passes):
   - **Back up the old file — always create a new timestamped backup, keep all of them, never overwrite an existing backup.** Copy this exactly with **Bash** (for config.toml): `cp config.toml "config.toml.$(date +%Y%m%d-%H%M%S).bak"`; for tui.toml: `cp tui.toml "tui.toml.$(date +%Y%m%d-%H%M%S).bak"`. Skip the backup only if the target didn't exist.
   - Overwrite with the candidate: `mv config-new.toml config.toml`.
   - If reload errors after the overwrite, the user can recover from **the most recent timestamped backup**.
7. Tell the user how to apply it (see Capability 4).

## Capability 3: validate the candidate file (must pass before overwrite)

Use **`kimi doctor`** to validate the candidate you wrote — it doesn't start the TUI and doesn't modify any file; it runs kimi's own parser + schema (syntax and schema together), so it's the authoritative check. Pick the subcommand by which file you changed, and pass the **candidate** path explicitly:

- changed `config.toml` → `kimi doctor config <config-new.toml path>`
- changed `tui.toml` → `kimi doctor tui <tui-new.toml path>`

When a path is passed explicitly the file must exist (your candidate does, so that's fine). **Exit code 0 = pass (valid or skipped); non-zero = a specified file is missing or the config is invalid** — show the output verbatim, fix the candidate, and re-run, looping until it's 0.

Then do two checks `kimi doctor` can't:

1. **Cross-check values against the official docs** (single source of truth): are the key / section / enum values as documented, and snake_case? doctor guarantees "schema-valid", but "valid yet not what the user wanted" (e.g. a misspelled model alias) needs the docs.
2. **Completeness**: every existing entry is still present (the candidate fully replaces the target — a dropped line is a deletion).

> To also check whether the currently **active** config is OK overall, run `kimi doctor` with no path (it checks the default `config.toml` + `tui.toml`, showing a missing one as skipped).

## Capability 4: tell the user how to apply changes

Once local validation passes, tell the user how to make the change take effect — **the reload command depends on which file you changed**:

- changed **`config.toml`** → run **`/reload`** in the TUI (reloads the session and applies `config.toml`; it also reloads `tui.toml`).
- changed **`tui.toml`** → run **`/reload-tui`** (reloads only `tui.toml`, lighter); `/reload` works too (reloads both).
- changed both → a single **`/reload`** covers it.

Note: `/reload` is available **only when idle** — if a reply is streaming, press Esc / Ctrl-C to stop first. `kimi doctor` already validated the schema before the overwrite, so reload should apply cleanly; if it still errors, follow the message to fix it or recover from the most recent timestamped backup. If you don't want to reload now, the **next new session** picks it up automatically.

## Don'ts

- **Always back up before overwriting**, with a **timestamped name and all history kept** — don't skip the backup, don't keep only a single `.bak`, don't overwrite an old backup.
- Don't drop unrelated entries (the candidate fully replaces the target — a dropped line is a deletion).
- When you can't reach the docs / have no authoritative reference, don't edit by guessing.
