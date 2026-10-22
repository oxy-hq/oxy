// The landing screen's leaderboard: the ten stores with the most sales.
//
// Identifiers are double-quoted and aliased to lower case, so the rows have the
// same keys on DuckDB (the demo workspace's `local`) and on Postgres.
const SQL = `SELECT "Store" AS store, ROUND(SUM("Weekly_Sales") / 1e6, 2) AS sales_musd
FROM oxymart
GROUP BY 1
ORDER BY 2 DESC
LIMIT 10`;

export default async (req, ctx) => {
  const { rows, truncated } = await ctx.query(SQL);
  ctx.log("stores: ranked", rows.length, "stores");
  return Response.json({ stores: rows, truncated: Boolean(truncated) });
};
