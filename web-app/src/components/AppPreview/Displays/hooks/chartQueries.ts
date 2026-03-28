import type { AsyncDuckDBConnection } from "@duckdb/duckdb-wasm";
import { getArrowColumnValues } from "../utils";

/** Wraps an identifier in double quotes, escaping any embedded double quotes. */
const q = (identifier: string) => `"${identifier.replace(/"/g, '""')}"`;

export const getXAxisData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  xField: string
): Promise<(string | number)[]> => {
  const xData = await connection.query(
    `SELECT DISTINCT ${q(xField)} as x FROM "${fileName}" ORDER BY ${q(xField)}`
  );
  return getArrowColumnValues(xData, "x") as (string | number)[];
};

export const getSeriesData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  seriesField: string
): Promise<unknown[]> => {
  const seriesStmt = await connection.prepare(
    `SELECT DISTINCT ${q(seriesField)} as series FROM "${fileName}"`
  );
  const series = await seriesStmt.query();
  return getArrowColumnValues(series, "series");
};

export const getSeriesValues = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  xField: string,
  yField: string,
  seriesField: string,
  seriesValue: unknown
): Promise<{ x: number | string; y: number | string }[]> => {
  const seriesDataStatement = await connection.prepare(
    `SELECT ${q(xField)} as x, SUM(${q(yField)}) as y FROM "${fileName}"
     WHERE ${q(seriesField)} = ?
     GROUP BY ${q(xField)}, ${q(seriesField)}
     ORDER BY ${q(xField)}`
  );
  const result = await seriesDataStatement.query(seriesValue);
  const xValues = getArrowColumnValues(result, "x");
  const yValues = getArrowColumnValues(result, "y");
  return xValues.map((x: unknown, index: number) => ({
    x: x as number | string,
    y: yValues[index] as number | string
  }));
};

export const getSimpleAggregatedData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  xField: string,
  yField: string
): Promise<(string | number)[]> => {
  const yData = await connection.query(
    `SELECT ${q(xField)} as x, SUM(${q(yField)}) as y FROM "${fileName}"
     GROUP BY ${q(xField)}
     ORDER BY ${q(xField)}`
  );
  return getArrowColumnValues(yData, "y") as (string | number)[];
};

export const getPieChartData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  nameField: string,
  valueField: string
): Promise<{ name: string; value: number }[]> => {
  const pieData = await connection.query(
    `SELECT ${q(nameField)} as name, SUM(${q(valueField)}) as value
     FROM "${fileName}"
     GROUP BY ${q(nameField)}`
  );
  const nameValues = getArrowColumnValues(pieData, "name");
  const valueValues = getArrowColumnValues(pieData, "value");
  return nameValues.map((name: unknown, index: number) => ({
    name: name as string,
    value: valueValues[index] as number
  }));
};
