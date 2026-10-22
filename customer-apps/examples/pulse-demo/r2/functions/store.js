// One store's summary (r2): r1 plus a "live check". With `{ "live": true }` the
// function also calls a public HTTPS endpoint, and only then: `ctx.fetch` needs
// the internet, and a failed call counts against the invocation even when it
// is caught, so nothing seeded or scheduled sends it.
const LIVE_URL = "https://example.com/";

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

async function liveCheck(ctx) {
  const started = Date.now();
  try {
    const res = await ctx.fetch(LIVE_URL, { method: "GET" });
    return { url: LIVE_URL, status: res.status, ms: Date.now() - started };
  } catch (err) {
    return { url: LIVE_URL, error: String(err.message || err), ms: Date.now() - started };
  }
}

export default async (req, ctx) => {
  const body = parse(req);
  const id = storeId(body);
  if (id === null) {
    return Response.json({ error: "store must be a positive integer" }, { status: 400 });
  }
  const { rows } = await ctx.query(storeSql(id));
  if (!rows.length) return Response.json({ error: `no store ${id}` }, { status: 404 });
  const live = body.live === true ? await liveCheck(ctx) : null;
  return Response.json({ store: rows[0], live });
};
