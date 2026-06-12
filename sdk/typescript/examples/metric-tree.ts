// Example: query a workspace's metric tree from the SDK.
//
// Run with:
//   OXY_API_KEY=… OXY_PROJECT_ID=… pnpm tsx examples/metric-tree.ts

import { OxyClient } from "@oxy-hq/sdk";

async function main() {
  const client = await OxyClient.create({
    baseUrl: process.env.OXY_BASE_URL ?? "https://api.oxy.tech",
    apiKey: process.env.OXY_API_KEY,
    projectId: process.env.OXY_PROJECT_ID,
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
    previous_period: ["2025-08-01", "2025-08-31"],
  });
  console.log(
    `\noperating_profit moved ${explain.target_delta.toFixed(0)} ` +
      `(${(explain.coverage * 100).toFixed(0)}% explained)`
  );

  // 4. Opportunity sizing
  const opp = await client.metricTree.findOpportunities({
    target: "orders.net_revenue",
    time_dimension: "orders.order_date",
    period: ["2025-09-01", "2025-09-30"],
  });
  console.log("\nTop opportunities:");
  for (const dim of opp.dimensions.slice(0, 3)) {
    console.log(`  ${dim.dimension}: +${dim.total_upside.toFixed(0)} (${dim.benchmark_basis})`);
  }

  // 5. Predict downstream impact
  const predict = await client.metricTree.predict([
    { measure: "marketing_spend.total_spend", delta: 10_000 },
  ]);
  console.log("\n+$10k of marketing spend would propagate as:");
  for (const impact of predict.impacts.slice(0, 5)) {
    console.log(
      `  ${impact.measure} ${impact.estimated_delta >= 0 ? "+" : ""}` +
        `${impact.estimated_delta.toFixed(2)} (${impact.confidence})`
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
