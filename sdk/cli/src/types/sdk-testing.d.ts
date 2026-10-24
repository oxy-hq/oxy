/**
 * The slice of `@oxy-hq/sdk/testing` that `oxyc fn dev` uses, declared here.
 *
 * The CLI cannot type-check the SDK's testing SOURCE the way it does
 * `@oxy-hq/sdk/manifest`: this package compiles with `noUncheckedIndexedAccess`
 * and the SDK does not, so mapping the module in `tsconfig.json` `paths` makes
 * `tsc` report ~40 errors in files this package does not own. `tsdown.config.ts`
 * and `vitest.config.ts` still alias the module to the source, so the bundle and
 * the tests run the real `createTestContext`; only the types are restated, and
 * `fn-dev.test.ts` drives the real thing so a drifted shape fails there.
 */
declare module "@oxy-hq/sdk/testing" {
  /** The loose shape a JSON import of `oxy-app.json` satisfies. */
  export interface ManifestLike {
    slug?: string;
    functions?: Record<string, Record<string, unknown> | undefined>;
    notifications?: unknown;
  }

  export interface TestDatabase {
    dialect: "clickhouse" | "postgres" | "duckdb";
    kind: "airhouse" | "airhouse_managed" | "postgres_managed" | "customer";
  }

  export interface TestContextOptions {
    function: string;
    databases?: Record<string, TestDatabase>;
    defaultDatabase?: string;
    env?: Record<string, string>;
  }

  /** One host op the function made. */
  export interface HostCall {
    op: string;
    args: Record<string, unknown>;
    outcome: "ok" | "refused" | "threw";
    message?: string;
    result?: unknown;
  }

  export interface TestContext {
    /** The `OxyFunctionContext` handed to the handler. */
    ctx: unknown;
    readonly calls: readonly HostCall[];
    ops(): string[];
    /** Replace one host op for this context; the call is still recorded. */
    override(op: string, impl: (args: Record<string, unknown>) => Promise<unknown> | unknown): void;
  }

  export function createTestContext(
    manifest: ManifestLike,
    options: TestContextOptions
  ): TestContext;
}
