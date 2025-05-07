import { apiBaseURL } from "@/services/env";
import { DataContainer } from "@/types/app";

export const getArrowValue = (value: unknown): number | string | unknown => {
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
  let currentIndex: number | undefined = undefined;
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

const getNextDataFromArray = (
  array: DataContainer,
  index?: number,
): DataContainer | null => {
  if (!Array.isArray(array) || index === undefined || index >= array.length) {
    return null;
  }
  const value = array[index];
  return value === null || value === undefined ? null : value;
};

const getNextDataFromObject = (
  obj: DataContainer,
  key: string,
): DataContainer | null => {
  if (typeof obj !== "object" || obj === null || !(key in obj)) {
    return null;
  }
  const value = (obj as Record<string, DataContainer>)[key];
  return value === null ? null : value;
};

const getNextData = (
  currentData: DataContainer,
  part: KeyPart,
): DataContainer | null => {
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

export const getDataFileUrl = (file_path: string) => {
  const pathb64 = btoa(file_path);
  return `${apiBaseURL}/app/file/${pathb64}`;
};
