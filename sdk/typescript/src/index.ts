// `@oxy-hq/sdk` public surface.
//
// Customer-app bundle helpers only — the legacy `OxyClient` / `OxySDK` /
// iframe-postMessage stack was removed in v2 once the platform
// consolidated on the `/api/projects/:id/query` proxy. Bundles use
// `OxyAppProvider` + hooks (`useQuery`, `useSemanticQuery`,
// `useAgentRun`, `useProcedureRun`) plus the drop-in components
// (`OxyChat`, `OxyAnswer`) exported below.
export * from "./customer-app";
