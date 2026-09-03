/**
 * The pieces every command sits on: exit codes, table rendering, the `gh`
 * truncation guard, and the request path itself.
 *
 * The request tests run against a real loopback server rather than a mocked
 * `fetch`, because the thing worth checking is which HEADER goes out for which
 * path — and a mock proves only that the mock was called.
 */

import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { buildUrl, errorForResponse, request } from "../api/request.js";
import { refuseIfAtLimit } from "../github/gh.js";
import { table } from "../ui/render.js";
import { ExitCode, exitCodeForStatus } from "./errors.js";

describe("exitCodeForStatus", () => {
  /**
   * An agent branches on the code ALONE, so these are the difference between
   * "re-run login", "stop", and "retry". Collapsing them into 1 would make all
   * three indistinguishable.
   */
  it("separates the three responses a caller can actually have", () => {
    expect(exitCodeForStatus(401)).toBe(ExitCode.AUTH);
    expect(exitCodeForStatus(403)).toBe(ExitCode.AUTH);
    expect(exitCodeForStatus(404)).toBe(ExitCode.NOT_FOUND);
    expect(exitCodeForStatus(500)).toBe(ExitCode.UNAVAILABLE);
    expect(exitCodeForStatus(503)).toBe(ExitCode.UNAVAILABLE);
    expect(exitCodeForStatus(400)).toBe(ExitCode.REQUEST);
    expect(exitCodeForStatus(422)).toBe(ExitCode.REQUEST);
  });

  /**
   * A 2xx never reaches this function — the caller checks `status.ok` first —
   * so the value here is only the fallback for "asked about something that is
   * not an error". Pinned so a future refactor that DOES route a 2xx through
   * it fails loudly rather than reporting success as `FAILURE` in silence.
   */
  it("falls back to FAILURE for a status that is not an error at all", () => {
    expect(exitCodeForStatus(200)).toBe(ExitCode.FAILURE);
    expect(exitCodeForStatus(204)).toBe(ExitCode.FAILURE);
  });

  it("keeps every code distinct — a duplicate would erase a branch", () => {
    const codes = Object.values(ExitCode);
    expect(new Set(codes).size).toBe(codes.length);
  });
});

describe("errorForResponse", () => {
  it("puts the response body in `detail`, not folded into the message", () => {
    const err = errorForResponse({
      status: 400,
      statusText: "Bad Request",
      headers: {},
      body: '{\n  "field": "sql",\n  "error": "empty"\n}',
      url: "http://x/api/y",
      fromCache: false
    });
    // The body is often several lines of JSON; folded into a one-line message
    // it is unreadable, and truncated it loses the field name that identifies
    // the problem.
    expect(err.message).not.toContain("field");
    expect(err.detail).toContain('"field": "sql"');
    expect(err.code).toBe(ExitCode.REQUEST);
  });

  it("hints at an assume-role session on a 403, which is the usual cause", () => {
    const err = errorForResponse({
      status: 403,
      statusText: "Forbidden",
      headers: {},
      body: "",
      url: "http://x",
      fromCache: false
    });
    expect(err.hint).toMatch(/assume/);
  });

  /** In an admin surface a 404 can be a scope boundary, not a missing row. */
  it("warns that an admin 404 may be a scope boundary", () => {
    const err = errorForResponse({
      status: 404,
      statusText: "Not Found",
      headers: {},
      body: "",
      url: "http://x",
      fromCache: false
    });
    expect(err.hint).toMatch(/scope boundary/);
  });
});

describe("buildUrl", () => {
  it("joins target and path without doubling the slash", () => {
    expect(buildUrl("http://x", "/api/y")).toBe("http://x/api/y");
    expect(buildUrl("http://x/", "/api/y")).toBe("http://x/api/y");
  });

  /**
   * Concatenation rather than `new URL(path, target)`, which would DISCARD the
   * prefix — and `--target https://host/oxy` is a supported shape for a
   * deployment served under a path.
   */
  it("keeps a path prefix on the target", () => {
    expect(buildUrl("https://host/oxy", "/api/user")).toBe("https://host/oxy/api/user");
  });
});

describe("table", () => {
  it("prints nothing at all for an empty set", () => {
    // A bare header reads as a claim that the set was inspected and found
    // empty, which is only sometimes what happened.
    expect(table([], [{ header: "A", value: () => "" }])).toBe("");
  });

  /**
   * A newline would break a markdown table into a row and some loose prose,
   * and a pipe would open a phantom column — one bad value mangling the whole
   * table rather than one cell.
   */
  it("neutralises characters that would break the table", () => {
    const out = table([{ v: "a|b\nc\td" }], [{ header: "V", value: (r) => r.v }]);
    expect(out).toContain("\\|");
    expect(out.split("\n").filter((l) => l.startsWith("|"))).toHaveLength(3); // header, rule, one row
  });
});

