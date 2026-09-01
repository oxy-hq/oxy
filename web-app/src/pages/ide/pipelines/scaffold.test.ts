import { describe, expect, it } from "vitest";

import { buildPipelineScaffold, firstOfLastMonth } from "./scaffold";

describe("buildPipelineScaffold", () => {
  it("emits a database-reference destination, not raw credentials", () => {
    const yaml = buildPipelineScaffold({
      name: "shopify_raw",
      sourceId: "rest_api",
      destinationDatabase: "my_warehouse",
      datasetName: "shopify_raw"
    });
    expect(yaml).toContain("name: shopify_raw");
    expect(yaml).toContain("kind: rest_api");
    expect(yaml).toContain("database: my_warehouse");
    expect(yaml).toContain("dataset_name: shopify_raw");
    expect(yaml).not.toContain("connection_string"); // never in the dest block
    expect(yaml).toContain("concurrency: 1");
  });

  it("emits a toast source with a secret var, never the raw secret", () => {
    const yaml = buildPipelineScaffold({
      name: "toast_daily",
      sourceId: "toast",
      toast: {
        clientId: "abc123",
        clientSecretVar: "TOAST_PROD_SECRET",
        restaurantGuids: ["11111111-2222-3333-4444-555555555555", "guid-2"],
        baseUrl: "https://sandbox.toasttab.com"
      },
      destinationDatabase: "wh",
      datasetName: "toast_daily"
    });
    expect(yaml).toContain("kind: toast");
    expect(yaml).toContain("client_id: abc123");
    expect(yaml).toContain("client_secret_var: TOAST_PROD_SECRET");
    expect(yaml).toContain('- "11111111-2222-3333-4444-555555555555"');
    expect(yaml).toContain('- "guid-2"');
    expect(yaml).toContain("base_url: https://sandbox.toasttab.com");
    // The secret value itself is never scaffolded.
    expect(yaml).not.toContain("client_secret:");
  });

  it("emits a quickbooks source with secret vars, never raw secrets", () => {
    const yaml = buildPipelineScaffold({
      name: "qb_daily",
      sourceId: "quickbooks",
      quickbooks: {
        clientId: "intuit-abc",
        clientSecretVar: "QB_PROD_SECRET",
        refreshTokenVar: "QB_PROD_REFRESH",
        realmId: "9130350000000000",
        baseUrl: "https://sandbox-quickbooks.api.intuit.com"
      },
      destinationDatabase: "wh",
      datasetName: "qb_daily"
    });
    expect(yaml).toContain("kind: quickbooks");
    expect(yaml).toContain("client_id: intuit-abc");
    expect(yaml).toContain("client_secret_var: QB_PROD_SECRET");
    expect(yaml).toContain("refresh_token_var: QB_PROD_REFRESH");
    // realm_id is quoted so YAML keeps the all-digits id a string.
    expect(yaml).toContain('realm_id: "9130350000000000"');
    expect(yaml).toContain("base_url: https://sandbox-quickbooks.api.intuit.com");
    // Neither secret value is ever scaffolded.
    expect(yaml).not.toContain("client_secret:");
    expect(yaml).not.toContain("refresh_token:");
  });

  it("emits an sp_api source with secret vars, never raw secrets", () => {
    const yaml = buildPipelineScaffold({
      name: "amazon_daily",
      sourceId: "sp_api",
      spApi: {
        clientId: "amzn1.application-oa2-client.abc",
        clientSecretVar: "SP_API_PROD_SECRET",
        refreshTokenVar: "SP_API_PROD_REFRESH",
        marketplaceId: "A2EUQ1WTGCTBG2",
        defaultStart: "2026-01-01"
      },
      destinationDatabase: "wh",
      datasetName: "amazon_daily"
    });
    expect(yaml).toContain("kind: sp_api");
    expect(yaml).toContain("client_id: amzn1.application-oa2-client.abc");
    expect(yaml).toContain("client_secret_var: SP_API_PROD_SECRET");
    expect(yaml).toContain("refresh_token_var: SP_API_PROD_REFRESH");
    expect(yaml).toContain("marketplace_id: A2EUQ1WTGCTBG2");
    // Neither secret VALUE is ever scaffolded — only the names.
    expect(yaml).not.toContain("client_secret:");
    expect(yaml).not.toContain("refresh_token:");
  });

  it("always emits default_start, quoted so YAML keeps it a string", () => {
    const yaml = buildPipelineScaffold({
      name: "amazon_daily",
      sourceId: "sp_api",
      spApi: {
        clientId: "c",
        clientSecretVar: "S",
        refreshTokenVar: "R",
        marketplaceId: "ATVPDKIKX0DER",
        defaultStart: "2026-01-01"
      },
      destinationDatabase: "wh",
      datasetName: "amazon_daily"
    });
    // Quoted: unquoted, YAML parses it as a date and the connector's String
    // field rejects it with a serde error naming the struct, not the field.
    expect(yaml).toContain('default_start: "2026-01-01"');
    // Never omitted-to-default. The connector pulls forward only, so this is
    // the entire backfill policy and `build_sp_api` refuses a config without
    // it — the wizard must not be able to produce one.
    expect(yaml).toMatch(/default_start:/);
  });

  it("omits base_url when not provided", () => {
    const yaml = buildPipelineScaffold({
      name: "t",
      sourceId: "toast",
      toast: { clientId: "x", clientSecretVar: "S", restaurantGuids: ["g"] },
      destinationDatabase: "wh",
      datasetName: "t"
    });
    expect(yaml).not.toContain("base_url:");
  });

  it("includes the description line only when provided", () => {
    expect(
      buildPipelineScaffold({
        name: "p",
        sourceId: "filesystem",
        destinationDatabase: "wh",
        datasetName: "p"
      })
    ).not.toContain("description:");

    expect(
      buildPipelineScaffold({
        name: "p",
        description: "hourly ingest",
        sourceId: "filesystem",
        destinationDatabase: "wh",
        datasetName: "p"
      })
    ).toContain('description: "hourly ingest"');
  });

  it("scaffolds the per-source config block", () => {
    expect(
      buildPipelineScaffold({
        name: "p",
        sourceId: "postgres_cdc",
        destinationDatabase: "wh",
        datasetName: "p"
      })
    ).toContain("publication_name: oxy_pub");
  });

  it("scaffolds a clickhouse source with a password var, never a raw secret", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "clickhouse",
      destinationDatabase: "wh",
      datasetName: "p"
    });
    expect(yaml).toContain("kind: clickhouse");
    expect(yaml).toContain("password_var: CLICKHOUSE_PASSWORD");
    expect(yaml).not.toMatch(/^\s*password:/m);
  });

  it("scaffolds a clickhouse source from picked credentials and tables", () => {
    const yaml = buildPipelineScaffold({
      name: "ch_raw",
      sourceId: "clickhouse",
      clickhouse: {
        host: "h.clickhouse.cloud",
        port: 8443,
        database: "analytics",
        username: "reader",
        passwordVar: "CH_PW",
        secure: true,
        tables: [
          { name: "events", writeDisposition: "append", cursorField: "created_at" },
          { name: "users", writeDisposition: "merge", primaryKey: ["id"] },
          { name: "products", writeDisposition: "replace" }
        ]
      },
      destinationDatabase: "wh",
      datasetName: "ch_raw",
      destinationIsAirhouse: true
    });
    expect(yaml).toContain("kind: clickhouse");
    expect(yaml).toContain("host: h.clickhouse.cloud");
    expect(yaml).toContain("password_var: CH_PW");
    // Per-table disposition: append+cursor, merge+key, replace.
    expect(yaml).toContain("- name: events");
    expect(yaml).toContain("cursor_field: created_at");
    expect(yaml).toContain("write_disposition: append");
    expect(yaml).toContain("- name: users");
    expect(yaml).toContain("primary_key:");
    expect(yaml).toContain("- id");
    expect(yaml).toContain("write_disposition: merge");
    expect(yaml).toContain("- name: products");
    expect(yaml).toContain("write_disposition: replace");
    // The raw password is never written into the YAML.
    expect(yaml).not.toMatch(/^\s*password:/m);
    // ClickHouse pipelines default to splitting `a___b` into schema.table.
    expect(yaml).toContain('schema_separator: "___"');
  });

  it("only adds schema_separator for clickhouse sources", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "postgres_cdc",
      destinationDatabase: "wh",
      datasetName: "p",
      destinationIsAirhouse: true
    });
    expect(yaml).not.toContain("schema_separator");
  });

  it("omits schema_separator for clickhouse into a non-airhouse destination", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "clickhouse",
      clickhouse: {
        host: "h",
        database: "d",
        passwordVar: "CH_PW",
        tables: [{ name: "events", writeDisposition: "append" }]
      },
      destinationDatabase: "pg_wh",
      datasetName: "p",
      destinationIsAirhouse: false
    });
    expect(yaml).toContain("kind: clickhouse");
    // schema_separator is airhouse-only; a postgres destination rejects it.
    expect(yaml).not.toContain("schema_separator");
  });

  it("falls back to the first option for an unknown source id", () => {
    const yaml = buildPipelineScaffold({
      name: "p",
      sourceId: "nope",
      destinationDatabase: "wh",
      datasetName: "p"
    });
    expect(yaml).toContain("kind: rest_api");
  });
});

