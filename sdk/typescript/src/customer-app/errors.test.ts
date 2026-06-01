import { describe, expect, it } from "vitest";
import { interpretCustomerAppError } from "./errors";

describe("interpretCustomerAppError", () => {
  describe("manifest", () => {
    it("404 → manifest not found", () => {
      const r = interpretCustomerAppError(new Error("Failed to load oxy-app.json: HTTP 404"));
      expect(r.title).toBe("Manifest not found");
    });

    it("network failure → manifest could not be loaded", () => {
      const r = interpretCustomerAppError(new Error("Failed to load oxy-app.json: NetworkError"));
      expect(r.title).toBe("Manifest could not be loaded");
    });

    it("v1 manifest → schema mismatch", () => {
      const r = interpretCustomerAppError(
        new Error("oxy-app.json: schemaVersion must be 2 (got 1)")
      );
      expect(r.title).toBe("Manifest schema mismatch");
    });
  });

  describe("/query 401/403/404", () => {
    it("401 → session expired", () => {
      const r = interpretCustomerAppError(new Error("401: unauthorized"));
      expect(r.title).toBe("Session expired");
    });

    it("403 origin not allowed → CSRF branch", () => {
      const r = interpretCustomerAppError(new Error("403: origin not allowed"));
      expect(r.title).toBe("Request origin not allowed");
    });

    it("403 not a member → access denied branch", () => {
      const r = interpretCustomerAppError(new Error("403: forbidden: not a member of this org"));
      expect(r.title).toBe("Access denied");
    });

    it("403 SELECT-only gate → read-only branch", () => {
      const r = interpretCustomerAppError(new Error("403: only SELECT/WITH allowed; reject DROP"));
      expect(r.title).toBe("Query rejected — read-only endpoint");
    });

    it("404 project → project not found", () => {
      const r = interpretCustomerAppError(new Error("404: project not found"));
      expect(r.title).toBe("Project not found");
    });
  });

  describe("/query 400/500/502", () => {
    it("400 empty SQL → empty SQL branch", () => {
      const r = interpretCustomerAppError(new Error("400: `sql` must be non-empty"));
      expect(r.title).toBe("Empty SQL");
    });

    it("400 query failed → query failed", () => {
      const r = interpretCustomerAppError(
        new Error("400: query failed; see server logs for details")
      );
      expect(r.title).toBe("Query failed");
    });

    it("502 → warehouse unreachable", () => {
      const r = interpretCustomerAppError(new Error("502: warehouse connection failed"));
      expect(r.title).toBe("Warehouse unreachable");
    });
  });

  describe("stale-bundle / SPA fallback", () => {
    it("doctype HTML → fetched HTML where JSON expected", () => {
      const r = interpretCustomerAppError(
        new Error("Unexpected token '<', \"<!doctype \"... is not valid JSON")
      );
      expect(r.title).toBe("Fetched HTML where JSON was expected");
      expect(r.hint).toMatch(/rebuild/i);
    });
  });

  describe("catch-all", () => {
    it("unknown error → generic", () => {
      const r = interpretCustomerAppError(new Error("something weird"));
      expect(r.title).toBe("Unexpected error loading the dashboard");
    });

    it("non-Error → coerces to string", () => {
      const r = interpretCustomerAppError("plain string");
      expect(r.message).toBe("plain string");
    });
  });
});
