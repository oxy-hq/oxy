import type { UseQueryResult } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import type {
  AirwayContractPolicy,
  AirwayEnvironment,
  AirwayPolicyPreviewResponse,
  AirwayResourceVerdict,
  AirwayUnevaluatedPipeline
} from "@/services/api/airwayConfig";
import { computeSaveGate, type PreviewSubject, resolveInherited, splitPipelineRef } from "./utils";

/**
 * The save gate is the only thing standing between an operator and an
 * outage: tightening `contract_policy` halts every pipeline whose resources
 * don't satisfy it, and the refusal surfaces hours later as a config error
 * from a queued worker. It has now been fixed four times, every time for the
 * same reason — a state that meant *unknown* was being read as *safe* — and
 * until this file existed it had no automated coverage at all, so each
 * regression was found by a human re-reading it.
 *
 * So: every gate state is pinned here, plus the specific historical defects,
 * named. These exercise the real exported `computeSaveGate`.
 */

/**
 * `UseQueryResult` is a wide discriminated union; `computeSaveGate` reads four
 * fields of it. The helpers below build exactly those and cast at the seam,
 * which keeps the cast in one place instead of at every call site — and keeps
 * the tests driving the real function rather than a re-modelled one.
 */
type PreviewQueryResult = UseQueryResult<AirwayPolicyPreviewResponse>;

function asQueryResult(partial: {
  isSuccess?: boolean;
  isError?: boolean;
  isFetching?: boolean;
  data?: AirwayPolicyPreviewResponse;
}): PreviewQueryResult {
  return {
    isSuccess: false,
    isError: false,
    isFetching: false,
    data: undefined,
    ...partial
  } as unknown as PreviewQueryResult;
}

function verdict(overrides: Partial<AirwayResourceVerdict> = {}): AirwayResourceVerdict {
  return {
    pipeline_ref: "11111111-1111-1111-1111-111111111111:pipelines/toast.airway.yml",
    resource: "orders",
    mutability: "undeclared",
    passes: true,
    reason: null,
    not_fixable_here: false,
    ...overrides
  };
}

function unevaluated(path: string): AirwayUnevaluatedPipeline {
  return {
    pipeline_ref: `11111111-1111-1111-1111-111111111111:pipelines/${path}`,
    error: "connector could not be built: invalid toast config"
  };
}

/** A successful body. Defaults to the airway defaults, which is what an unset draft resolves to. */
function body(overrides: Partial<AirwayPolicyPreviewResponse> = {}): AirwayPolicyPreviewResponse {
  return {
    source_kind: "toast",
    contract_policy: "permissive",
    environment: "production",
    resources: [],
    unevaluated: [],
    uncompiled_workspaces: 0,
    out_of_scope_pipelines: 0,
    ...overrides
  };
}

/** A query that succeeded with `data`. */
function succeeded(data: AirwayPolicyPreviewResponse): PreviewQueryResult {
  return asQueryResult({ isSuccess: true, data });
}

function subject(
  contractPolicy: AirwayContractPolicy | null,
  environment: AirwayEnvironment | null
): PreviewSubject {
  return { contractPolicy, environment };
}

/** The draft matching `body()`'s defaults: nothing set, so airway's defaults apply. */
const INHERIT_BOTH = subject(null, null);

describe("computeSaveGate — the six gate states", () => {
  it("is never-previewed when the operator has not asked for a preview", () => {
    expect(computeSaveGate(asQueryResult({}), true, INHERIT_BOTH, "global")).toEqual({
      kind: "unknown",
      reason: "never-previewed"
    });
  });

  it("is loading while the preview is in flight", () => {
    expect(
      computeSaveGate(asQueryResult({ isFetching: true }), true, INHERIT_BOTH, "global")
    ).toEqual({
      kind: "unknown",
      reason: "loading"
    });
  });

  it("is error when the preview failed to load", () => {
    expect(computeSaveGate(asQueryResult({ isError: true }), true, INHERIT_BOTH, "global")).toEqual(
      {
        kind: "unknown",
        reason: "error"
      }
    );
  });

  it("is incomplete when a pipeline could not be evaluated", () => {
    const gate = computeSaveGate(
      succeeded(body({ unevaluated: [unevaluated("a.airway.yml"), unevaluated("b.airway.yml")] })),
      true,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "incomplete", unevaluatedCount: 2 });
  });

  it("is failures when resources would be rejected", () => {
    const gate = computeSaveGate(
      succeeded(
        body({
          contract_policy: "require_declared",
          resources: [verdict({ passes: false }), verdict({ resource: "menus", passes: false })]
        })
      ),
      true,
      subject("require_declared", null),
      "global"
    );
    expect(gate).toEqual({ kind: "failures", failingCount: 2 });
  });

  it("is clean only for a successful, zero-failure, fully-evaluated preview", () => {
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict(), verdict({ resource: "menus" })] })),
      true,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("counts only failing resources, not every resource scanned", () => {
    const gate = computeSaveGate(
      succeeded(
        body({
          contract_policy: "forbid_opaque",
          resources: [verdict(), verdict({ resource: "menus", passes: false })]
        })
      ),
      true,
      subject("forbid_opaque", null),
      "global"
    );
    expect(gate).toEqual({ kind: "failures", failingCount: 1 });
  });
});

