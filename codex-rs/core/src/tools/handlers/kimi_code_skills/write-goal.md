---
name: write-goal
description: Help the user craft a well-specified `/goal` objective for goal mode — turn a rough intention into a completion contract with a clear finish line, proof, boundaries, and stop rule. Use when the user asks for help writing, refining, or improving a goal.
---

# Write a good goal (write-goal)

Help the user turn a rough intention into a `/goal` objective that goal mode can pursue across many turns without supervision. A goal is not a task description — it is a completion contract. It says what must become *true*, how that truth is *proven*, where the work may and may not *reach*, and when to *stop and report* instead of grinding on.

This skill is about authoring the objective text together with the user. Drafting and starting are separate steps: you settle the wording first, and only once the user has approved it do you start the goal by calling `CreateGoal`. The user still gets a final confirmation before it runs.

## Ask, don't narrate

**This is the most important rule in this skill. Every decision you put to the user goes through `AskUserQuestion`. No exceptions except one (below).**

Goal authoring is a chain of choices — what to scope, which phrasing, whether to add a budget and how large, which permission mode to start under. For every one of them: **stop and call `AskUserQuestion`.** Batch related choices in the same `AskUserQuestion` call when that helps the user decide. Do not write a paragraph that lists options and asks the user to reply in prose. Do not say "let me know if you'd prefer A or B." Do not bundle three questions into a wall of text. If you catch yourself typing out choices for the user to answer in free text, delete it and call `AskUserQuestion` instead.

A prose menu is a defect, not a style choice: it is slower for the user, easy to skim past, and usually gets a vague answer that forces another round. The only time you may ask in plain text is when `AskUserQuestion` is genuinely unavailable — auto mode, or a host that does not support it — and only then do you fall back to a short message with clearly labelled options and wait. Plain prose for *open-ended* input ("what would prove this is done?") is fine; the rule is about **choices between options**, which always use the tool.

## Rules of engagement

- **Only help when the user has asked for it.** Never volunteer to wrap an ordinary request in a goal, and never start one on your own. A normal "fix this test" is a normal request; treat it as a goal only when the user says they want a goal. If a task looks like it would suit goal mode, you may mention that once — but wait for the user to choose.
- **Write in the user's language.** Draft the objective in whatever language the user is writing to you in. If the project configuration or a saved memory names a preferred language, honor that instead. Keep the surrounding discussion in the same language.
- **Show before you start.** Always present the full drafted goal back to the user and get their agreement before anything runs. The user should read the exact text that will become the objective, not a paraphrase of it.
- **Draft with the user, not for them.** Goal-writing is a conversation. Offer a draft, explain the choices you made, invite changes, and fold the feedback in. Expect more than one round.
- **Respect the user's final call.** If, after you have pointed out what is vague or risky, the user still wants a looser or thinner goal, write the goal they asked for. Note the trade-off once; do not keep relitigating it or quietly "improve" the wording against their wishes.

## What makes a goal good

The strongest goals share one shape: they define **proof, not effort**. "Keep improving the code" describes effort and never ends. "Done when `npm test` exits 0 and no file outside `src/auth` changed" describes proof and is checkable. Aim for a contract with these parts:

1. **End state** — the condition that must become true. Name the finish line concretely: a passing suite, an empty queue, a search that returns zero matches, a deployed artifact.
2. **Proof** — the observable evidence that the end state holds. Prefer things the agent can run and you can inspect afterward: a command's exit code, a test count, a `grep`/`rg` with no hits, a file that now exists, a metric over a threshold.
3. **Boundaries** — what the work may and may not touch. Name the scope (which module, which directory) and the off-limits actions (do not edit the spec, do not change unrelated files, do not make destructive data changes).
4. **The loop** — when the work is iterative, say how to iterate: rerun the check after each change, work through the queue item by item, replay the failing cases until they pass.
5. **The stop rule** — how to end honestly when "done" is not reachable. A "stop and ask before widening scope" clause and an explicit blocked path ("if an external service is down, record it and move on") let the agent report instead of faking a pass or looping forever. This is about *honesty*, not a spending limit — keep it separate from any budget (see below).

Two habits make almost any goal better:

- **Make it queue-shaped.** Goals that shrink a list work best: failing tests, open issues, error traces, files to migrate, rows to process. A queue gives the agent a worklist and gives you a countable definition of done.
- **Lean on existing verification.** Tests, CI, type-checks, lint, eval suites, browser audits, and zero-match searches are leverage — they are what let a goal run unattended and still be trusted. If a task has no way to prove completion, help the user add one or reconsider whether goal mode fits.

