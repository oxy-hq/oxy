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

     ALTITUDE — read this before writing anything. Every item names a DECISION, not a
     detail. Much of the code here is drafted by an assistant, which means most choices in
     a diff were never argued by anyone: a default was picked, an unhappy path was given
     semantics, an abstraction was placed in a crate, a pattern was invented next to an
     existing one. Those choices ship silently and become the system. This section exists so
     no design decision reaches main without one human who can state it and one who checked
     it. If the answer is "the assistant chose it and nobody weighed in", write exactly that
     — an unowned decision is the most useful thing this section can surface.

     Replace the five placeholders below with five NEW ones written for THIS diff. Do not
     reuse items from a previous PR. Each is a question the reviewer sees, plus the answer
     inside the <details> block. Keep the answer to 1–2 sentences and keep it honest:
     "not handled — worst case is X" is a good answer.

     Pick five, one per angle where the diff has one:
       - THE FORK: the alternative that was rejected, and what would have to be true for it
         to have been the better call
       - INVENTED BEHAVIOUR: what this decided that nobody specified — a default, a limit,
         retry or timeout semantics, what the unhappy path now does, the name of a new
         contract
       - SHAPE: the abstraction introduced or extended (module, trait, table, state, event)
         and why it lives where it does rather than one layer up or down
       - BAKED-IN ASSUMPTION: what this now takes for granted — ordering, idempotency,
         single-writer, tenancy, freshness, scale — and what breaks the day it stops holding
       - DELIBERATELY NOT DONE: the case left unhandled, the follow-up, the existing repo
         pattern this departs from and why

     Where the diff lands tells you which decision is load-bearing — use this to find it,
     not as a compliance checklist:
       | Diff touches                                     | The decision worth surfacing                                       |
       | ------------------------------------------------ | ------------------------------------------------------------------ |
       | a route under `server/router/`                   | which fleet role this pins the feature to, and whether reading local disk was chosen or inherited |
       | any access or permission decision                | which authority ring this lands in — who can newly do this, and who quietly cannot |
       | a query or handler over tenant data              | what the scoping key is, and what one missing filter would expose   |
       | a new `.foo.yml`, or a workspace FS read         | whether the artifact became compiled state or a per-request read, and what that costs later |
       | background work, `tokio::spawn`, a periodic loop | durable or fire-and-forget, and what is lost when the instance dies mid-flight |
       | an SSE stream                                    | what the client is left believing on each failure path              |
       | `crates/agentic/**`                              | which layer owns the new logic, and what placing it there forecloses |
       | `migration/` or `entity/`                        | the shape of the data change and the deploy order it now requires   |
       | `web-app/**`                                     | the state model chosen, and what the user sees while it is loading, empty, or wrong |
       | an LLM or pipeline path                          | which vendor path this takes and what changes for configs already in the wild |

     Do NOT write items about naming, formatting, a lint, a one-line guard, or an edge case
     in isolation — those are review comments, and the bots already have them. An edge case
     earns a slot only when the answer reveals a decision. Nothing yes/no, nothing answerable
     from the diff header, nothing CI answers.

     MAINTAINERS: this is the review checklist, and it is aimed at you as much as the author.
     Settle each from the code, then reveal. Tick when the two agree; leave it unticked and
     ask when they do not, or when the code alone would not have told you — a decision only
     one side can state is the finding. -->

_Each item is a decision this change makes. Settle it from the code before revealing the
answer; tick when the two agree, leave it unticked and ask when they do not._

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
