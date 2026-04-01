// PostMessage authentication utilities
export { isInIframe, requestAuthFromParent } from "./auth/postMessage";
export { OxyClient } from "./client";
export type { OxyConfig } from "./config";
export { createConfig, createConfigAsync } from "./config";
export type { QueryResult } from "./parquet";
// Parquet reading utilities
export {
  initializeDuckDB,
  ParquetReader,
  queryParquet,
  readParquet
} from "./parquet";
export type { OxyContextValue, OxyProviderProps } from "./react";
// React integration
export { OxyProvider, useOxy, useOxySDK } from "./react";
export { OxySDK } from "./sdk";

// Type exports
export type {
  AppDataResponse,
  AppItem,
  DataContainer,
  DisplayData,
  DisplayWithError,
  FileReference,
  // PostMessage auth types
  OxyAuthRequestMessage,
  OxyAuthResponseMessage,
  PostMessageAuthOptions,
  PostMessageAuthResult,
  TableData
} from "./types";

// Error class exports
export {
  PostMessageAuthInvalidOriginError,
  PostMessageAuthInvalidResponseError,
  PostMessageAuthNotInIframeError,
  PostMessageAuthTimeoutError
} from "./types";
