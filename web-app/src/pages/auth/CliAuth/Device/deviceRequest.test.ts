import { describe, expect, it } from "vitest";
import { cleanUserCode, deviceReturnTo, readDeviceAsk } from "./deviceRequest";

describe("readDeviceAsk", () => {
  it("reads a request with no kind as oxyc login", () => {
    expect(readDeviceAsk({})).toEqual({ kind: "login" });
  });

  it("reads a token request by the link's own rules", () => {
    expect(
      readDeviceAsk({ kind: "sandbox_agent", apps: "acme/store,acme/ops", hours: "8", name: "n" })
    ).toEqual({ kind: "mint", ask: { apps: ["acme/store", "acme/ops"], hours: "8", name: "n" } });
    expect(readDeviceAsk({ kind: "agent", hours: "4", standing: "1" })).toEqual({
      kind: "agent_mint",
      ask: { hours: "4", name: null, standing: true }
    });
  });

  it("refuses what a link would refuse", () => {
    expect(readDeviceAsk({ kind: "personal" }).kind).toBe("invalid");
    expect(readDeviceAsk({ kind: "agent", apps: "acme/store" }).kind).toBe("invalid");
    expect(readDeviceAsk({ kind: "agent", standing: "maybe" }).kind).toBe("invalid");
  });
});

describe("cleanUserCode", () => {
  it("reads a code however it was typed", () => {
    expect(cleanUserCode("BCDF-GHJK")).toBe("BCDF-GHJK");
    expect(cleanUserCode("bcdfghjk")).toBe("BCDF-GHJK");
    expect(cleanUserCode(" bcdf ghjk ")).toBe("BCDF-GHJK");
  });

  it("refuses what cannot be one", () => {
    for (const bad of ["", "BCDF-GHJ", "BCDF-GHJA", "BCDF-GH0K", "BCDF-GHJKL"]) {
      expect(cleanUserCode(bad)).toBeNull();
    }
  });

  it("comes back to the same code after signing in", () => {
    expect(deviceReturnTo("https://oxy.example", "BCDF-GHJK")).toBe(
      "https://oxy.example/cli-auth/device?code=BCDF-GHJK"
    );
  });
});
