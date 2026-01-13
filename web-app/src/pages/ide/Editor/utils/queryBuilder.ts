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
  getFullFieldName?: (field: string) => string;
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
  getFullFieldName,
}: BuildSemanticQueryOptions): SemanticQueryRequest {
  const processedFilters = filters
    .filter((f) => {
      if ("value" in f) return f.value !== null && f.value !== "";
      if ("values" in f) return f.values && f.values.length > 0;
      if ("from" in f && "to" in f) return f.from && f.to;
      return false;
    })
    .map((f) => {
      const field = getFullFieldName ? getFullFieldName(f.field) : f.field;

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
    field: getFullFieldName ? getFullFieldName(order.field) : order.field,
    direction: order.direction,
  }));

  return {
    query: {
      ...(topic && { topic }),
      dimensions: dimensions.map((d) =>
        getFullFieldName ? getFullFieldName(d) : d,
      ),
      measures: measures.map((m) =>
        getFullFieldName ? getFullFieldName(m) : m,
      ),
      filters: processedFilters,
      orders: processedOrders,
      variables: processedVariables,
    },
    result_format: "parquet",
  };
}
