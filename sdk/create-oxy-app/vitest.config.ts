import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // The templates' own tests (`templates/functions/functions/notify.test.ts`)
    // run inside a SCAFFOLDED app, against its installed `@oxy-hq/sdk` — not
    // here, where nothing they import resolves. `scripts/ci/verify-templates.mjs`
    // is what exercises a scaffold.
    exclude: [...configDefaults.exclude, "templates/**"]
  }
});
