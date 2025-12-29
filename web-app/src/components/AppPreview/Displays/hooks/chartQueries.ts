import type { AsyncDuckDBConnection } from "@duckdb/duckdb-wasm";
import { getArrowColumnValues } from "../utils";

export const getXAxisData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  xField: string,
): Promise<(string | number)[]> => {
  const xData = await connection.query(
    `SELECT DISTINCT ${xField} as x FROM "${fileName}" ORDER BY ${xField}`,
  );
  return getArrowColumnValues(xData, "x") as (string | number)[];
};

export const getSeriesData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  seriesField: string,
): Promise<unknown[]> => {
  const seriesStmt = await connection.prepare(
    `SELECT DISTINCT ${seriesField} as series FROM "${fileName}"`,
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
  seriesValue: unknown,
): Promise<{ x: number | string; y: number | string }[]> => {
  const seriesDataStatement = await connection.prepare(
    `SELECT ${xField} as x, SUM(${yField}) as y FROM "${fileName}" 
     WHERE ${seriesField} = ? 
     GROUP BY ${xField}, ${seriesField}
     ORDER BY ${xField}`,
  );
  const result = await seriesDataStatement.query(seriesValue);
  const xValues = getArrowColumnValues(result, "x");
  const yValues = getArrowColumnValues(result, "y");
  return xValues.map((x, index) => ({
    x: x as number | string,
    y: yValues[index] as number | string,
  }));
};

export const getSimpleAggregatedData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  xField: string,
  yField: string,
): Promise<(string | number)[]> => {
  const yData = await connection.query(
    `SELECT ${xField} as x, SUM(${yField}) as y FROM "${fileName}" 
     GROUP BY ${xField}
     ORDER BY ${xField}`,
  );
  return getArrowColumnValues(yData, "y") as (string | number)[];
};

export const getPieChartData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  nameField: string,
  valueField: string,
): Promise<{ name: string; value: number }[]> => {
  const pieData = await connection.query(
    `SELECT ${nameField} as name, SUM(${valueField}) as value 
     FROM "${fileName}" 
     GROUP BY ${nameField}`,
  );
  const nameValues = getArrowColumnValues(pieData, "name");
  const valueValues = getArrowColumnValues(pieData, "value");
  return nameValues.map((name, index) => ({
    name: name as string,
    value: valueValues[index] as number,
  }));
};
