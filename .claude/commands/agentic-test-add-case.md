---
name: agentic-test-add-case
description: Add a new case to an existing .flow.test.yml rather than authoring a new flow from scratch
activeForm: Adding case to existing agentic browser flow
argument-hint: "<flow-file> <description>"
allowed-tools:
  - Bash
  - Read
  - Edit
  - Grep
  - AskUserQuestion
---

# Add a case to an existing flow

Use when the dev wants a new test case that's a near-sibling of an
existing flow's cases — same `target:`, same `backend_mode:`, same
`setup:`, same surface — rather than authoring a whole new file. Sharing
settings + setup across cases is cheaper to maintain and keeps related
coverage in one place.

`$ARGUMENTS` is parsed as `<flow-file> <description>`. The flow file is
either a path (`web-app/tests/agentic/flows/chat-ask.flow.test.yml`) or a
stem (`chat-ask`).

This command must be run from the root of an `oxy-hq/oxygen-internal`
checkout.

## Steps

### 1. Resolve the flow file

```bash
# If the dev gave a stem, find the file.
if [ ! -f "$FLOW_PATH" ]; then
  FLOW_PATH=$(find web-app/tests/agentic/flows -name "${FLOW_STEM}.flow.test.yml" | head -1)
fi
```

If the file doesn't exist, tell the dev: "I can't find a flow at
`<path>`. Either give me the full path or use `/test-feature` to author
a new flow." Exit.

### 2. Re-read source of truth

Always re-read these — they evolve:

- `web-app/tests/agentic/README.md` — confirm the schema hasn't drifted.
- `json-schemas/flow-test.json`.
- `web-app/tests/agentic/canonical-prompts.md` — for `cache_scope: shared`
  step text the new case might want to reuse.
- The full contents of the target flow file (`Read`) so you preserve its
  `target:`, `backend_mode:`, `settings:`, and `setup:` and add a case
  that's shape-compatible with the existing ones.

### 3. Check the existing flow's contract

Before adding a case, verify the new case fits:

- **Same `target:`**? If not, the dev probably wants a new flow, not a new
  case here. Use `AskUserQuestion` to confirm.
- **Same `backend_mode:`**? Cases inside one flow share the backend boot.
  Mixing modes inside one flow doesn't work — local-mode and cloud-mode
  flows are spawned differently by the runner.
- **Same setup state**? If the new case needs different setup (a clean
  `test.sql` vs a dirty one, a different starting URL, a different demo
  file to revert via `restore_demo_file:`), it can't share this flow's
  `setup:` block. Tell the dev and route them to `/test-feature` instead.
- **No name collision**? The new case's `name:` must be unique within
  the flow.

### 4. Consider `cache_scope: shared` reuse

If a step in the new case is **byte-identical** to a step in another flow
or `canonical-prompts.md`, opt into `cache_scope: shared` so the new case
hits an existing recording on first run.

Don't paraphrase to "be more readable" — paraphrasing breaks the cache
key (which is `sha256("shared|" + stepText)`) and pays cold cost on the
new case's first record.

### 5. Author the new case

Read the existing cases for the flow's authoring style. Match it. Append
the new case to the `cases:` list — preserving:

- The flow's existing two-space indent.
- The flow's existing `tags:` taste (e.g. always include the surface tag
  if existing cases do).
- Wording for any sub-sequence that's verbatim across cases (chat
  prelude, navigation prelude) — copy byte-for-byte for cache reuse.

```yaml
  - name: <new descriptive name>
    tags: [<surface>, <classification>]
    steps:
      - act: |
          <one logical user action with explicit selectors>
        cache_scope: shared    # only if this step text is canonical
      - wait_for: <primitive>
    expect:
      - assert: <structural claim>
      - judge:  <one sentence covering the dev's stated success criterion>
```

Use `Edit` with the existing last case's `expect:` block as `old_string`
and the same block followed by the new case's YAML as `new_string` to
keep the diff minimal.

### 6. Self-checks

Same checks as `/test-feature`:

```bash
# Parse-check
cd web-app
ANTHROPIC_API_KEY=fake-test-key \
  npx tsx -e "import('./tests/agentic/runner/yaml-loader.ts').then(m => m.loadFlow('<flow-path>')).then(f => console.log('parsed OK,', f.cases.length, 'cases')).catch(e => { console.error('PARSE FAIL:', e.message); process.exit(1) })"

# Schema-check
npx --yes js-yaml '<flow-path>' > /tmp/flow.json
npx --yes ajv-cli@latest validate -s ../json-schemas/flow-test.json -d /tmp/flow.json

# Durability lint
pnpm test:agentic --dry-run <flow-stem>
```

If anything fails, iterate. Common cause: indentation drift between the
existing cases and the new one — match exactly.

### 7. Report back

Show the dev:

1. **The flow file you edited** and the new case's name.
2. **Running the new case alone**: positional filter matches the flow
   filename, so the new case runs along with every other case in the
   same flow:
   ```bash
   HEADED=1 pnpm test:agentic <flow-stem>
   ```
   If they want to scope to only the new case, suggest tagging it
   distinctively (e.g. `[<flow-name>, regression-2026-05-12]`) and
   filtering with `--tag`.
3. **Cost expectation**: the new case pays cold cost on its first run
   (~$0.05–0.40 per state-changing step; ~$1.00 for cloud-mode
   onboarding-class). Existing cases in the same flow still warm-replay —
   their cache entries are unaffected by an unrelated case being added.
4. **`_budgets.yml` impact**: budget entries are per-flow, not per-case.
   If the new case meaningfully changes the flow's cold cost shape,
   update the entry.

## Error handling

- **The flow's existing settings are wrong for the new case** (different
  `target:`, different `backend_mode:`, different setup). Don't mutate
  the flow's top-level settings to accommodate a divergent case — that
  invalidates every existing case's cache. Author a new flow instead.
- **The new case's name collides with an existing case.** Append a
  disambiguator (`-v2`, `-edge-case`) and tell the dev.
- **The flow has comments the dev cares about.** When using `Edit`, the
  surrounding comments must be preserved verbatim. Use a small
  `old_string` (the last case's final `expect:` line) so comments at the
  top of the file are untouched.
- **The new case uses a setup command the existing flow doesn't.** Either
  the new case really is a separate flow (route to `/test-feature`), or
  the dev should append the new setup command to the flow's existing
  `setup:` list (which then runs for every existing case — confirm with
  `AskUserQuestion` first; this invalidates every case's cache).

## Notes

- Don't reorder existing cases. Cache keys depend on `step_index` within
  a case (case ordering doesn't affect step indices, but reordering is
  gratuitous churn).
- Don't change the flow's `settings:` block. Cache entries depend on
  flow-file content; mutating settings invalidates every case's cache.
- If the dev's request really is for a new flow (different surface,
  different setup, different mode), say so explicitly and offer to run
  `/test-feature` instead.
- When copying a prelude step verbatim from another flow, also opt into
  `cache_scope: shared`. The byte-identical step text + shared scope
  means the new case hits the existing recording on first run.