Longer runs are not better runs. A tight contract that finishes in a handful of turns beats an open-ended one that burns hours re-running the whole suite after every edit.

## Budgets are opt-in

Goal mode can run under a turn or token budget, but **do not set one by default, and never bake a turn cap into the objective text.** A well-specified goal already stops on its own — when the proof passes or a blocker is hit — so an arbitrary cap usually does nothing except risk cutting off work midway.

When a budget is genuinely useful — typically an open-ended or exploratory goal that could run long unattended — you may suggest one, framed around the number users actually feel: token cost. Let the user choose the value, and sanity-check it against the work. A cap far larger than the task needs (say a thousand turns for a goal that will finish in a few) is not a safety net; it just invites wasted tokens. If the user asks for a value that looks oversized, say so and offer a smaller one, but respect their final call.

## Workflow

1. **Understand the intention.** Ask what outcome the user actually wants and what would prove it is done. If a finish line or a check is missing, that gap is the first thing to resolve together. As soon as the open questions reduce to concrete options, put them to the user with **AskUserQuestion** — do not list the options in prose.
2. **Draft the goal.** Write a concrete objective in the user's language, covering as many parts of the contract above as the task warrants. Keep it readable — one or a few sentences for simple work, a short structured block (end state, checks, boundaries, stop rule) for larger work.
3. **Show it and explain.** Present the draft in full and walk through the choices: what you picked as the finish line, what proves it, what you fenced off, when it stops. Point out anything still soft.
4. **Revise together.** Take the user's edits and produce a new draft. When you are weighing alternative phrasings or scopes, offer them as an **AskUserQuestion** choice instead of describing them. Repeat until they are satisfied. If they want it looser than you would recommend, say so once, then write their version.
5. **Start it.** Once the user approves the wording, start the goal by calling `CreateGoal` with the agreed objective (and a `completionCriterion` if you settled on one). Do not just print the text for the user to paste, and do not start before they have approved. Starting still surfaces a final confirmation, so the user keeps the last word on whether it runs.

## A reusable shape

For a non-trivial goal, this fill-in-the-blanks structure covers the contract:

```
<What must become true.>
Done when <command/search/state that proves it>.
Scope: only <files/area>; do not <off-limits action>.
Loop: <how to iterate — rerun the check after each change, etc.>.
If <blocking condition>, stop and report instead of forcing a pass.
```

Not every goal needs every line, and none of them is a turn cap — the goal stops when the proof passes or a blocker is hit. A small, well-scoped task can be a single clear sentence. Add structure as the work grows or the cost of a wrong autonomous run rises.

## Weak to strong

- Weak: `Find all bugs in this codebase.` — no finish line, no proof, no stop. The agent may block at once or run far past what you wanted.
  Strong: `Fix every test in test/auth that currently fails, rerun npm test until it exits 0, change no file outside test/ or src/auth, and report anything you cannot fix with its location and why.`
- Weak: `Optimize the project.` — no scope, no measure.
  Strong: `Migrate the payment module to the new API, make npm test -- payment exit 0, keep the diff limited to payment-related files, and stop and ask before touching shared infrastructure.`
- Weak: `Make it faster.`
  Strong: `Make renderFrame at least 3x faster measured by the bench/render benchmark; if you cannot reach 3x after several attempts, report the best result and why.`

## Common mistakes

| Mistake | Better |
| --- | --- |
| Starting or suggesting a goal the user did not ask for | Only draft a goal once the user asks; mention the option at most once otherwise |
| Drafting in English when the user is writing in another language | Match the user's language (or the project / memory preference) |
| Running the goal before the user has seen the exact text | Show the full draft and get agreement first |
| Polishing the goal silently against the user's stated wishes | Note the trade-off once, then write the goal they asked for |
| Burying a discrete choice in prose | Offer the options with AskUserQuestion (plain labelled options if it is unavailable) |
| Specifying effort ("keep improving X") | Specify proof ("done when check X passes") |
| Baking a turn cap into the objective or setting a budget unprompted | Let the goal stop on its proof; suggest a budget only when useful, framed on token cost |
| No blocked path | Add an explicit "stop and report" rule for blockers |
| A goal with no way to verify completion | Anchor it to tests, a search, a metric, or another inspectable check |
