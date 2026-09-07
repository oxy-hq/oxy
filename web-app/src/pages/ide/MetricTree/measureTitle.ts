import type { MetricNode } from "@/types/metricTree";

type TitleableNode = Partial<Pick<MetricNode, "measure" | "label" | "description">>;

/**
 * The short name a measure is shown under: its raw name, exactly as written in
 * the `.view.yml`.
 *
 * NOT `node.label`. Airlayer builds that field as `description ?? name`, so a
 * measure with any documentation at all arrives carrying a full sentence —
 * "Total revenue recognised across all completed orders, net of refunds" — in
 * the field a card title reads from. On a 184px node that truncates to noise,
 * and in the definition panel it printed the same sentence twice, once as the
 * heading and once as the description below it.
 *
 * The name is shown verbatim rather than prettified: it is the identifier
 * people write in YAML and search for, so `net_revenue` on screen matches
 * `net_revenue` in the file. The description keeps its one home — the
 * definition panel, plus the hover title on a card.
 */
export function measureTitle(node: TitleableNode): string {
  return node.measure?.trim() || (node.label ?? "");
}

/** The measure's description, but only when it says something the title didn't. */
export function measureDescription(node: TitleableNode): string | null {
  const description = node.description?.trim();
  if (!description) return null;
  return description === measureTitle(node) ? null : description;
}
