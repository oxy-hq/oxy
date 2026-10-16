/**
 * `notification-catalog.ts` beside this is a byte copy of the SDK's
 * `src/custom-app/notification-catalog.ts`: the one source of the notification
 * catalogue's rules, which `oxyc validate`, `oxyc publish`, the Vite plugin and
 * `@oxy-hq/sdk/testing` all run. The rules' own cases live with the source;
 * this pins the copy to it. Only this test reads outside the package — the
 * published bundle carries the copy.
 */

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const SOURCE = new URL(
  "../../../typescript/src/custom-app/notification-catalog.ts",
  import.meta.url
);
const COPY = new URL("./notification-catalog.ts", import.meta.url);
const SYNC = "run `node sdk/typescript/scripts/sync-notification-catalog.mjs` from the repo root";

describe("the notification catalogue's checks", () => {
  it("are a byte copy of the SDK's", () => {
    expect(readFileSync(COPY).equals(readFileSync(SOURCE)), `stale copy: ${SYNC}`).toBe(true);
  });
});
