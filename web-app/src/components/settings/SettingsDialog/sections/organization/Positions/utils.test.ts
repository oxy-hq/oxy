import { describe, expect, it } from "vitest";
import { assignmentLabel } from "@/libs/operatingGraph";
import type { AssignmentRow, RoleRow } from "@/types/operatingGraph";
import { heldByLabel, holderCounts, positionSummary } from "./utils";

const assignment = (over: Partial<AssignmentRow>): AssignmentRow => ({
  id: "a1",
  user_id: "u1",
  user_name: "Nia Okafor",
  user_kind: "frontline",
  role_id: "r1",
  role_name: "Shift lead",
  role_scope: "location",
  location_id: "l1",
  location_name: "Clovis",
  supervisor_id: null,
  supervisor_name: null,
  created_at: "2026-09-07T10:00:00Z",
  ...over
});

const role = (id: string, scope: RoleRow["scope"]): RoleRow => ({
  id,
  org_id: "org",
  name: id,
  scope,
  created_at: "2026-09-07T10:00:00Z",
  updated_at: "2026-09-07T10:00:00Z"
});

describe("holderCounts", () => {
  it("counts distinct people per position — one person at two stores is one holder", () => {
    const counts = holderCounts([
      assignment({ id: "a1", user_id: "u1", role_id: "lead", location_id: "l1" }),
      assignment({ id: "a2", user_id: "u1", role_id: "lead", location_id: "l2" }),
      assignment({ id: "a3", user_id: "u2", role_id: "lead", location_id: "l1" }),
      assignment({ id: "a4", user_id: "u2", role_id: "manager", location_id: null })
    ]);
    expect(counts.get("lead")).toBe(2);
    expect(counts.get("manager")).toBe(1);
    expect(counts.get("unheld")).toBeUndefined();
  });
});

describe("assignmentLabel", () => {
  it("reads place · position for a location position", () => {
    expect(assignmentLabel(assignment({}))).toBe("Clovis · Shift lead");
  });

  it("reads Org-wide for an org-wide position, whatever the location field says", () => {
    expect(
      assignmentLabel(
        assignment({ role_name: "Area manager", role_scope: "franchisor", location_name: null })
      )
    ).toBe("Org-wide · Area manager");
  });

  it("does not promote a location position whose place is gone to org-wide", () => {
    expect(assignmentLabel(assignment({ location_name: null }))).toBe("No location · Shift lead");
  });
});

describe("labels", () => {
  it("counts heads in plain words", () => {
    expect(heldByLabel(0)).toBe("Nobody");
    expect(heldByLabel(1)).toBe("1 person");
    expect(heldByLabel(3)).toBe("3 people");
  });

  it("summarises the vocabulary", () => {
    expect(positionSummary([role("a", "location"), role("b", "franchisor")])).toBe(
      "2 positions · 1 org-wide"
    );
    expect(positionSummary([role("a", "location")])).toBe("1 position · 0 org-wide");
  });
});
