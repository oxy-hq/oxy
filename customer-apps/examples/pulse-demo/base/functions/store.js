// One store's summary (r1). The id comes from the caller, so it is parsed as an
// integer and refused otherwise before it reaches SQL; an id with no rows is a
// 404, not an error.
const storeSql = (id) => `SELECT "Store" AS store, COUNT(*) AS weeks,
 ROUND(SUM("Weekly_Sales") / 1e6, 2) AS sales_musd,
 ROUND(AVG("Weekly_Sales")) AS avg_weekly_sales
FROM oxymart
WHERE "Store" = ${id}
GROUP BY 1`;

function storeId(body) {
  const id = Number(body.store);
  return Number.isInteger(id) && id > 0 && id < 100000 ? id : null;
}

function parse(req) {
  try {
    return JSON.parse(req.body || "{}");
  } catch {
    return {};
  }
}

export default async (req, ctx) => {
  const id = storeId(parse(req));
  if (id === null) {
    return Response.json({ error: "store must be a positive integer" }, { status: 400 });
  }
  const { rows } = await ctx.query(storeSql(id));
  if (!rows.length) return Response.json({ error: `no store ${id}` }, { status: 404 });
  return Response.json({ store: rows[0] });
};
