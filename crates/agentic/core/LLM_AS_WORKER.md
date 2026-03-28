# The LLM as Worker

## The Problem with Pure LLM Systems

Most LLM pipelines look like this:

```
User question → LLM → Answer
```

It works until it doesn't. The LLM might:

- Reference a column that doesn't exist
- Return the right shape but the wrong numbers
- Interpret "last quarter" as the wrong date range
- Misidentify a metric definition

The system doesn't know any of this. The answer ships anyway.

---

## Why LLMs Can't Self-Correct

The intuitive fix is to ask the LLM to check its own work:

> "Review your SQL. Is it correct?"

Research shows this doesn't work.

> _"Without external feedback, self-correction degrades accuracy."_
> — Huang et al. (2023), "Large Language Models Cannot Self-Correct Reasoning Yet"
> _"Self-repair is bottlenecked by the model's ability to provide feedback on its own code; diverse fresh attempts often outperform iterative repair."_
> — Olausson et al. (2023), "Demystifying GPT Self-Repair for Code Generation"

The LLM anchors on its previous output. It looks for what should change, not whether the whole approach was wrong. It patches when it should rethink.

---

## The Core Insight

**The LLM is good at generation. It is bad at verification.**

These are different cognitive tasks. Separate them.

```
LLM           →  generates output
Validator     →  checks output against ground truth
Orchestrator  →  routes on pass/fail
```

The LLM writes SQL. A deterministic validator parses it with a static SQL parser and checks every table and column reference against an in-memory catalog. Either they exist — or they don't. No ambiguity. No second opinion needed.

---

## What "Worker" Means

A worker does a job. A worker does not decide what job to do next.

In this system:

| Role         | Responsibility            | How it decides |
| ------------ | ------------------------- | -------------- |
| Orchestrator | Tracks FSM state, routes  | Deterministic  |
| Validator    | Checks output correctness | Deterministic  |
| LLM          | Generates within a state  | Stochastic     |

The LLM is called with a scoped prompt inside a state. It produces output. The orchestrator validates. If validation passes, the FSM advances. If it fails, the orchestrator routes to recovery. The LLM is not consulted on the routing decision.

```
┌─────────────────────────────────┐
│           State: Solving        │
│                                 │
│  Prompt ──► LLM ──► SQL         │
│                      │          │
│                      ▼          │
│              Validator: does    │
│              this SQL parse?    │
│              do these tables    │
│              exist?             │
│                      │          │
│              pass ───┤          │
│              fail ───┘          │
└─────────────────────────────────┘
         │             │
         ▼             ▼
     Executing     Diagnosing
                  (route to retry)
```

---

## What Validators Check

Validators are pure Rust functions. They have no model. They check facts.

**After Specifying — does the plan make logical sense?**

- Is every referenced metric registered in the catalog?
- Does every join key exist on both referenced tables?
- Do the filter columns resolve to real columns?

**After Solving — is the SQL structurally sound?**

- Does it parse without syntax errors? (`sql_syntax`)
- Are all FROM/JOIN tables in the catalog? (`tables_exist_in_catalog`)
- Does the SQL include every table the spec planned to use? (`spec_tables_present`)
- Do all qualified `table.column` references exist in the catalog? (`column_refs_valid`)

The third rule is particularly useful: it catches the case where the LLM silently dropped a required join or table from its query.

**After Executing — are the results plausible?**

- Is the result set non-empty?
- Does the shape (rows/columns) match what the spec said it would produce?
- Are there `NaN` or `Inf` values in numeric columns?
- Are numeric values within expected statistical bounds (z-score check)?

None of these require an LLM. None of them are fuzzy. They pass or they fail.

---

## What Happens on Failure

The validator returns a typed error. The orchestrator passes it to `diagnose`. `diagnose` is also deterministic — it's a routing table.

```rust
// Not LLM. Just a match.
// Ok(state) → recover to that state. Err(error) → fatal, escalate.
fn diagnose_impl(
    error: AnalyticsError,
    back: BackTarget<AnalyticsDomain>,
) -> Result<ProblemState<AnalyticsDomain>, AnalyticsError> {
    match &error {
        SyntaxError => match back {
            BackTarget::Solve(spec, _)                       => Ok(ProblemState::Solving(spec)),
            BackTarget::Clarify(i, _) | BackTarget::Specify(i, _) => Ok(ProblemState::Specifying(i)),
            _                                                => Err(error), // fatal
        },
        ShapeMismatch => match back {
            BackTarget::Solve(spec, _)                       => Ok(ProblemState::Solving(spec)),
            BackTarget::Clarify(i, _) | BackTarget::Specify(i, _) => Ok(ProblemState::Specifying(i)),
            _                                                => Err(error), // fatal
        },
        EmptyResults => match intent_from_back(back) {
            Some(intent)                                     => Ok(ProblemState::Specifying(intent)),
            None                                             => Err(error), // fatal
        },
        ...
    }
}
```

Note: `EmptyResults` from `Executing` uses `intent_from_back`, which returns `None` for a `BackTarget::Execute` (no intent available) — that path is fatal. In practice the executing handler converts the back-target to `BackTarget::Specify(intent, ...)` _before_ emitting `Diagnosing`, so `diagnose_impl` receives a `BackTarget::Specify` and recovers cleanly. The handler does the path-aware conversion; `diagnose` just routes on what it receives.

The retry carries a `RetryContext` — errors observed, attempt count, optionally the previous output on the second failure. On the first retry the previous output is deliberately withheld, forcing the LLM to generate fresh rather than patch a broken answer.

---

## What the LLM Sees on Retry

A fresh prompt, scoped to the current state, with:

- The original spec (what it was asked to produce)
- The errors from the validator (what was wrong)
- On second failure only: the text of the previous failed attempt

It does NOT see:

- The prior state's LLM thinking
- The orchestrator's routing decision
- Artifacts from other states

Each LLM call is stateless. Fresh context. This is intentional — accumulated artifacts from failed states contaminate the generation.

---

## The Analogy: Compiler + Developer

The LLM is a developer writing code. The validator is the compiler.

| System         | Developer     | Compiler               | Build log         |
| -------------- | ------------- | ---------------------- | ----------------- |
| Software build | writes code   | checks correctness     | structured errors |
| This FSM       | LLM generates | validator checks facts | `RetryContext`    |

A developer doesn't ask the compiler "do you think this is good code?" The compiler just runs. Pass or fail. If it fails, the developer reads the error and tries again.

We don't ask the LLM "is your SQL correct?" We run the SQL against the schema. Pass or fail.

---

## The Result

| Property                      | Pure LLM | LLM + Deterministic Validation |
| ----------------------------- | -------- | ------------------------------ |
| Catches hallucinated columns  | ✗        | ✓ (schema check)               |
| Catches empty query results   | ✗        | ✓ (post-execution check)       |
| Catches wrong result shape    | ✗        | ✓ (shape match check)          |
| Catches statistical anomalies | ✗        | ✓ (z-score check)              |
| Self-corrects reliably        | ✗        | ✓ (routed retry with error)    |
| Deterministic on same input   | ✗        | ✓ (routing is deterministic)   |

The LLM's job is to be creative and generative. The validator's job is to be boring and correct. Neither should try to do the other's job.
