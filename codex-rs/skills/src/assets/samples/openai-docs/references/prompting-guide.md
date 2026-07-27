GPT-5.5 works best when prompts define the outcome and leave room for the model to choose an efficient solution path. Compared with earlier models, you can often use shorter, more outcome-oriented prompts: describe what good looks like, what constraints matter, what evidence is available, and what the final answer should contain.

Avoid carrying over every instruction from an older prompt stack. Legacy prompts often over-specify the process because earlier models needed more help staying on track. With GPT-5.5, that can add noise, narrow the model's search space, or lead to overly mechanical answers.

For more detail on GPT-5.5 behavior changes, start with the [Using GPT-5.5 guide](/api/docs/guides/latest-model). This guide focuses on prompt changes that follow from those behavior changes.

The patterns here are starting points. Adapt them to your product surface, tools, evals, and user experience goals.

## Personality and behavior

GPT-5.5's default style is efficient, direct, and task-oriented. This is useful for production systems: responses stay focused, behavior is easier to steer, and the model avoids unnecessary conversational padding.

GPT-5.6 works best when prompts define the outcome, important constraints, available evidence, and completion bar, then leave room for the model to choose an efficient path.

Removing repeated instructions and examples and simplifying tool descriptions can improve task performance and token efficiency. In a sample of internal coding-agent eval runs, configurations with leaner system prompts improved evaluation scores by roughly 10–15% while reducing total tokens by 41–66% and cost by 33–67%. Results will vary by workload, so treat these ranges as directional and validate changes on representative tasks from your own application.

Example personality block for a steady task-focused assistant:

Start with a prompt and tool set that already works. Remove one group of instructions, examples, or tools at a time, then rerun the same evals.

Prefer making progress over stopping for clarification when the request is already clear enough to attempt. Use context and reasonable assumptions to move forward. Ask for clarification only when the missing information would materially change the answer or create meaningful risk, and keep any question narrow.

- repeated statements of the same rule;
- repeated style or process instructions that do not change behavior;
- examples that do not change behavior;
- process instructions for behavior the model already performs reliably;
- tools and tool descriptions unrelated to the task.

Match the user's tone within professional bounds. Avoid emojis and profanity by default, unless the user explicitly asks for that style or has clearly established it as appropriate for the conversation.
```

- the user-visible outcome;
- success criteria and stopping conditions;
- safety, business, evidence, and permission constraints;
- tool-routing rules when the route depends on context;
- required output shape and validation requirements.

```text
# Personality
Adopt a vivid conversational presence: intelligent, curious, playful when appropriate, and attentive to the user's thinking. Ask good questions when the problem is blurry, then become decisive once there is enough context.

Be warm, collaborative, and polished. Conversation should feel easy and alive, but not chatty for its own sake. Offer a real point of view rather than merely mirroring the user, while staying responsive to their goals and constraints.

Be thoughtful and grounded when the task calls for synthesis or advice. State a clear recommendation when you have enough context, explain important tradeoffs, and name uncertainty without becoming evasive.
```

For more expressive products, add warmth, curiosity, humor, or point of view explicitly, but keep the block short. Use personality to shape the experience, not to compensate for unclear goals or missing task instructions.

## Improve time to first visible token with a preamble

In streaming applications, users notice how long it takes before the first visible response appears. GPT-5.5 may spend time reasoning, planning, or preparing tool calls before emitting visible text.

For longer or tool-heavy tasks, prompt the model to start with a short preamble: a brief visible update that acknowledges the request and states the first step. This can improve perceived responsiveness without changing the underlying task.

Use this pattern when the task may take more than one step, require tool calls, or involve a long-running agent workflow.

```text
Before any tool calls for a multi-step task, send a short user-visible update that acknowledges the request and states the first step. Keep it to one or two sentences.
```

For coding agents that expose separate message phases, you can be more explicit:

```text
You must always start with an intermediary update before any content in the analysis channel if the task will require calling tools. The user update should acknowledge the request and explain your first step.
```

## Outcome-first prompts and stopping conditions

GPT-5.5 is strongest when the prompt defines the target outcome, success criteria, constraints, and available context, then lets the model choose the path.

For many tasks, describe the destination rather than every step. This gives the model room to choose the right search, tool, or reasoning strategy for the task.

Prefer this:

```text
Resolve the customer's issue end to end.

Success means:
- the eligibility decision is made from the available policy and account data
- any allowed action is completed before responding
- the final answer includes completed_actions, customer_message, and blockers
- if evidence is missing, ask for the smallest missing field
```

**Avoid unnecessary absolute rules.** Older prompts often use strict instructions like `ALWAYS`, `NEVER`, `must`, and `only` to control model behavior. Use those words for true invariants, such as safety rules, required output fields, or actions that should never happen. For judgment calls, such as when to search, ask for clarification, use a tool, or keep iterating, prefer decision rules instead.

Avoid this style of instruction unless every step is truly required:

```text
First inspect A, then inspect B, then compare every field, then think through
all possible exceptions, then decide which tool to call, then call the tool,
then explain the entire process to the user.
```

Add explicit stopping conditions:

```text
Resolve the user query in the fewest useful tool loops, but do not let loop minimization outrank correctness, accessible fallback evidence, calculations, or required citation tags for factual claims.