describe("refuseIfAtLimit", () => {
  /**
   * `gh` truncates SILENTLY at its limit, and the guard cannot tell "exactly N,
   * complete" from "truncated at N" — so it refuses both. Not hypothetical: the
   * first real `oxyc activity` refused because that repo had exactly 200 merged
   * PRs and the default was 200.
   */
  it("refuses a result set that came back exactly at the limit", () => {
    expect(() => refuseIfAtLimit(200, 200, "the search")).toThrow(/may be truncated/);
    expect(() => refuseIfAtLimit(201, 200, "the search")).toThrow();
  });

  it("passes anything below the limit", () => {
    expect(() => refuseIfAtLimit(199, 200, "the search")).not.toThrow();
    expect(() => refuseIfAtLimit(0, 200, "the search")).not.toThrow();
  });
});

describe("request — credential selection, over a real socket", () => {
  let server: Server;
  let base: string;

  beforeAll(async () => {
    server = createServer((req, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          path: req.url,
          authorization: req.headers.authorization ?? null,
          apiKey: req.headers["x-api-key"] ?? null,
          contentType: req.headers["content-type"] ?? null,
          accept: req.headers.accept ?? null
        })
      );
    });
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });

  afterAll(() => server.close());

  const send = async (path: string, extra: Record<string, unknown> = {}) => {
    const res = await request({
      target: base,
      path,
      method: "GET",
      bearer: "tok",
      apiKey: "key",
      ...extra
    });
    return JSON.parse(res.body);
  };

  it("sends only the bearer on the /api surface", async () => {
    const got = await send("/api/user");
    expect(got.authorization).toBe("Bearer tok");
    expect(got.apiKey).toBeNull();
  });

  /**
   * The PATH picks the credential, not the caller — which is the whole reason
   * this command exists rather than a `curl` alias.
   */
  it("sends the API key on the /external/api surface", async () => {
    const got = await send("/external/api/w/sql/query");
    expect(got.apiKey).toBe("key");
  });

  it("lets an explicit -H Authorization win over the resolved bearer", async () => {
    const got = await send("/api/user", { headers: { Authorization: "Bearer mine" } });
    expect(got.authorization).toBe("Bearer mine");
  });

  it("defaults a body to JSON, and lets -H content-type override it", async () => {
    expect((await send("/api/x", { method: "POST", body: "{}" })).contentType).toBe(
      "application/json"
    );
    const overridden = await send("/api/x", {
      method: "POST",
      body: "a=1",
      headers: { "content-type": "application/x-www-form-urlencoded" }
    });
    expect(overridden.contentType).toBe("application/x-www-form-urlencoded");
  });

  it("sends no content-type when there is no body", async () => {
    expect((await send("/api/user")).contentType).toBeNull();
  });

  /** A refused connection is "the deployment did not answer" — retryable. */
  it("maps an unreachable target to the retryable code", async () => {
    await expect(
      request({ target: "http://127.0.0.1:1", path: "/api/x", method: "GET", timeoutMs: 2000 })
    ).rejects.toMatchObject({ code: ExitCode.UNAVAILABLE });
  });
});

describe("table headers come from response data, so they are hostile too", () => {
  /**
   * `--md` derives column names from the response — JSON keys, or the header
   * row of a SQL result — so `SELECT 1 AS "a | b"` puts a pipe in a header and
   * splits every row onto the wrong columns. Cells were always sanitized;
   * headers were not, because when the code was written they were only ever
   * literals in our own source.
   */
  it("neutralises a pipe in a header", () => {
    const out = table([{ v: "x" }], [{ header: "a | b", value: (r) => r.v }]);
    const lines = out.split("\n");
    // header, rule, one row — a raw pipe would have made the header 3 columns
    // wide while the rule and the row stayed at 1.
    expect(lines).toHaveLength(3);
    expect(lines[0]?.match(/(?<!\\)\|/g)).toHaveLength(2);
  });

  it("neutralises a newline in a header", () => {
    const out = table([{ v: "x" }], [{ header: "a\nb", value: (r) => r.v }]);
    expect(out.split("\n")).toHaveLength(3);
  });
});
