// Example: query a workspace's metric tree from the SDK.
//
// Run with:
//   OXY_API_KEY=… OXY_PROJECT_ID=… pnpm tsx examples/metric-tree.ts

import { OxyClient } from "@oxy-hq/sdk";

async function main() {
  const client = await OxyClient.create({
    baseUrl: process.env.OXY_BASE_URL ?? "https://api.oxy.tech",
    apiKey: process.env.OXY_API_KEY,
    projectId: process.env.OXY_PROJECT_ID
  });

  // 1. Walk the tree
  const tree = await client.metricTree.getTree();
  console.log(`Tree: ${tree.nodes.length} measures, ${tree.edges.length} edges`);

  // 2. Rank drivers of a measure
  const target = "orders.net_revenue";
  const sensitivity = await client.metricTree.getSensitivity(target);
  console.log(`\nDrivers of ${target}:`);
  for (const driver of sensitivity.drivers.slice(0, 5)) {
    const coef =
      driver.effective_coefficient !== null && driver.effective_coefficient !== undefined
        ? ` coef=${driver.effective_coefficient.toFixed(3)}`
        : "";
    console.log(`  ${driver.measure} (${driver.direction} ${driver.strength})${coef}`);
  }

  // 3. Period-over-period RCA
  const explain = await client.metricTree.explain({
    target: "financials.operating_profit",
    time_dimension: "financials.month",
    current_period: ["2025-09-01", "2025-09-30"],
    previous_period: ["2025-08-01", "2025-08-31"]
  });
  console.log(
    `\noperating_profit moved ${explain.target_delta.toFixed(0)} ` +
      `(${(explain.coverage * 100).toFixed(0)}% explained)`
  );

  // 4. Opportunity sizing
  const opp = await client.metricTree.findOpportunities({
    target: "orders.net_revenue",
    time_dimension: "orders.order_date",
    period: ["2025-09-01", "2025-09-30"]
  });
  console.log("\nTop opportunities:");
  for (const dim of opp.dimensions.slice(0, 3)) {
    console.log(`  ${dim.dimension}: +${dim.total_upside.toFixed(0)} (${dim.benchmark_basis})`);
  }

  // 5. Scenario forecasting, part one: value the starting point and measure the
  //    coefficients the propagation needs. `predict` is database-free and
  //    cannot fit an undeclared driver edge itself — this is where that comes
  //    from, which is why it is worth a warehouse query.
  const lever = "marketing_spend.total_spend";
  const baseline = await client.metricTree.getBaseline({
    roots: [lever],
    time_dimension: "orders.order_date",
    period: ["2025-09-01", "2025-09-30"]
  });

  // 6. Predict downstream impact. Pass the baseline's values and fitted
  //    coefficients through verbatim, refusals included — without them an edge
  //    declaring no `coefficient:` propagates nothing and its downstream
  //    measures are simply missing from `impacts`.
  const predict = await client.metricTree.predict([{ measure: lever, delta: 10_000 }], {
    values: baseline.values,
    coefficients: baseline.fitted
  });
  console.log("\n+$10k of marketing spend would propagate as:");
  for (const impact of predict.impacts.slice(0, 5)) {
    console.log(
      `  ${impact.measure} ${impact.estimated_delta >= 0 ? "+" : ""}` +
        `${impact.estimated_delta.toFixed(2)} (${impact.confidence})`
    );
  }

  // 7. Scenario forecasting, part two: the time axis. A year of history, not
  //    the baseline's month — the forecaster refuses anything under eight
  //    seasonal cycles, so reusing the scenario window would refuse every
  //    curve. This is the BASELINE curve; the scenario's second curve is
  //    arithmetic over it and the `predict` result above.
  const projection = await client.metricTree.getProjection({
    roots: [lever],
    time_dimension: "orders.order_date",
    period: ["2024-09-01", "2025-08-31"],
    granularity: "day",
    horizon: 30
  });
  console.log(`\nProjection (${projection.forecaster}, ${projection.horizon} buckets):`);
  for (const series of projection.series) {
    if (series.refusal) {
      // A refusal is a result, not a gap — never draw it as a flat line.
      console.log(`  ${series.measure}: no curve — ${series.refusal}`);
      continue;
    }
    const last = series.forecast[series.forecast.length - 1];
    console.log(
      `  ${series.measure}: ${series.history.length} historical buckets, ` +
        `ends at ${last?.point.toFixed(2)} [${last?.lower?.toFixed(2) ?? "—"}, ` +
        `${last?.upper?.toFixed(2) ?? "—"}]`
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
