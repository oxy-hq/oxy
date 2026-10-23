import { describe, expect, it } from "vitest";
import type { CustomDomain } from "@/types/apps";
import { dnsRecords, normalizeDomainInput } from "./dnsRecords";

const domain: CustomDomain = {
  id: "d-1",
  domain: "app.acme.com",
  status: "pending",
  url: "https://app.acme.com/",
  txt_name: "_oxygen-challenge.app.acme.com",
  txt_value: "oxygen-verification=abc123",
  verified_at: null,
  last_checked_at: null,
  last_check_error: null,
  created_at: "2026-10-23T00:00:00Z"
};

describe("dnsRecords", () => {
  it("lists the CNAME then the TXT record", () => {
    expect(dnsRecords(domain, "edge.oxy.tech")).toEqual([
      { type: "CNAME", name: "app.acme.com", value: "edge.oxy.tech" },
      {
        type: "TXT",
        name: "_oxygen-challenge.app.acme.com",
        value: "oxygen-verification=abc123"
      }
    ]);
  });

  it("leaves the CNAME value empty when no routing target is configured", () => {
    expect(dnsRecords(domain, null)[0]?.value).toBeNull();
    expect(dnsRecords(domain, "  ")[0]?.value).toBeNull();
  });
});

describe("normalizeDomainInput", () => {
  it("trims and lower-cases", () => {
    expect(normalizeDomainInput("  App.Acme.COM ")).toBe("app.acme.com");
  });

  it("returns null for blank input", () => {
    expect(normalizeDomainInput("   ")).toBeNull();
  });
});
