import {
  SemanticQueryRequest,
  SemanticQueryFilter,
  SemanticQueryOrder,
} from "@/services/api/semantic";
import { Variable } from "../components/SemanticQueryPanel";

interface BuildSemanticQueryOptions {
  topic?: string;
  dimensions: string[];
  measures: string[];
  filters: SemanticQueryFilter[];
  orders: SemanticQueryOrder[];
  variables: Variable[];
}

/**
 * Builds a SemanticQueryRequest from the provided options.
 * Handles filter validation and variable transformation.
 */
export function buildSemanticQuery({
  topic,
  dimensions,
  measures,
  filters,
  orders,
  variables,
}: BuildSemanticQueryOptions): SemanticQueryRequest {
  const processedFilters = filters
    .filter((f) => {
      if ("value" in f) return f.value !== null && f.value !== "";
      if ("values" in f) return f.values && f.values.length > 0;
      if ("from" in f && "to" in f) return f.from && f.to;
      return false;
    })
    .map((f) => {
      const field = f.field;

      if ("values" in f) {
        return { field, op: f.op, values: f.values };
      } else if ("from" in f && "to" in f) {
        return { field, op: f.op, from: f.from, to: f.to };
      } else {
        return { field, op: f.op, value: f.value };
      }
    });

  const processedVariables = variables.reduce(
    (acc, v) => {
      if (v.key) acc[v.key] = v.value;
      return acc;
    },
    {} as Record<string, unknown>,
  );

  const processedOrders = orders.map((order) => ({
    field: order.field,
    direction: order.direction,
  }));

  return {
    query: {
      ...(topic && { topic }),
      dimensions: dimensions,
      measures: measures,
      filters: processedFilters,
      orders: processedOrders,
      variables: processedVariables,
    },
    result_format: "parquet",
  };
}
