// Customer-app bundle helpers — manifest loading + query execution.
//
// Customer apps are static bundles served by oxy at
// `app.oxygen-hq.com/customer-apps/<org_slug>/<app_slug>/`. They commit a
// `public/oxy-app.json` declaring their identity (slug, orgSlug,
// projectId), then call `useQuery` inside React components to run SQL
// queries against the linked project.
//
// Minimal usage:
//
// ```ts
// import { OxyAppProvider, useQuery } from "@oxy-hq/sdk";
//
// function App() {
//   return <OxyAppProvider><Dashboard /></OxyAppProvider>;
// }
// function Dashboard() {
//   const { rows, columns, loading, error } = useQuery({ sql: "SELECT * FROM orders LIMIT 10" });
//   ...
// }
// ```
//
// See `crates/app/src/server/api/custom_apps.rs` for the server-side
// query contract.

export { base64ToBytes, bytesToBase64 } from "./base64";
export type { CustomAppDebugSnapshot } from "./debug";
export { getCustomAppDebug } from "./debug";
export type { CustomAppErrorReport } from "./errors";
export { apiErrorFromResponse, interpretCustomAppError, OxyApiError } from "./errors";
export type {
  EmailAttachment,
  EmailSendInput,
  EmailSendResult,
  OxyAirwayApi,
  OxyEmailApi,
  OxyFetchResult,
  OxyFunctionContext,
  OxyFunctionHandler,
  OxyFunctionRequest,
  OxyFunctionRow,
  OxyFunctionUser,
  OxySecretsApi,
  OxySemanticApi,
  OxyStorageApi,
  OxyWarehouseApi,
  StorageDownloadUrl,
  StorageListPage,
  StorageObject,
  StoragePutOptions,
  StoragePutResult,
  StorageUploadUrl,
  StorageUploadUrlInput
} from "./function-context";
export type { OxyInjectedAppConfig } from "./inject";
export { readInjectedAppConfig } from "./inject";
export type { OxyAppLogger, OxyAppLogLevel } from "./logger";
export { getOxyAppLogger, setOxyAppLogger } from "./logger";
export type {
  LoadManifestOptions,
  OxyAppFunctionManifest,
  OxyAppManifest,
  ResolvedCustomAppManifest
} from "./manifest";
export {
  _resetCustomAppManifestCacheForTest,
  loadCustomAppManifest
} from "./manifest";
// Metric-tree analysis hooks (drivers / what-if / RCA / opportunity sizing)
export type { MetricTreeHookResult, UseMetricTreeOpts } from "./metric-tree-hooks";
export {
  useDistribution,
  useExplain,
  useMetricTree,
  useOpportunity,
  usePredict,
  useSensitivity,
  useTimeDimensions
} from "./metric-tree-hooks";
export type {
  AgentArtifact,
  AgentRunEvent,
  AgentRunState,
  AgentSqlArtifact,
  AppFetcher,
  OxyAnswerProps,
  OxyAppProviderProps,
  OxyChatProps,
  ProcedureProgress,
  ProcedureResult,
  ProcedureRunState,
  SemanticArrayOp,
  SemanticDateRangeOp,
  SemanticFilter,
  SemanticScalarOp,
  SemanticTimeDimension,
  UseAgentRunInput,
  UseAgentRunResult,
  UseFunctionResult,
  UseProcedureRunInput,
  UseProcedureRunOpts,
  UseProcedureRunResult,
  UseQueryInput,
  UseQueryOpts,
  UseQueryResult,
  UseSemanticQueryInput,
  UseSemanticQueryOpts,
  UseSemanticQueryResult
} from "./react";
// React provider + hooks
export {
  OxyAnswer,
  OxyAppProvider,
  OxyChat,
  useAgentRun,
  useFunction,
  useOxyApp,
  useProcedureRun,
  useQuery,
  useResolvedManifest,
  useSemanticQuery,
  useTrackEvent
} from "./react";
// World-model hooks (graph / instances / driver-tree)
export { readJsonSseStream } from "./sse";
export type {
  UseMeasureBreakdownResult,
  UseWorldModelGraphResult,
  UseWorldModelInstancesOpts,
  UseWorldModelInstancesResult
} from "./world-model-hooks";
export {
  useMeasureBreakdown,
  useWorldModelGraph,
  useWorldModelInstances
} from "./world-model-hooks";
// World Model node interface (the `expand` / `drill` / `explain` / `size` paradigm)
export type {
  ExpandedNode,
  ExplainOpts,
  MetricHandle,
  MetricScope,
  SizeOpts,
  WorldModelApi
} from "./world-node";
export { createWorldModel, useWorldModel, WorldModelScopeUnsupportedError } from "./world-node";
