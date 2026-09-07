import { describe, expect, it } from "vitest";
import { newTraceparent, withInvocationIds } from "./traceparent";

describe("newTraceparent", () => {
  it("mints a sampled W3C header whose trace id the caller keeps", () => {
    const tp = newTraceparent();
    expect(tp.header).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-01$/);
    expect(tp.header.split("-")[1]).toBe(tp.traceId);
  });

  it("does not repeat", () => {
    const ids = new Set(Array.from({ length: 50 }, () => newTraceparent().traceId));
    expect(ids.size).toBe(50);
  });
});

describe("withInvocationIds", () => {
  it("stamps trace and request ids on an error without overwriting existing ones", () => {
    const err = new Error("boom") as Error & { traceId?: string; requestId?: string };
    withInvocationIds(err, "a".repeat(32), "req-1");
    expect(err.traceId).toBe("a".repeat(32));
    expect(err.requestId).toBe("req-1");
    withInvocationIds(err, "b".repeat(32), "req-2");
    expect(err.traceId).toBe("a".repeat(32));
    expect(err.requestId).toBe("req-1");
  });

  it("leaves a missing request id absent and non-objects untouched", () => {
    const err = withInvocationIds(
      {} as { traceId?: string; requestId?: string },
      "c".repeat(32),
      null
    );
    expect(err.traceId).toBe("c".repeat(32));
    expect("requestId" in err).toBe(false);
    expect(withInvocationIds("nope", "d".repeat(32))).toBe("nope");
  });
});
