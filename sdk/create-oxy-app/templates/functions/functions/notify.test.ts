// A unit test for `notify`, against the host's real gates rather than a
// hand-rolled fake: `createTestContext` reads `oxy-app.json` the way the
// platform does, so a capability the manifest does not grant is refused here
// exactly as it is in production. Run it with `pnpm test`.
//
// `@oxy-hq/sdk/testing` needs Node (vitest's default environment), not the
// browser — see its README section for what it enforces and what it is not.
// The handler is called directly rather than through `t.run(...)`: that
// helper removes the globals the isolate lacks (`Buffer`, `TextEncoder`, …)
// for the duration of the call, and Node's own `Response` — which this
// handler builds its reply with — needs them. Reach for `t.run` on a handler
// that returns plain data.

import { createTestContext } from "@oxy-hq/sdk/testing";
import { describe, expect, it } from "vitest";
import manifest from "../oxy-app.json";
import notify from "./notify";

describe("notify", () => {
  it("emails the invoking user, never an address from the request", async () => {
    const t = createTestContext(manifest, {
      function: "notify",
      user: { email: "ada@example.test" }
    });

    const res = await notify(
      { method: "POST", headers: {}, body: JSON.stringify({ name: "Ada" }) },
      t.ctx
    );

    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ ok: true, sentTo: "ada@example.test" });
    const [sent] = t.callsTo("email.send");
    expect(sent?.outcome).toBe("ok");
    expect(sent?.args).toMatchObject({ to: "ada@example.test", subject: "Welcome!" });
  });

  it("answers 400 for a caller with no mailbox (a frontline worker)", async () => {
    const t = createTestContext(manifest, {
      function: "notify",
      user: { email: null, kind: "frontline" }
    });

    const res = await notify({ method: "POST", headers: {}, body: "{}" }, t.ctx);

    expect(res.status).toBe(400);
    expect(t.callsTo("email.send")).toEqual([]);
  });
});
