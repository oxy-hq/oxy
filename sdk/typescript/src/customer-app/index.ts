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
// See `crates/app/src/server/api/customer_apps.rs` for the server-side
// query contract.

export type { CustomerAppDebugSnapshot } from "./debug";
export { getCustomerAppDebug } from "./debug";
export type { CustomerAppErrorReport } from "./errors";
export { apiErrorFromResponse, interpretCustomerAppError, OxyApiError } from "./errors";
export type {
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
  OxyWarehouseApi
} from "./function-context";
export type { OxyInjectedAppConfig } from "./inject";
export { readInjectedAppConfig } from "./inject";
export type { OxyAppLogger, OxyAppLogLevel } from "./logger";
export { getOxyAppLogger, setOxyAppLogger } from "./logger";
export type {
  LoadManifestOptions,
  OxyAppFunctionManifest,
  OxyAppManifest,
  ResolvedCustomerAppManifest
} from "./manifest";
export {
  _resetCustomerAppManifestCacheForTest,
  loadCustomerAppManifest
} from "./manifest";
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
  useProcedureRun,
  useQuery,
  useResolvedManifest,
  useSemanticQuery,
  useTrackEvent
} from "./react";
