import { describe, expect, it } from "vitest";
import { parseQuickBooksSource } from "./parsePipelineSource";

describe("parseQuickBooksSource", () => {
  const yaml = `
name: qb_daily
source:
  kind: quickbooks
  config:
    client_id: intuit-abc
    client_secret_var: QB_CLIENT_SECRET
    refresh_token_var: QB_REFRESH_TOKEN
    realm_id: "9341456860808037"
destination:
  database: warehouse
  dataset_name: qb_daily
`;

  it("extracts the quickbooks source config", () => {
    expect(parseQuickBooksSource(yaml)).toEqual({
      kind: "quickbooks",
      clientId: "intuit-abc",
      clientSecretVar: "QB_CLIENT_SECRET",
      refreshTokenVar: "QB_REFRESH_TOKEN"
    });
  });

  it("returns null for a non-quickbooks source", () => {
    const toast = `
source:
  kind: toast
  config:
    client_id: abc
`;
    expect(parseQuickBooksSource(toast)).toBeNull();
  });

  it("returns null when required fields are missing", () => {
    const partial = `
source:
  kind: quickbooks
  config:
    client_id: abc
`;
    expect(parseQuickBooksSource(partial)).toBeNull();
  });

  it("returns null for malformed yaml", () => {
    expect(parseQuickBooksSource(": : : not yaml : :")).toBeNull();
  });
});
