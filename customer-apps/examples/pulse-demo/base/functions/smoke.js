// A check (`"check": true`): read-only, so it is safe in every environment.
// The warehouse answers and the snapshot folder is listable (empty is fine).
const SNAPSHOT = "snapshots/latest.json";

export default async (req, ctx) => {
  const { rows } = await ctx.query("SELECT COUNT(*) AS n FROM oxymart");
  const n = rows.length ? Number(rows[0].n) : 0;
  if (!n) throw new Error("smoke: oxymart is empty");
  const page = await ctx.storage.list({ prefix: "snapshots/", limit: 10 });
  const snapshot = page.objects.some((o) => o.key.endsWith(`/${SNAPSHOT}`));
  return Response.json({ ok: true, rows: n, snapshot });
};
