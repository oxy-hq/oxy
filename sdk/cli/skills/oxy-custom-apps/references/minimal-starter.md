# Minimal starter

Easiest path: `pnpm dlx @oxy-hq/create-oxy-app@latest my-app --template vite`
(`--template functions` adds Oxy Functions).
The scaffolded layout:

```
my-app/
├── oxy-app.json            ← project root, not public/
├── src/
│   ├── App.tsx
│   └── main.tsx
├── package.json
├── vite.config.ts
└── tsconfig.json
```

`oxy-app.json`:
```json
{ "schemaVersion": 2, "slug": "my-app", "orgSlug": "your-org", "name": "My App" }
```

`src/main.tsx`:
```tsx
import { OxyAppProvider } from "@oxy-hq/sdk";
import { createRoot } from "react-dom/client";
import { App } from "./App";

createRoot(document.getElementById("root")!).render(
  <OxyAppProvider><App /></OxyAppProvider>
);
```

`src/App.tsx`:
```tsx
import { useQuery } from "@oxy-hq/sdk";

export function App() {
  const { rows, loading, error } = useQuery({
    sql: "SELECT 1 AS hello",
  });
  if (loading) return <p>loading…</p>;
  if (error) return <p style={{ color: "red" }}>{error.message}</p>;
  return <pre>{JSON.stringify(rows, null, 2)}</pre>;
}
```

`vite.config.ts`:
```ts
import oxyApp from "@oxy-hq/vite-plugin";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), oxyApp()],
});
```

The plugin handles `outDir = "out"`, base-path resolution from the
manifest, dev-server proxy to `/api`, and the `window.__OXY_APP__`
dev shim. Plain `pnpm build` produces a bundle with the correct
asset base baked in. Then publish it:
[SKILL.md → Publishing](../SKILL.md#publishing).
