import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  __clearInflightFunctions,
  functionInvokeKey,
  sharedFunctionInvoke
} from "./function-invoke";

describe("function-invoke", () => {
  beforeEach(() => __clearInflightFunctions());

  it("functionInvokeKey distinguishes name and body", () => {
    expect(functionInvokeKey("f", { a: 1 })).toBe(functionInvokeKey("f", { a: 1 }));
    expect(functionInvokeKey("f", { a: 1 })).not.toBe(functionInvokeKey("g", { a: 1 }));
    expect(functionInvokeKey("f", { a: 1 })).not.toBe(functionInvokeKey("f", { a: 2 }));
    expect(functionInvokeKey("f", undefined)).toBe(functionInvokeKey("f", {}));
  });

  it("shares ONE in-flight request for concurrent identical invokes", async () => {
    const run = vi.fn(() => new Promise<string>((resolve) => setTimeout(() => resolve("ok"), 10)));
    const key = functionInvokeKey("f", { a: 1 });
    const [r1, r2] = await Promise.all([
      sharedFunctionInvoke(key, run),
      sharedFunctionInvoke(key, run)
    ]);
    expect(run).toHaveBeenCalledTimes(1); // deduped
    expect(r1).toBe("ok");
    expect(r2).toBe("ok");
  });

  it("does NOT memoize results — a fresh invoke after settle runs again", async () => {
    const run = vi.fn(async () => "ok");
    const key = functionInvokeKey("f", { a: 1 });
    await sharedFunctionInvoke(key, run);
    await sharedFunctionInvoke(key, run); // in-flight entry already cleared
    expect(run).toHaveBeenCalledTimes(2); // side effects preserved
  });

  it("clears the in-flight entry even when the request rejects", async () => {
    const key = functionInvokeKey("f", { a: 1 });
    await expect(
      sharedFunctionInvoke(key, async () => {
        throw new Error("boom");
      })
    ).rejects.toThrow("boom");
    // A subsequent invoke is not stuck on the failed promise.
    await expect(sharedFunctionInvoke(key, async () => "recovered")).resolves.toBe("recovered");
  });
});
