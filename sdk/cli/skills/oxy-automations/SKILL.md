---
name: oxy-automations
description: Use when writing, debugging, or reviewing an Oxy automation (a `.automation.yml` / legacy `.procedure.yml` workflow — tasks, loops, variables, schedules) in a customer workspace, or when an automation *run* misbehaves: an `http_request` task that works locally and fails in cloud, a file that is silently not an automation, or an automation run landing `completed_with_errors`. Covers the file-naming trap, task-type gotchas, and where the authoritative reference lives.
---

# Oxy automations

**The reference is upstream and maintained — read it rather than guessing:**
<https://www.oxygen-hq.com/docs/guide/build/automations>
(`index`, `loops`, `task-types`, `variables`).

This skill deliberately does **not** restate the schema. It carries the things
that reference does not tell you, and the mistakes that cost time.

## The file-naming trap

The canonical extension is **`.automation.yml`**.

- `.procedure.yml` is the **legacy** name. It is still accepted, so a workspace
  full of it keeps working and nothing warns you — which is exactly why it
  spreads. Poke House's workspace has 18 automations, and every one of them is
  still `.procedure.yml`.
- `.workflow.yml` is **no longer a recognised file kind at all**. A file with
  that name is silently not an automation.
- `oxy migrate-automations` exists to rename legacy files. It is the only thing
  that reads them for that purpose.

**Write new files as `.automation.yml`.** Do not copy the extension from a
neighbouring file in the workspace you are editing — that is how the legacy name
propagated in the first place.

## Naming, and why the code disagrees with the docs

"Automation" is the current name for what used to be called a Procedure, and
before that a Workflow. The Rust types still say `Workflow*` / `Procedure*`, and
`type: workflow` plus the `agentic_workflow_state` table are **wire and storage
contracts** — they are not stale naming to be tidied up. Seeing `workflow` in the
code or in a payload does not mean you are looking at something deprecated.

Routes follow the same split: `/automations/:id` is canonical;
`/workflows/:id`, `/procedures`, and `/agentic-workflows` are aliases.

## Task-type gotchas

- **`http_request` is HTTPS-only and blocks private egress.** Requests to
  localhost, cloud-metadata endpoints, and private IP ranges are refused unless
  that task opts in with `allow_hosts`. A task that works locally and fails in
  cloud is usually this, not a network problem.
- **Partial failures are their own state.** A run where some steps failed records
  as `completed_with_errors`, not as a failed run. Code and dashboards that only
  check for "failed" will report it as a success.

## Before you write one

Check whether the work belongs in an automation at all. Scheduled data movement
is usually an Airway pipeline (`.airway.yml`); a question answered from the
semantic layer is usually an agent or a Data App. Automations are for
multi-step orchestration you want to run on a schedule or trigger.

**But "usually a pipeline" is not "never an automation", and the seam is
undocumented.** There is a `type: airway` task that runs a pipeline as a step —
absent from the task-types page — and a step completes only once the pipeline's
end-of-load fold has committed. That is what makes the standard
ingest-then-aggregate automation safe: `type: airway` followed by
`type: execute_sql` building a rollup table. Reach for it whenever a dashboard
would otherwise aggregate at query time. **`oxy-data-architecture`** owns that
pattern and the guarantee behind it.
