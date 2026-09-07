import { describe, expect, it } from "vitest";
import { sectionFromParam } from "./useSettingsDeepLink";

describe("sectionFromParam", () => {
  it("accepts every section the nav has, organization and workspace alike", () => {
    expect(sectionFromParam("organization.general")).toBe("organization.general");
    expect(sectionFromParam("organization.crew")).toBe("organization.crew");
    expect(sectionFromParam("workspace.databases")).toBe("workspace.databases");
    expect(sectionFromParam("preferences.appearance")).toBe("preferences.appearance");
  });

  it("drops anything the dialog does not have, and the empty cases", () => {
    // A typo must not open some other section.
    expect(sectionFromParam("organization.crews")).toBeNull();
    expect(sectionFromParam("admin.billing")).toBeNull();
    expect(sectionFromParam("")).toBeNull();
    expect(sectionFromParam(null)).toBeNull();
    expect(sectionFromParam(undefined)).toBeNull();
  });
});
