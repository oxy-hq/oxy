/**
 * The path matcher behind `oxyc schema`.
 *
 * Every case here is one the live OpenAPI document actually produced — the
 * first implementation matched nothing at all against it.
 */

import { describe, expect, it } from "vitest";
import { comparablePath, literalSegments } from "./discover.js";

describe("comparablePath", () => {
  /**
   * OpenAPI carries the `/api` prefix in `servers`, so the document says
   * `/{workspace_id}/agents` while the CLI and its callers say
   * `/api/{workspace_id}/agents`. Normalising the request path UP to `/api/…`
   * guaranteed a miss on every single lookup.
   */
  it("drops the /api prefix from either side", () => {
    expect(comparablePath("/api/{workspace_id}/agents")).toBe(
      comparablePath("/{workspace_id}/agents")
    );
    expect(comparablePath("api/health")).toBe(comparablePath("/health"));
  });

  /**
   * The document says `{workspace_id}`; this CLI's own placeholder is
   * `{workspace}`, and `oxyc schema {workspace}/agents` is exactly what
   * somebody copies across from `oxyc api`.
   */
  it("treats differently-named placeholders as equal", () => {
    expect(comparablePath("{workspace}/agents")).toBe(comparablePath("/{workspace_id}/agents"));
    expect(comparablePath("/a/{id}/b/{other}")).toBe(comparablePath("/a/{x}/b/{y}"));
  });

  it("does not confuse two paths that differ outside the braces", () => {
    expect(comparablePath("/{w}/agents")).not.toBe(comparablePath("/{w}/apps"));
  });

  it("is insensitive to a leading slash and a trailing one", () => {
    expect(comparablePath("health")).toBe(comparablePath("/health/"));
  });

  /** `/apidoc` starts with `/api` but is not under it. */
  it("only strips /api as a whole segment", () => {
    expect(comparablePath("/apidoc/openapi.json")).toBe("/apidoc/openapi.json");
  });
});

describe("literalSegments", () => {
  it("keeps only the parts a fuzzy match should look at", () => {
    expect(literalSegments("/api/{workspace_id}/agents")).toBe("//agents");
    expect(literalSegments("/{w}/agents")).toContain("agents");
  });

  it("lets a bare word find the endpoints that contain it", () => {
    expect(
      literalSegments("/{workspace_id}/api-keys/{id}").includes(literalSegments("api-keys"))
    ).toBe(true);
  });
});