describe("computeSaveGate — a closed disclosure is never trusted", () => {
  /**
   * React Query serves cached data for an unchanged key even with
   * `enabled: false`, so `isSuccess` can be `true` off a stale cache hit. A
   * preview the operator is not looking at is not a preview.
   */
  it("reports never-previewed for a clean cached body when the disclosure is closed", () => {
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict()] })),
      false,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "never-previewed" });
  });

  it("does not even look at failures when the disclosure is closed", () => {
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict({ passes: false })] })),
      false,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "never-previewed" });
  });
});

describe("computeSaveGate — never-compiled workspaces must not gate (regression)", () => {
  /**
   * The defect: the server folded "this workspace has no promoted revision"
   * into `unevaluated` as one synthetic entry. On any real deployment some
   * workspace has never compiled, so `unevaluated` was never empty, so the
   * gate sat in `incomplete` forever and *every* save confirmed. A
   * confirmation that always fires trains operators to click through it, which
   * defeats the guardrail entirely.
   *
   * The fix is a distinction, not a threshold: the count now rides its own
   * `uncompiled_workspaces` field, and the gate keys only on `unevaluated`.
   */
  it("is clean when only never-compiled workspaces are reported", () => {
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict()], uncompiled_workspaces: 7 })),
      true,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("is clean even when every workspace in the fleet has never compiled", () => {
    const gate = computeSaveGate(
      succeeded(body({ resources: [], uncompiled_workspaces: 42 })),
      true,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("still gates on real unevaluated pipelines, and counts only those", () => {
    const gate = computeSaveGate(
      succeeded(
        body({ unevaluated: [unevaluated("broken.airway.yml")], uncompiled_workspaces: 7 })
      ),
      true,
      INHERIT_BOTH,
      "global"
    );
    // 1, not 8: the two facts are never summed.
    expect(gate).toEqual({ kind: "unknown", reason: "incomplete", unevaluatedCount: 1 });
  });

  /**
   * Same rule, second field — but for an **override** only, and that
   * qualification carries the whole of `partial-scope`. The preview is fenced
   * to the caller's platform scope, so a bounded operator's body always carries
   * a non-zero `out_of_scope_pipelines`: it describes the *grant*, not the
   * draft. An override lands on one in-scope workspace the scan did cover, so
   * gating it on the fleet-wide remainder would confirm every override a scoped
   * operator ever adds — the identical confirmation-fatigue defect one field
   * over. The global row is the opposite case; see the next describe.
   */
  it("is clean for an override when pipelines were withheld by the caller's scope", () => {
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict()], out_of_scope_pipelines: 137 })),
      true,
      INHERIT_BOTH,
      "override"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  // Also the reason ordering: `incomplete` outranks `partial-scope`, because an
  // `unevaluated` entry names a pipeline the operator can go and read.
  it("counts only unevaluated pipelines when both are reported", () => {
    const gate = computeSaveGate(
      succeeded(
        body({
          unevaluated: [unevaluated("broken.airway.yml")],
          uncompiled_workspaces: 7,
          out_of_scope_pipelines: 137
        })
      ),
      true,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "incomplete", unevaluatedCount: 1 });
  });
});

