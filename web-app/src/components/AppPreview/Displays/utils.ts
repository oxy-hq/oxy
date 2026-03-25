import {
  DataType,
  type Schema,
  Struct,
  type Table,
  type Timestamp,
  type TypeMap
} from "apache-arrow";
import dayjs from "dayjs";
import timezone from "dayjs/plugin/timezone";
import utc from "dayjs/plugin/utc";

dayjs.extend(utc);
dayjs.extend(timezone);

import { getDuckDB } from "@/libs/duckdb";
import { encodeBase64 } from "@/libs/encoding";
import { apiClient } from "@/services/api/axios";
import type { DataContainer } from "@/types/app";

const getArrowValue = (value: unknown): number | string | unknown => {
  if (value instanceof Uint32Array) return formatNumber(value[0]);
  if (value instanceof Float32Array) return formatNumber(value[0]);
  if (value instanceof Float64Array) return formatNumber(value[0]);
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (typeof value === "number") {
    return formatNumber(value);
  }
  return value;
};

export const getArrowColumnValues = (table: Table<TypeMap>, columnName: string) => {
  const fieldType = getArrowFieldType(columnName, table.schema);
  return table.toArray().map((row: unknown) => {
    const value = (row as Record<string, unknown>)[columnName];
    if (!fieldType) {
      return getArrowValue(value);
    }
    return getArrowValueWithType(value, fieldType);
  });
};

export const getArrowValueWithType = (
  value: unknown,
  type: DataType
): number | string | unknown => {
  if (DataType.isDate(type)) {
    return formatDate(value as number);
  }
  if (DataType.isTimestamp(type)) {
    return formatDateTime(value as number, (type as Timestamp)?.timezone);
  }
  if (DataType.isTime(type)) {
    return formatTime(value as number);
  }
  // in the BE we are using snowflake-rs library which doesn't return field metadata
  // so there is no way to know if a field is snowflake timestamp or not
  // except checking the structure of the value itself
  if (isSnowflakeTimestamp(value, type)) {
    return formatSnowflakeTimestamp(value as { epoch: number; fraction: number });
  }
  if (DataType.isDecimal(type)) {
    const scale = (type as { scale: number }).scale;
    const numValue = Number(value) / 10 ** scale;
    return formatNumber(numValue);
  }
  return getArrowValue(value);
};

function isSnowflakeTimestamp(value: unknown, type: DataType): boolean {
  return (
    Struct.isStruct(type) &&
    typeof value === "object" &&
    value !== null &&
    "epoch" in value &&
    "fraction" in value
  );
}

function formatSnowflakeTimestamp(value: {
  epoch: number | bigint;
  fraction: number | bigint;
}): string {
  const epoch = typeof value.epoch === "bigint" ? Number(value.epoch) : value.epoch;
  const fraction = typeof value.fraction === "bigint" ? Number(value.fraction) : value.fraction;
  const milliseconds = epoch * 1000 + Math.floor(fraction / 1_000_000);
  return dayjs.utc(milliseconds).format("YYYY-MM-DD HH:mm");
}

function formatDate(value: number | string): string {
  return dayjs.utc(value).format("YYYY-MM-DD");
}

function formatDateTime(value: number | string, tz?: string | null): string {
  if (tz) return dayjs(value).tz(tz).format("YYYY-MM-DD HH:mm");
  return dayjs.utc(value).format("YYYY-MM-DD HH:mm");
}