describe("firstOfLastMonth", () => {
  it("is the first of the previous month", () => {
    expect(firstOfLastMonth(new Date("2026-09-15T12:00:00Z"))).toBe("2026-08-01");
  });

  it("rolls back across the year boundary", () => {
    // `new Date(y, -1, 1)` would be December of the previous year only because
    // Date normalises it; asserted rather than assumed.
    expect(firstOfLastMonth(new Date("2026-01-05T00:00:00Z"))).toBe("2025-12-01");
  });

  it("bounds the first window between one and two months", () => {
    // This is the whole point of anchoring to a month boundary. `plan_pull`
    // asks for the span as ONE report and the cursor only advances on success,
    // so a window too large to build fails identically on every later run —
    // the span has to be self-limiting rather than open-ended.
    const dayMs = 24 * 60 * 60 * 1000;
    for (const iso of [
      "2026-03-01T00:00:00Z", // first of the month — the shortest span
      "2026-03-31T23:59:59Z", // last of the month — the longest span
      "2026-01-01T00:00:00Z", // year boundary, shortest
      "2026-12-31T00:00:00Z" // year end, longest
    ]) {
      const now = new Date(iso);
      const spanDays = (now.getTime() - new Date(firstOfLastMonth(now)).getTime()) / dayMs;
      expect(spanDays).toBeGreaterThanOrEqual(27); // a short February still clears a month
      expect(spanDays).toBeLessThan(63); // never more than two months
    }
  });

  it("never returns a future or same-month date", () => {
    // A start at or after `now` makes `plan_pull` return `UpToDate`, so the
    // pipeline would land nothing at all and look healthy doing it.
    const now = new Date("2026-06-10T00:00:00Z");
    expect(new Date(firstOfLastMonth(now)).getTime()).toBeLessThan(now.getTime());
    expect(firstOfLastMonth(now).slice(0, 7)).not.toBe("2026-06");
  });
});
