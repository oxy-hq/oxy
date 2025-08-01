import type { AsyncDuckDBConnection } from "@duckdb/duckdb-wasm";
import { getArrowValue } from "../utils";

export const getXAxisData = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  xField: string,
): Promise<(string | number)[]> => {
  const xData = await connection.query(
    `SELECT DISTINCT ${xField} as x FROM "${fileName}" ORDER BY ${xField}`,
  );
  return xData.toArray().map((row) => getArrowValue(row.x)) as (
    | string
    | number
  )[];
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
  return series.toArray().map((row) => getArrowValue(row.series));
};

export const getSeriesValues = async (
  connection: AsyncDuckDBConnection,
  fileName: string,
  xField: string,
  yField: string,
  seriesField: string,
  seriesValue: unknown,
): Promise<(number | string)[]> => {
  const seriesDataStatement = await connection.prepare(
    `SELECT ${xField} as x, SUM(${yField}) as y FROM "${fileName}" 
     WHERE ${seriesField} = ? 
     GROUP BY ${xField}, ${seriesField}
     ORDER BY ${xField}`,
  );
  const yData = await seriesDataStatement.query(seriesValue);
  return yData.toArray().map((row) => getArrowValue(row.y)) as (
    | number
    | string
  )[];
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
  return yData.toArray().map((row) => getArrowValue(row.y)) as (
    | string
    | number
  )[];
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
  return pieData
    .toArray()
    .map((row) => ({
      name: getArrowValue(row.name),
      value: getArrowValue(row.value),
    }))
    .filter((row) => row.name && row.value) as {
    name: string;
    value: number;
  }[];
};