function formatTime(value: number | bigint | string): string {
  if (typeof value === "bigint") {
    // DuckDB returns TIME as BigInt microseconds since midnight
    const totalSeconds = Number(value / 1000000n);
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;
    return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return dayjs.utc(value).format("HH:mm:ss");
}

function formatNumber(num: number) {
  return num % 1 === 0 ? num.toString() : num.toFixed(2);
}

type KeyPart = {
  key: string;
  index?: number;
};

const getKeyParts = (key: string) => {
  const parts: KeyPart[] = [];
  let currentPart = "";
  let currentIndex: number | undefined;
  for (let i = 0; i < key.length; i++) {
    const char = key[i];
    if (char === ".") {
      if (currentPart) {
        parts.push({ key: currentPart, index: currentIndex });
        currentPart = "";
        currentIndex = undefined;
      }
    } else if (char === "[") {
      if (currentPart) {
        parts.push({ key: currentPart });
        currentPart = "";
      }
      const endIndex = key.indexOf("]", i);
      if (endIndex !== -1) {
        currentIndex = parseInt(key.slice(i + 1, endIndex), 10);
        // eslint-disable-next-line sonarjs/updated-loop-counter
        i = endIndex;
      }
    } else {
      currentPart += char;
    }
  }
  if (currentPart) {
    parts.push({ key: currentPart, index: currentIndex });
  }
  return parts;
};

const getNextDataFromArray = (array: DataContainer, index?: number): DataContainer | null => {
  if (!Array.isArray(array) || index === undefined || index >= array.length) {
    return null;
  }
  const value = array[index];
  return value === null || value === undefined ? null : value;
};

const getNextDataFromObject = (obj: DataContainer, key: string): DataContainer | null => {
  if (typeof obj !== "object" || obj === null || !(key in obj)) {
    return null;
  }
  const value = (obj as Record<string, DataContainer>)[key];
  return value === null ? null : value;
};

const getNextData = (currentData: DataContainer, part: KeyPart): DataContainer | null => {
  if (currentData === null || currentData === undefined) {
    return null;
  }

  let nextData: DataContainer = currentData;

  if (Array.isArray(nextData)) {
    nextData = getNextDataFromArray(nextData, part.index);
    if (nextData === null) {
      return null;
    }
  }

  if (typeof nextData === "object" && nextData !== null) {
    nextData = getNextDataFromObject(nextData, part.key);
    if (nextData === null) {
      return null;
    }
  } else if (typeof nextData !== "object") {
    return null;
  }

  return nextData ?? null;
};

export const getData = (data: DataContainer, key: string) => {
  if (isFilePath(key)) {
    return {
      file_path: key
    };
  }
  const parts = getKeyParts(key);
  let currentData: DataContainer = data;
  for (const part of parts) {
    currentData = getNextData(currentData, part);
    if (currentData === null) {
      return null;
    }
  }
  return currentData;
};

const isFilePath = (key: string) => {
  return key.endsWith(".parquet") || key.endsWith(".csv") || key.endsWith(".json");
};

export const registerAuthenticatedFile = async (
  filePath: string,
  projectId: string,
  branchName: string
): Promise<string> => {
  const db = await getDuckDB();
  const file_name = `${encodeBase64(filePath)}.parquet`;

  const pathb64 = encodeBase64(filePath);
  const response = await apiClient.get(`/${projectId}/apps/file/${pathb64}`, {
    responseType: "arraybuffer",
    params: { branch: branchName }
  });
  const fileData = new Uint8Array(response.data);

  await db.registerFileBuffer(file_name, fileData);

  return file_name;
};

/**
 * Register table data in DuckDB, preferring inline JSON over a network download.
 *
 * When the server embeds query results as `json` in the TableData (which it
 * always does for parameterized runs), we load that JSON directly into DuckDB's
 * virtual filesystem and expose it as a VIEW — zero extra HTTP round-trips.
 * When `json` is absent we fall back to the normal parquet download.
 *
 * Returns a name that can be used directly in `FROM "name"` queries.
 */
export const registerFromTableData = async (
  tableData: { file_path: string; json?: string | null },
  projectId: string,
  branchName: string
): Promise<string> => {
  const db = await getDuckDB();

  if (tableData.json) {
    // Stable name derived from the file path so the same data isn't re-registered
    const safeId = encodeBase64(tableData.file_path).replace(/[^a-zA-Z0-9]/g, "_");
    const jsonFileName = `${safeId}.json`;
    const viewName = safeId;

    await db.registerFileText(jsonFileName, tableData.json);
    const conn = await db.connect();
    try {
      await conn.query(
        `CREATE OR REPLACE VIEW "${viewName}" AS SELECT * FROM read_json_auto('${jsonFileName}')`
      );
    } finally {
      await conn.close();
    }
    return viewName;
  }

  return registerAuthenticatedFile(tableData.file_path, projectId, branchName);
};

/**
 * Download a project source file as Parquet from the server and register it in
 * DuckDB WASM under a `.parquet`-suffixed name.
 *
 * Returns { original: 'oxymart.csv', registered: 'oxymart.parquet' } so the
 * caller can rewrite SQL references before running in DuckDB WASM.
 *
 * Subsequent calls for the same path are no-ops and return the cached mapping.
 */
const registeredSourceFiles = new Map<string, string>();

export const registerSourceFile = async (
  filePath: string,
  projectId: string,
  branchName: string
): Promise<{ original: string; registered: string }> => {
  const safeId = encodeBase64(filePath).replace(/[^a-zA-Z0-9]/g, "_");
  const parquetName = `${safeId}.parquet`;

  const cacheKey = `${projectId}:${branchName}:${filePath}`;
  const cached = registeredSourceFiles.get(cacheKey);
  if (cached) {
    return { original: filePath, registered: cached };
  }

  const db = await getDuckDB();
  const pathb64 = encodeBase64(filePath);
  const response = await apiClient.get(`/${projectId}/apps/source/${pathb64}`, {
    responseType: "arraybuffer",
    params: { branch: branchName }
  });
  const data = new Uint8Array(response.data);
  // Server returns Parquet bytes — register under the .parquet name.
  await db.registerFileBuffer(parquetName, data);
  registeredSourceFiles.set(cacheKey, parquetName);
  return { original: filePath, registered: parquetName };
};

/**
 * Minimal Jinja-compatible renderer for SQL templates.
 * Minimal client-side Jinja renderer for app task SQL.
 *
 * Supported patterns (only these are handled — anything else is unsupported):
 *   {% if controls.x %}...{% endif %}     — conditional block (truthy check only)
 *   {{ controls.x | default('v') }}       — substitution with string fallback
 *   {{ controls.x }}                      — raw value substitution
 *
 * Single quotes inside substituted values are escaped to prevent SQL injection.
 *
 * If any Jinja tokens remain after rendering ({% ... %} or {{ ... }}), the
 * template uses unsupported syntax. This function throws in that case so the
 * caller can fall back to the server rather than silently producing wrong SQL.
 */
export function renderJinja(template: string, controls: Record<string, unknown>): string {
  let result = template;

  // {% if controls.x %}...{% endif %}
  result = result.replace(
    /\{%-?\s*if\s+controls\.(\w+)\s*-?%\}([\s\S]*?)\{%-?\s*endif\s*-?%\}/g,
    (_, name: string, body: string) => (controls[name] ? body : "")
  );

  // {{ controls.x | sqlquote }} — wraps value in single quotes with internal quotes escaped
  result = result.replace(
    /\{\{-?\s*controls\.(\w+)\s*\|\s*sqlquote\s*-?\}\}/g,
    (_, name: string) => `'${String(controls[name] ?? "").replace(/'/g, "''")}'`
  );

  // {{ controls.x | default('fallback') }}
  result = result.replace(
    /\{\{-?\s*controls\.(\w+)\s*\|\s*default\(['"]([^'"]*)['"]\)\s*-?\}\}/g,
    (_, name: string, fallback: string) => String(controls[name] ?? fallback).replace(/'/g, "''")
  );

  // {{ controls.x }}
  result = result.replace(/\{\{-?\s*controls\.(\w+)\s*-?\}\}/g, (_, name: string) =>
    String(controls[name] ?? "").replace(/'/g, "''")
  );

  // Detect any remaining Jinja tokens — unsupported syntax.
  if (/\{[{%]/.test(result)) {
    throw new Error(
      "renderJinja: unsupported Jinja syntax detected after rendering. " +
        "Only {% if %}...{% endif %}, {{ x }}, {{ x | sqlquote }}, and {{ x | default('v') }} are supported client-side."
    );
  }

  return result;
}

/**
 * Run a (Jinja-rendered) SQL query in DuckDB WASM and serialize the result
 * as a JSON string compatible with read_json_auto / registerFromTableData.
 */
export async function runSqlInDuckDB(sql: string): Promise<string> {
  const db = await getDuckDB();
  const conn = await db.connect();
  let result: Awaited<ReturnType<typeof conn.query>>;
  try {
    result = await conn.query(sql);
  } finally {
    await conn.close();
  }

  // Convert Arrow Table to plain JSON array, ensuring all numeric Arrow types
  // become plain JS numbers so JSON.stringify produces numeric literals and
  // read_json_auto infers the correct column type (not VARCHAR).
  const rows = result.toArray().map((row) => {
    const obj: Record<string, unknown> = {};
    for (const field of result.schema.fields) {
      const val = (row as Record<string, unknown>)[field.name];
      if (typeof val === "bigint") {
        // Arrow Int64 / BigInt → Number (values within JS safe-integer range are exact)
        obj[field.name] = Number(val);
      } else if (ArrayBuffer.isView(val) && !(val instanceof DataView)) {
        // TypedArray — DuckDB WASM represents HUGEINT as Uint32Array(4) in Arrow.
        // Extract the scalar from index 0 (lowest 32-bit word, sufficient for typical
        // COUNT/SUM results < 2^32; larger values accept the same precision loss that
        // the existing chart-display path already accepts).
        obj[field.name] = Number((val as unknown as ArrayLike<number | bigint>)[0]);
      } else {
        obj[field.name] = val;
      }
    }
    return obj;
  });

  return JSON.stringify(rows);
}

export const getArrowFieldType = (
  fieldName: string,
  schema: Schema<TypeMap>
): DataType | undefined => {
  return schema.fields.find((f) => (f as { name: string }).name === fieldName)?.type;
};
