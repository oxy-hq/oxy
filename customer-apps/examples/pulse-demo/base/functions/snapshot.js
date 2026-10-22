// Every 15 minutes: the leaderboard, written to ONE fixed key. A key per run
// would grow the app's storage silo by 96 objects a day on every dev box.
const KEY = "snapshots/latest.json";

const SQL = `SELECT "Store" AS store, ROUND(SUM("Weekly_Sales") / 1e6, 2) AS sales_musd
FROM oxymart
GROUP BY 1
ORDER BY 2 DESC
LIMIT 10`;

export default async (req, ctx) => {
  const { rows } = await ctx.query(SQL);
  const snapshot = { takenAt: new Date().toISOString(), stores: rows };
  await ctx.storage.put(KEY, JSON.stringify(snapshot), {
    contentType: "application/json",
    allowOverwrite: true
  });
  ctx.log("snapshot: wrote", rows.length, "stores to", KEY);
  return Response.json({ key: KEY, stores: rows.length });
};
