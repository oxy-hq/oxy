<!-- markdownlint-disable MD041 -->
<!-- Four sections, one of them usually deleted. A reviewer should read this in under a
     minute and know whether to look closely and where.

     Do NOT spend the body on what is already covered elsewhere: clippy, rustfmt, biome,
     `cargo nextest`, and style/lint nits are CI's and the review bots' job
     (.github/instructions/copilot-code-review.md, claude-review.yaml). Line-level
     discussion belongs in review comments.

     Delete these guidance comments from the final body. -->

## Why

<!-- The problem, in 1–3 sentences. What was broken, missing, or slow, and who felt it.
     Link the issue / thread / incident. If the reason is "a maintainer asked for it", say
     that — an honest one-liner beats an invented rationale. -->

## What changed

<!-- Only the major moves — the ones that change how the system behaves or is shaped:
     new or removed surface, a contract / schema / wire-format change, a swapped
     dependency, a rewritten code path, a changed default. 3–5 bullets, max.

     Skip renames, formatting, and follow-the-compiler churn; the diff already says that.

     Two things a monorepo reviewer cannot see in the diff — state them if they apply:
       - Blast radius: who else calls this. A shared crate (oxy-shared, oxy-core,
         agentic-core, entity) or a changed public signature reaches packages this PR
         does not touch — name them.
       - Not reversible by `git revert`: migrations, backfills, published artifacts
         (crates.io / npm / Docker / SDK), external state, anything a running old binary
         would read wrong mid-deploy.

     If this crosses CODEOWNERS teams, say in one line which reviewer should look at what. -->

-

## Libraries, tech & skills

<!-- DELETE this whole section if the PR only uses what the repo already uses.

     List anything a reviewer would otherwise have to look up, one line each:
       - A new or upgraded dependency (crate or npm): what it does, what it replaces, and
         why this one. For anything load-bearing, add maintenance / license / MSRV / bundle
         size in the same line. Note if it lands in the workspace `Cargo.toml` or root
         `package.json`, where it becomes everyone's dependency.
       - An unfamiliar technology, external API, or protocol — link the doc you worked from.
       - A pattern new to this repo (a runtime, a codegen step, a build hook, a new test
         kind). Say why the existing pattern did not fit.
       - The project skill or internal-doc that governs this change
         (`oxy-route-classification`, `oxy-compile-boundary`, `oxy-task-spec-default`,
         `oxy-scaling-design`, `oxy-customer-apps-perf`, `internal-docs/*`) — name it and
         confirm it was followed, or say where you deliberately diverged. -->

-

## Comprehension check

<!-- FOR THE ASSISTANT DRAFTING THIS PR BODY:

     This section quizzes BOTH sides. Writing it is how the author shows they understand
     what they are shipping; settling it from the code before revealing is how the reviewer
     shows they actually looked. Neither side gets to skim — that is why the answer is
     written down, and why it starts collapsed.

     Replace the five placeholders below with five NEW ones written for THIS diff. Do not
     reuse items from a previous PR. Each is a question the reviewer sees, plus the answer
     inside the <details> block. Keep the answer to 1–2 sentences, and keep it honest:
     "not covered, worst case is X" is a useful answer.

     Phrase each as a directed lookup — something settled by reading the code, not by
     reading this body. "Which fleet role serves the new /foo route, and what does it read
     off local disk?" is checkable; "Is the route classified correctly?" is a yes-box that
     ticks itself.

     A good item is one only someone who understands this change can settle, and whose
     wrong answer would be a real bug. Pick five, spread across these angles:
       - the load-bearing decision: why this approach and not the obvious alternative
       - blast radius: what else calls this, what breaks if it misbehaves
       - the edge case most likely to be wrong HERE (empty result, concurrency, partial
         failure, first-run / migration state, an auth or tenant boundary)
       - how a reviewer can tell it works: the test or command that fails without this diff
       - rollback, or the one thing you would watch after deploy

     Prefer the invariant this repo already knows it can break — route by what the diff touches:
       | Diff touches                                   | Ask about                                                        |
       | ---------------------------------------------- | ---------------------------------------------------------------- |
       | a route under `server/router/`                 | IdeOnly vs FleetOk — does it read node-local disk / `.git`?       |
       | any access or permission decision              | which `Ring` in `allows()`; a `role_guards` extractor, not a hand-rolled `matches!` |
       | a query or handler over tenant data            | scoping by `workspace_id` / project — what leaks if it is missing |
       | a new `.foo.yml`, or a workspace FS read        | compile boundary: a `*_definitions` row keyed by `revision_id`, not a per-request read |
       | background work, `tokio::spawn`, a periodic loop | a `TaskSpec` on the queue — what happens when the instance dies   |
       | an SSE stream                                  | the terminal `done`/`error`/`cancelled` on every failure path     |
       | `crates/agentic/**`                            | layering direction; no domain ↔ domain import                     |
       | `migration/` or `entity/`                      | additive-only, backfill, and the old binary still running mid-deploy |
       | `web-app/**`                                   | loading / error / empty states, effect races, which agentic flow covers it |
       | an LLM or pipeline path                        | Azure OpenAI routes via the OSS path; `openai_compat` needs an explicit `api_url` |

     Do NOT write anything answerable from the diff header ("which file changed?"),
     anything yes/no, or process items CI answers ("did tests pass?").

     MAINTAINERS: this is the review checklist. Each item should be settleable against the
     code in well under a minute. Answer it from the code, then open the author's answer.
     Tick when the two agree; leave it unticked and ask when they do not, or when the code
     would not tell you — that disagreement is the entire point of this section. -->

_Settle each from the code before revealing the answer. Tick when the two agree; leave it
unticked and ask when they do not._

- [ ] <!-- question 1 -->

  <details><summary>Reveal answer</summary>

  <!-- 1–2 sentences -->

  </details>

- [ ] <!-- question 2 -->

  <details><summary>Reveal answer</summary>

  <!-- 1–2 sentences -->

  </details>

- [ ] <!-- question 3 -->

  <details><summary>Reveal answer</summary>

  <!-- 1–2 sentences -->

  </details>

- [ ] <!-- question 4 -->

  <details><summary>Reveal answer</summary>

  <!-- 1–2 sentences -->

  </details>

- [ ] <!-- question 5 -->

  <details><summary>Reveal answer</summary>

  <!-- 1–2 sentences -->

  </details>

<!-- Required ONLY if this PR deletes a landing/homepage doc or touches >~50 files — delete otherwise. -->
- [ ] Homepage / positioning / tagline copy was carried over **verbatim**, not regenerated.
