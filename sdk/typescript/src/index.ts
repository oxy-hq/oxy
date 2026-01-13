export { OxyClient } from "./client";
export { OxySDK } from "./sdk";
export { createConfig, createConfigAsync } from "./config";
export type { OxyConfig } from "./config";

// React integration
export { OxyProvider, useOxy, useOxySDK } from "./react";
export type { OxyProviderProps, OxyContextValue } from "./react";

// Parquet reading utilities
export {
  ParquetReader,
  queryParquet,
  readParquet,
  initializeDuckDB,
} from "./parquet";
export type { QueryResult } from "./parquet";

// PostMessage authentication utilities
export { isInIframe, requestAuthFromParent } from "./auth/postMessage";

// Type exports
export type {
  AppItem,
  AppDataResponse,
  DataContainer,
  DisplayData,
  DisplayWithError,
  FileReference,
  TableData,
  // PostMessage auth types
  OxyAuthRequestMessage,
  OxyAuthResponseMessage,
  PostMessageAuthOptions,
  PostMessageAuthResult,
} from "./types";

// Error class exports
export {
  PostMessageAuthTimeoutError,
  PostMessageAuthInvalidOriginError,
  PostMessageAuthNotInIframeError,
  PostMessageAuthInvalidResponseError,
} from "./types";
