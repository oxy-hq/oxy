// The weekly digest (r3): the top five stores, kept at one fixed storage key.
// It emails only when the body says `{ "send": true }` — a button click. A
// scheduled fire has an empty body, so the Monday schedule never sends mail
// (on a dev box every app email opens a browser tab).
const KEY = "digests/latest.json";

const SQL = `SELECT "Store" AS store, ROUND(SUM("Weekly_Sales") / 1e6, 2) AS sales_musd
FROM oxymart
GROUP BY 1
ORDER BY 2 DESC
LIMIT 5`;

function parse(req) {
  try {
    return JSON.parse(req.body || "{}");
  } catch {
    return {};
  }
}

export default async (req, ctx) => {
  const body = parse(req);
  const { rows } = await ctx.query(SQL);
  const lines = rows.map((r) => `Store ${r.store}: $${r.sales_musd}M`);
  await ctx.storage.put(KEY, JSON.stringify({ at: new Date().toISOString(), lines }), {
    contentType: "application/json",
    allowOverwrite: true
  });
  if (body.send !== true) return Response.json({ sent: false, lines });
  const to = ctx.user && ctx.user.email;
  if (!to) return Response.json({ error: "the caller has no email address" }, { status: 400 });
  const { messageId } = await ctx.email.send({
    to,
    subject: "Pulse weekly digest",
    text: lines.join("\n")
  });
  return Response.json({ sent: true, messageId, lines });
};
