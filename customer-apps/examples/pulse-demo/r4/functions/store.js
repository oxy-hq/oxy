// One store's summary (r4) — THE BAD RELEASE, on purpose.
//
// r4 adds the store's rank in the latest snapshot and a holiday/regular trend:
// the summary and the snapshot are read together, then the trend. Two defects
// ship with it, and they are what the release timeline is there to catch:
//
//   - the "no such store" guard was dropped, so an unknown id (a stale link,
//     a closed store) throws a TypeError instead of answering 404;
//   - the trend is a second, sequential query, so every call is slower.
//
// r5 is the fix. Do not "fix" this file: the demo needs it broken.
const SNAPSHOT = "snapshots/latest.json";
const LIVE_URL = "https://example.com/";

const storeSql = (id) => `SELECT "Store" AS store, COUNT(*) AS weeks,
 ROUND(SUM("Weekly_Sales") / 1e6, 2) AS sales_musd,
 ROUND(AVG("Weekly_Sales")) AS avg_weekly_sales
FROM oxymart
WHERE "Store" = ${id}
GROUP BY 1`;

const trendSql = (id) => `SELECT CASE WHEN "Holiday_Flag" = 1 THEN 'holiday' ELSE 'regular' END AS week_kind,
 COUNT(*) AS weeks,
 ROUND(AVG("Weekly_Sales")) AS avg_weekly_sales
FROM oxymart
WHERE "Store" = ${id}
GROUP BY 1
ORDER BY 1`;

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

/** The latest snapshot, or null. Storage keys are the app's full keys, so the
 *  object is found by listing its folder (a prefix naming the file itself is
 *  refused by the filesystem store a dev box uses), then read by that key. */
async function latestSnapshot(ctx) {
  const page = await ctx.storage.list({ prefix: "snapshots/", limit: 10 });
  const found = page.objects.find((o) => o.key.endsWith(`/${SNAPSHOT}`));
  return found ? ctx.storage.get(found.key) : null;
}

function rankIn(snapshot, store) {
  if (!snapshot) return null;
  const stores = JSON.parse(snapshot.body).stores || [];
  const at = stores.findIndex((s) => Number(s.store) === Number(store));
  return at < 0 ? null : at + 1;
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
  const [summary, snapshot] = await Promise.all([
    ctx.query(storeSql(id)),
    latestSnapshot(ctx)
  ]);
  const trend = await ctx.query(trendSql(id));
  const store = summary.rows[0];
  const rank = rankIn(snapshot, store.store);
  const live = body.live === true ? await liveCheck(ctx) : null;
  return Response.json({ store, rank, trend: trend.rows, live });
};