GPT-5.6 tends to be more concise by default than GPT-5.5. When migrating, check whether broad brevity instructions such as “Be concise” or “Keep it short” are still useful. They may be unnecessary for some tasks and can sometimes make responses too brief. Keep them when they reliably produce the output your application needs.

For more consistent control across requests, use `text.verbosity` to set the default level of detail, then use the prompt for task-specific requirements. Choose `low`, `medium`, or `high` as the default level of detail for a request. In the prompt, specify any task-specific length, structure, or required content. See [Set up `text.verbosity`](https://developers.openai.com/api/docs/guides/deployment-checklist#set-up-textverbosity) for an API example.

For customer-facing assistants and collaborative products, define both personality and collaboration style.

Define missing-evidence behavior:

```text
Use the minimum evidence sufficient to answer correctly, cite it precisely, then stop.
```

When a task calls for a shorter answer, identify the information the model must preserve and the detail it can omit. For example:

    Lead with the conclusion. Include the evidence needed to support it, any material
    caveat, and the next action. Omit secondary detail and repetition.

    Keep all required facts, decisions, caveats, and next steps. Trim introductions,
    repetition, generic reassurance, and optional background first.

This gives the model a clear priority order: preserve the content needed to complete the task, then remove lower-value detail.

Broad labels such as “friendly” or “empathetic” can be ambiguous. Describe the writing choices that define your product's tone, such as how directly to state the answer, when to acknowledge a problem, and whether reassurance or a sign-off is appropriate.

    State the answer directly. If the user reports a problem, acknowledge the
    specific issue before giving the next step. Use reassurance only when it is
    relevant. Omit generic praise and unnecessary sign-offs.

Use headers, bold text, bullets, and numbered lists sparingly. Reach for them when the user requests them, when the answer needs clear comparison or ranking, or when the information would be harder to scan as prose. Otherwise, favor short paragraphs and natural transitions.

Respect formatting preferences from the user. If they ask for a terse answer, minimal formatting, no bullets, no headers, or a specific structure, follow that preference unless there is a strong reason not to.
```

Add explicit audience and length guidance:

## Define autonomy and approval boundaries

GPT-5.6 can be proactive and persistent when carrying out multi-step tasks. Define what level of action each request authorizes so the model can continue safe, in-scope work without unnecessary pauses while stopping before external, destructive, costly, or scope-expanding actions.

A compact policy is usually sufficient:

    For requests to answer, explain, review, diagnose, or plan, inspect the
    relevant materials and report the result. Do not implement changes unless
    the request also asks for them.

    For requests to change, build, or fix, make the requested in-scope local
    changes and run relevant non-destructive validation without asking first.

    Require confirmation for external writes, destructive actions, purchases,
    or a material expansion of scope.

Name safe local actions explicitly, such as reading files, inspecting logs, editing in-scope code, and running tests. Keep the policy in one place and state each rule once. Repeating instructions such as “ask first,” “do not mutate,” or “wait for approval” can cause unnecessary approval requests for safe, expected actions.

For long-running work, define the current layer of work. Distinguish research, design, implementation, review, and external coordination so the model does not silently move from one layer to another.

## Tool routing

Expose only task-relevant tools. Tool descriptions should state what the tool does, when to use it, important return fields, and error behavior.

When correctness depends on prerequisite retrieval or lookup, say so:

    Before taking an action, resolve required discovery, retrieval, and
    validation steps. Do not skip a prerequisite because the intended final
    state seems obvious.

When several reads are independent, parallelize them. When one result determines the next action, keep the work sequential. After parallel retrieval, synthesize before acting.

If a tool returns empty, partial, or suspiciously narrow results, try one or two meaningful fallbacks before concluding that no result exists.

## Programmatic Tool Calling

Programmatic Tool Calling (PTC) works best for bounded workflows where code can process several tool results or large intermediate outputs and return a much smaller structured result.

Multiple, parallel, or dependent calls alone do not justify Programmatic Tool Calling.

Use it for:

- filtering, joining, sorting, ranking, deduplication, and aggregation;
- batching across many similar records;
- repeated deterministic validation;
- large structured results that can be reduced to a compact schema.

Prefer direct tool calls when:

- one call is sufficient;
- intermediate outputs are already small;
- each result may change the next decision;
- an action requires approval;
- the final answer must preserve citations or native artifacts;
- the workflow requires semantic judgment between calls.

Do not rely on generic instructions such as “use Programmatic Tool Calling efficiently.” State the bounded stage, eligible tools, output schema, retry limit, stop condition, and handoff back to direct model judgment.

    Use Programmatic Tool Calling only for the bounded record-reduction stage.
    Call only the documented read-only tools. Filter and deduplicate the
    intermediate results, then emit exactly the required compact schema with
    evidence fields. Retry transient failures at most twice. Use direct tool
    calls for approval, semantic judgment, citations, and final validation.

If both routes are needed, define one clear handoff and tell the model not to switch routes or repeat completed work.

The `program_output` item and final assistant `message` are separate outputs; make sure to test both. In theory, a program can return the correct records while the message omits a required field, citation, or caveat.

Compare direct and programmatic calling on the same representative tasks. Check whether the final response is correct, complete, and includes the required evidence. Then compare total tokens, latency, cost, calls, turns, and retries. Count lower resource use as an improvement only when the response still passes your existing evals.

## Grounding, citations, and retrieval budgets

For grounded answers, citation behavior should be part of the prompt. Define what needs support, what counts as enough evidence, and how the model should behave when evidence is missing. Absence of evidence shouldn't automatically become a factual "no." For more details and examples, see the [citation formatting guide](/api/docs/guides/citation-formatting).

### Add an explicit retrieval budget

Retrieval budgets are stopping rules for search. They tell the model when enough evidence is enough.

```text
For ordinary Q&A, start with one broad search using short, discriminative keywords. If the top results contain enough citable support for the core request, answer from those results instead of searching again.

Make another retrieval call only when:
- The top results do not answer the core question.
- A required fact, parameter, owner, date, ID, or source is missing.
- The user asked for exhaustive coverage, a comparison, or a comprehensive list.
- A specific document, URL, email, meeting, record, or code artifact must be read.
- The answer would otherwise contain an important unsupported factual claim.

Do not search again to improve phrasing, add examples, cite nonessential details, or support wording that can safely be made more generic.
```

## Creative drafting guardrails

For drafting tasks, tell the model which claims must come from sources and which parts may be creatively written. This is especially important for slides, launch copy, customer summaries, talk tracks, leadership blurbs, and narrative framing.

```text
For creative or generative requests such as slides, leadership blurbs, outbound copy, summaries for sharing, talk tracks, or narrative framing, distinguish source-backed facts from creative wording.

- Use retrieved or provided facts for concrete product, customer, metric, roadmap, date, capability, and competitive claims, and cite those claims.
- Do not invent specific names, first-party data claims, metrics, roadmap status, customer outcomes, or product capabilities to make the draft sound stronger.
- If there is little or no citable support, write a useful generic draft with placeholders or clearly labeled assumptions rather than unsupported specifics.
```

## Frontend engineering and visual taste

For frontend work, refer to the [example instructions](/api/docs/guides/frontend-prompt) for practical ways to steer UI quality. They cover product and user context, design-system alignment, first-screen usability, familiar controls, expected states, responsive behavior, and common generated-UI defaults to avoid, such as generic heroes, nested cards, decorative gradients, visible instructional text, and broken layouts.

## Prompt the model to check its work

Give GPT-5.5 access to tools that let it check outputs when validation is possible.

For coding agents, ask for concrete validation commands:

Establish a baseline with the current reasoning effort before changing it.

If validation cannot be run, explain why and describe the next best check.
```

For visual artifacts, ask for inspection after rendering:

```text
Render the artifact before finalizing. Inspect the rendered output for layout, clipping, spacing, missing content, and visual consistency. Revise until the rendered output matches the requirements.
```

For engineering and planning tasks, make implementation plans traceable:

```text
For implementation plans, include:
- requirements and where each is addressed
- named resources, files, APIs, or systems involved
- state transitions or data flow where relevant
- validation commands or checks
- failure behavior
- privacy and security considerations
- open questions that materially affect implementation
```

## Phase parameter

Starting with GPT-5.4, long-running or tool-heavy Responses workflows can use assistant-item `phase` values to distinguish intermediate updates from final answers. GPT-5.5 uses the same pattern.

If you use `previous_response_id`, the API preserves prior assistant state automatically. If your application manually replays assistant output items into the next request, preserve each original `phase` value and pass it back unchanged. This matters most when a response includes preambles, repeated tool calls, or a final answer after intermediate assistant updates.

```text
If manually replaying assistant items:
- Preserve assistant `phase` values exactly.
- Use `phase: "commentary"` for intermediate user-visible updates.
- Use `phase: "final_answer"` for the completed answer.
- Do not add `phase` to user messages.
```

## Suggested prompt structure

Use this structure as a starting point for complex prompts. Keep each section short. Add detail only where it changes behavior.

```text
Role: [1-2 sentences defining the model's function, context, and job]

# Personality
[tone, demeanor, and collaboration style]

# Goal
[user-visible outcome]

# Success criteria
[what must be true before the final answer]

# Constraints
[policy, safety, business, evidence, and side-effect limits]

# Output
[sections, length, and tone]

# Stop rules
[when to retry, fallback, abstain, ask, or stop]
```
