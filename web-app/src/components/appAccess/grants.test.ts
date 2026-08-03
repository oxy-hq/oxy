import { describe, expect, it } from "vitest";
import type { GrantablePerson, GranteeRef, Team } from "@/types/appAccess";
import {
  addGrant,
  estimateReach,
  grantedKeys,
  grantKey,
  removeGrant,
  setGrantRole,
  toGrantRows
} from "./grants";

const team = (id: string, name: string, member_count: number): Team => ({
  id,
  name,
  description: null,
  member_count,
  created_at: "2026-07-30T00:00:00Z"
});

const person = (id: string, name: string, email: string): GrantablePerson => ({
  user_id: id,
  name,
  email,
  role: "member"
});

describe("grantKey", () => {
  it("keeps a user and a team apart when they share an id", () => {
    // The two kinds live in different tables, so a collision is legal. If the key
    // dropped the kind, adding a team would silently no-op because a user with the
    // same UUID was already on the list.
    expect(grantKey("user", "abc")).not.toBe(grantKey("team", "abc"));
  });
});

describe("addGrant", () => {
  it("adds as 'member' by default — never 'admin' by accident", () => {
    const next = addGrant([], "team", "t1");
    expect(next).toEqual([{ kind: "team", id: "t1", role: "member" }]);
  });

  it("is idempotent and preserves the existing role", () => {
    const start: GranteeRef[] = [{ kind: "team", id: "t1", role: "admin" }];
    const next = addGrant(start, "team", "t1");
    // Re-adding must not quietly demote an admin grant back to member.
    expect(next).toBe(start);
  });

  it("adds a team and a user that share an id as two separate grants", () => {
    const next = addGrant(addGrant([], "user", "x"), "team", "x");
    expect(next).toHaveLength(2);
  });

  it("does not mutate the input", () => {
    const start: GranteeRef[] = [];
    addGrant(start, "team", "t1");
    expect(start).toHaveLength(0);
  });
});

describe("removeGrant", () => {
  it("removes only the matching kind", () => {
    const start: GranteeRef[] = [
      { kind: "user", id: "x", role: "member" },
      { kind: "team", id: "x", role: "member" }
    ];
    expect(removeGrant(start, "team", "x")).toEqual([{ kind: "user", id: "x", role: "member" }]);
  });

  it("is a no-op for something not on the list", () => {
    const start: GranteeRef[] = [{ kind: "team", id: "t1", role: "member" }];
    expect(removeGrant(start, "team", "nope")).toEqual(start);
  });
});

describe("setGrantRole", () => {
  it("changes only the targeted grant", () => {
    const start: GranteeRef[] = [
      { kind: "team", id: "t1", role: "member" },
      { kind: "team", id: "t2", role: "member" }
    ];
    expect(setGrantRole(start, "team", "t1", "admin")).toEqual([
      { kind: "team", id: "t1", role: "admin" },
      { kind: "team", id: "t2", role: "member" }
    ]);
  });

  it("does not touch a user grant with the same id", () => {
    const start: GranteeRef[] = [
      { kind: "user", id: "x", role: "member" },
      { kind: "team", id: "x", role: "member" }
    ];
    const next = setGrantRole(start, "team", "x", "admin");
    expect(next.find((g) => g.kind === "user")?.role).toBe("member");
    expect(next.find((g) => g.kind === "team")?.role).toBe("admin");
  });
});

describe("grantedKeys", () => {
  it("produces one key per grant, kind-qualified", () => {
    const keys = grantedKeys([
      { kind: "user", id: "x", role: "member" },
      { kind: "team", id: "x", role: "member" }
    ]);
    expect(keys.size).toBe(2);
    expect(keys.has("user:x")).toBe(true);
    expect(keys.has("team:x")).toBe(true);
  });
});

describe("toGrantRows", () => {
  const teams = [team("t1", "Finance", 12), team("t2", "Solo", 1)];
  const people = [person("u1", "Alice", "alice@acme.com")];

  it("labels a team with its headcount, pluralized", () => {
    const rows = toGrantRows(
      [
        { kind: "team", id: "t1", role: "member" },
        { kind: "team", id: "t2", role: "member" }
      ],
      teams,
      people
    );
    expect(rows[0]).toMatchObject({ name: "Finance", detail: "12 people" });
    expect(rows[1]).toMatchObject({ name: "Solo", detail: "1 person" });
  });

  it("labels a person with their email", () => {
    const rows = toGrantRows([{ kind: "user", id: "u1", role: "admin" }], teams, people);
    expect(rows[0]).toMatchObject({
      name: "Alice",
      detail: "alice@acme.com",
      role: "admin"
    });
  });

  it("keeps a grant whose grantee is missing, rather than dropping it", () => {
    // Dropping the row would mean pressing Save silently revokes a grant the admin
    // never saw — the list is the request body.
    const rows = toGrantRows(
      [
        { kind: "team", id: "gone", role: "member" },
        { kind: "user", id: "gone", role: "member" }
      ],
      teams,
      people
    );
    expect(rows).toHaveLength(2);
    expect(rows[0].name).toBe("Unknown team");
    expect(rows[1].name).toBe("Unknown person");
  });

  it("preserves the order it was given", () => {
    const rows = toGrantRows(
      [
        { kind: "user", id: "u1", role: "member" },
        { kind: "team", id: "t1", role: "member" }
      ],
      teams,
      people
    );
    expect(rows.map((r) => r.name)).toEqual(["Alice", "Finance"]);
  });
});

describe("estimateReach", () => {
  const teams = [team("t1", "Finance", 12), team("t2", "Ops", 30)];

  it("counts a person as one and a team as its headcount", () => {
    expect(
      estimateReach(
        [
          { kind: "user", id: "u1", role: "member" },
          { kind: "team", id: "t1", role: "member" }
        ],
        teams
      )
    ).toBe(13);
  });

  it("is zero for an empty list", () => {
    expect(estimateReach([], teams)).toBe(0);
  });

  it("treats an unknown team as zero rather than throwing", () => {
    expect(estimateReach([{ kind: "team", id: "gone", role: "member" }], teams)).toBe(0);
  });

  it("surfaces the number that matters when granting admin to a big team", () => {
    // The whole point of the figure: catching "I just handed 30 people the app's
    // privileged surface" before Save.
    expect(estimateReach([{ kind: "team", id: "t2", role: "admin" }], teams)).toBe(30);
  });
});