describe("computeSaveGate — the global row reaches further than a fenced preview (regression)", () => {
  /**
   * The defect: `out_of_scope_pipelines` was excluded from the gate outright,
   * on the reasoning that gating on it would confirm every save a bounded
   * operator makes. That reasoning holds for an override and fails for the
   * global row. An override writes one workspace, inside the caller's scope,
   * which the preview covered — `clean` is a true statement about it. The
   * global row is fleet-wide by construction, and the preview provably did
   * *not* score `out_of_scope_pipelines` of the pipelines it will govern.
   *
   * Concretely: a `global_admin` scoped to two orgs flips `toast` to
   * `require_declared`, sees a clean scan of those two orgs, and saves with no
   * confirmation — halting pipelines in tenants they cannot see. "Reads as
   * safe, means unknown", which is the shape this gate exists to prevent.
   *
   * The fatigue objection does not transfer: a global policy flip is rare,
   * high-stakes, and for a bounded grant it is *always* partially blind, so
   * confirming every time is the honest reading rather than noise.
   */
  it("confirms a global save when the scan could not see part of the fleet", () => {
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict()], out_of_scope_pipelines: 137 })),
      true,
      INHERIT_BOTH,
      "global"
    );
    // The count rides along so the confirm copy can say how much went unscanned.
    expect(gate).toEqual({ kind: "unknown", reason: "partial-scope", outOfScopeCount: 137 });
  });

  it("stays clean for a global save when nothing was withheld", () => {
    // An unbounded grant — a Global Owner, or `scope_all` — always reports 0,
    // so this reason never reaches them and the surface is unchanged for them.
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict()], out_of_scope_pipelines: 0 })),
      true,
      INHERIT_BOTH,
      "global"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("leaves an override save alone when pipelines were withheld", () => {
    // The tier is the whole distinction: same body, same subject, opposite gate.
    const gate = computeSaveGate(
      succeeded(body({ resources: [verdict()], out_of_scope_pipelines: 137 })),
      true,
      INHERIT_BOTH,
      "override"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("still prefers a known failure count over the unscanned remainder", () => {
    // `failures` is the actionable answer and outranks every "unknown" reason —
    // a partial scan that already found a real failure should say so.
    const gate = computeSaveGate(
      succeeded(
        body({
          contract_policy: "require_declared",
          resources: [verdict({ passes: false })],
          out_of_scope_pipelines: 137
        })
      ),
      true,
      subject("require_declared", null),
      "global"
    );
    expect(gate).toEqual({ kind: "failures", failingCount: 1 });
  });
});

describe("computeSaveGate — a preview is trusted only for the settings it was computed under", () => {
  /**
   * The defect: the preview query key was `(sourceKind, contractPolicy)` and
   * excluded `environment`, so changing only `environment` reused the cached
   * body — `isSuccess` still true, gate still `clean`, saved with no
   * confirmation. `environment` is a real admission axis (`source_factory`
   * refuses connectors under `Sandbox`), so a scan run under `production`
   * describes a different question.
   *
   * The key now carries both axes and the body echoes both, so this is the
   * belt to that braces: trust is decided by what the body *says* it is, not
   * by which cache slot it came out of.
   */
  it("is stale when the body was computed under a different environment", () => {
    const gate = computeSaveGate(
      succeeded(body({ environment: "production", resources: [verdict()] })),
      true,
      subject(null, "sandbox"),
      "global"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "stale" });
  });

  it("is stale when the body was computed under a different contract policy", () => {
    const gate = computeSaveGate(
      succeeded(body({ contract_policy: "permissive", resources: [verdict()] })),
      true,
      subject("require_declared", null),
      "global"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "stale" });
  });

  /** A failure count for the wrong question is not this save's failure count. */
  it("prefers stale over failures when the body describes other settings", () => {
    const gate = computeSaveGate(
      succeeded(
        body({ contract_policy: "require_declared", resources: [verdict({ passes: false })] })
      ),
      true,
      subject("forbid_opaque", null),
      "global"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "stale" });
  });

  it("treats an unset draft field as airway's default, matching the server's echo", () => {
    // The client sends no parameter for an unset field and the server echoes
    // its default, so `null` must compare equal to `permissive` / `production`
    // — otherwise the common case (nothing overridden) would read as stale.
    const gate = computeSaveGate(
      succeeded(body({ contract_policy: "permissive", environment: "production" })),
      true,
      subject(null, null),
      "global"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("is clean when both axes match explicitly", () => {
    const gate = computeSaveGate(
      succeeded(
        body({ contract_policy: "forbid_opaque", environment: "sandbox", resources: [verdict()] })
      ),
      true,
      subject("forbid_opaque", "sandbox"),
      "global"
    );
    expect(gate).toEqual({ kind: "clean" });
  });
});

describe("an override inherits the kind's GLOBAL row, not airway's default", () => {
  /**
   * The defect: `AddOverrideDialog` previewed the raw draft. An override that
   * leaves `contract_policy` on "Inherit this kind's policy" runs under the
   * kind's global row — `resolve_admission` merges field by field, narrowest
   * non-null winning — but omitting the parameter made the server preview
   * `permissive`, and `describesSubject` compared against a subject that also
   * defaulted to `permissive`. Echo matched, gate read **clean**.
   *
   * Concretely: global `toast` = `require_declared`; the operator adds an
   * override for workspace W, leaves policy on inherit, sets Environment →
   * Sandbox. An all-clear scan rendered directly beneath copy promising W
   * follows the global policy, and Save went through unconfirmed with
   * `require_declared` never scored.
   *
   * The fix is `resolveInherited` at both seams — the preview request AND the
   * `PreviewSubject` — so these drive the same two-step the dialog does.
   */
  const inherit = (draftPolicy: string | null, globalPolicy: string | null): PreviewSubject => ({
    contractPolicy: resolveInherited(draftPolicy, globalPolicy),
    environment: resolveInherited(null, null)
  });

  it("resolves draft first, then the global row, then airway's default", () => {
    expect(resolveInherited("forbid_opaque", "require_declared")).toBe("forbid_opaque");
    expect(resolveInherited(null, "require_declared")).toBe("require_declared");
    expect(resolveInherited(null, null)).toBeNull();
    expect(resolveInherited(null, undefined)).toBeNull();
  });

  it("does NOT read a permissive scan as clean when the override inherits require_declared", () => {
    const gate = computeSaveGate(
      succeeded(body({ contract_policy: "permissive", resources: [verdict()] })),
      true,
      inherit(null, "require_declared"),
      "override"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "stale" });
  });

  it("is clean once the scan was computed under the inherited policy", () => {
    const gate = computeSaveGate(
      succeeded(body({ contract_policy: "require_declared", resources: [verdict()] })),
      true,
      inherit(null, "require_declared"),
      "override"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("reports the inherited policy's failures, not the default's zero", () => {
    const gate = computeSaveGate(
      succeeded(
        body({
          contract_policy: "require_declared",
          resources: [verdict({ passes: false }), verdict({ resource: "menus" })]
        })
      ),
      true,
      inherit(null, "require_declared"),
      "override"
    );
    expect(gate).toEqual({ kind: "failures", failingCount: 1 });
  });

  it("lets an explicit draft override the global row", () => {
    // The draft is `forbid_opaque`; the global row's `require_declared` must
    // not win, or the operator previews a policy they are replacing.
    const gate = computeSaveGate(
      succeeded(body({ contract_policy: "forbid_opaque", resources: [verdict()] })),
      true,
      inherit("forbid_opaque", "require_declared"),
      "override"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  it("falls back to airway's default only when no global row exists", () => {
    const gate = computeSaveGate(
      succeeded(body({ contract_policy: "permissive", resources: [verdict()] })),
      true,
      inherit(null, null),
      "override"
    );
    expect(gate).toEqual({ kind: "clean" });
  });

  /**
   * A stored value this build doesn't recognise must not be quietly replaced
   * by a default. It rides through as-is, the server refuses to compute a
   * preview for it, and the gate lands on `unknown` — so the save confirms
   * rather than claiming a clean scan of a policy nobody ran.
   */
  it("carries an unrecognised stored spelling through rather than defaulting it", () => {
    expect(resolveInherited(null, "require-declared")).toBe("require-declared");
    const gate = computeSaveGate(
      succeeded(body({ contract_policy: "permissive", resources: [verdict()] })),
      true,
      inherit(null, "require-declared"),
      "override"
    );
    expect(gate).toEqual({ kind: "unknown", reason: "stale" });
  });
});

describe("splitPipelineRef", () => {
  it("splits on the first colon, keeping the rest of the path intact", () => {
    expect(splitPipelineRef("ws-1:pipelines/a:b.airway.yml")).toEqual({
      workspaceId: "ws-1",
      path: "pipelines/a:b.airway.yml"
    });
  });

  it("treats a colon-less ref as a bare path", () => {
    expect(splitPipelineRef("pipelines/a.airway.yml")).toEqual({
      workspaceId: "",
      path: "pipelines/a.airway.yml"
    });
  });
});
