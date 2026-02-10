import { DataType, type Schema, Struct, type Table, type Timestamp, type Type } from "apache-arrow";
import dayjs from "dayjs";
import timezone from "dayjs/plugin/timezone";
import utc from "dayjs/plugin/utc";

dayjs.extend(utc);
dayjs.extend(timezone);

import { getDuckDB } from "@/libs/duckdb";
import { apiClient } from "@/services/api/axios";
import type { DataContainer } from "@/types/app";

const getArrowValue = (value: unknown): number | string | unknown => {
  if (value instanceof Uint32Array) return formatNumber(value[0]);
  if (value instanceof Float32Array) return formatNumber(value[0]);
  if (value instanceof Float64Array) return formatNumber(value[0]);
  if (typeof value === "bigint") {
    // Add this condition to handle BigInt
    return value.toString(); // Or value.toString() if precision is important
  }
  if (typeof value === "number") {
    return formatNumber(value);
  }
  return value;
};

export const getArrowColumnValues = (
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  table: Table<any>,
  columnName: string
) => {
  const fieldType = getArrowFieldType(columnName, table.schema);
  return table.toArray().map((row: unknown) => {
    const value = (row as Record<string, unknown>)[columnName];
    return getArrowValueWithType(value, fieldType!);
  });
};

export const getArrowValueWithType = (value: unknown, type: Type): number | string | unknown => {
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

function isSnowflakeTimestamp(value: any, type: Type): boolean {
  return (
    Struct.isStruct(type) &&
    value &&
    typeof value === "object" &&
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

function formatTime(value: number | string): string {
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
  const file_name = `${btoa(filePath)}.parquet`;

  const pathb64 = btoa(filePath);
  const response = await apiClient.get(`/${projectId}/app/file/${pathb64}`, {
    responseType: "arraybuffer",
    params: { branch: branchName }
  });
  const fileData = new Uint8Array(response.data);

  await db.registerFileBuffer(file_name, fileData);

  return file_name;
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const getArrowFieldType = (fieldName: string, schema: Schema<any>) => {
  console.log("Getting field type for:", fieldName, schema);
  return schema.fields.find((f: { name: string }) => f.name === fieldName)?.type;
};
