// {{APP_DISPLAY_NAME}} — scaffolded by `pnpm dlx create-oxy-app`.
//
// Replace the starter SQL below with your real query. The
// {{ params.X | sqlquote }} syntax interpolates params into the SQL on
// the client; oxy gates the request by your project membership.
//
// Auth model: oxy serves this bundle at
// `app.oxygen-hq.com/customer-apps/<org>/<app>/` after a session-cookie +
// org-membership check, so /api/* fetches ride the cookie automatically.

import { useQuery } from "@oxy-hq/sdk";

export default function App() {
  const { rows, loading, error } = useQuery({
    sql: "SELECT 1 AS hello",
  });

  if (loading) {
    return <main className="container">Loading…</main>;
  }
  if (error) {
    return (
      <main className="container">
        <div className="error">
          <div className="title">Query failed</div>
          <pre>{error.message}</pre>
        </div>
      </main>
    );
  }
  return (
    <main className="container">
      <h1>{{APP_DISPLAY_NAME}}</h1>
      <div className="subtle">Scaffolded by `pnpm dlx create-oxy-app`. Edit src/App.tsx to make it yours.</div>
      <div className="section">
        <h2>Result</h2>
        <pre>{JSON.stringify(rows, null, 2)}</pre>
      </div>
    </main>
  );
}
