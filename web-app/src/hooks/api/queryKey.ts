import type { PaginationState } from "@tanstack/react-table";

const agentKeys = {
  all: ["agent"] as const,
  list: (projectId: string, branchName: string) =>
    [...agentKeys.all, "list", projectId, branchName] as const,
  get: (pathb64: string, projectId: string, branchName: string) =>
    [...agentKeys.all, "get", pathb64, projectId, branchName] as const
};

const analyticsKeys = {
  all: ["analytics"] as const,
  runByThread: (projectId: string, threadId: string) =>
    [...analyticsKeys.all, "runByThread", projectId, threadId] as const,
  runsByThread: (projectId: string, threadId: string) =>
    [...analyticsKeys.all, "runsByThread", projectId, threadId] as const
};

const threadKeys = {
  all: ["thread"] as const,
  list: (projectId: string, page?: number, limit?: number) =>
    [...threadKeys.all, "list", projectId, { page, limit }] as const,
  item: (projectId: string, threadId: string) =>
    [...threadKeys.all, projectId, { threadId }] as const,
  messages: (projectId: string, threadId: string) =>
    [...threadKeys.all, "messages", projectId, threadId] as const
};

const traceKeys = {
  all: ["trace"] as const,
  list: (projectId: string, limit?: number, offset?: number, status?: string, duration?: string) =>
    [...traceKeys.all, "list", projectId, { limit, offset, status, duration }] as const,
  item: (projectId: string, traceId: string) => [...traceKeys.all, projectId, { traceId }] as const
};

const agenticWorkflowKeys = {
  all: ["agentic-workflows"] as const,
  files: (projectId: string) => [...agenticWorkflowKeys.all, "files", projectId] as const,
  file: (projectId: string, pathB64: string) =>
    [...agenticWorkflowKeys.all, "file", projectId, pathB64] as const,
  run: (projectId: string, runId: string) =>
    [...agenticWorkflowKeys.all, "run", projectId, runId] as const,
  runsForWorkflow: (projectId: string, workflowRef: string) =>
    [...agenticWorkflowKeys.all, "runs-for-workflow", projectId, workflowRef] as const
};

const airwayKeys = {
  all: ["agentic-airway"] as const,
  run: (projectId: string, runId: string) => [...airwayKeys.all, "run", projectId, runId] as const,
  runsForPipeline: (projectId: string, pipelineRef: string) =>
    [...airwayKeys.all, "runs-for-pipeline", projectId, pipelineRef] as const,
  files: (projectId: string) => [...airwayKeys.all, "files", projectId] as const
};

const scheduleKeys = {
  all: ["agentic-schedules"] as const,
  list: (projectId: string) => [...scheduleKeys.all, "list", projectId] as const,
  item: (projectId: string, id: string) => [...scheduleKeys.all, projectId, { id }] as const
};

const workflowKeys = {
  all: ["workflow"] as const,
  run: (projectId: string, branchName: string) =>
    [...workflowKeys.all, "run", projectId, branchName] as const,
  list: (projectId: string, branchName: string) =>
    [...workflowKeys.all, "list", projectId, branchName] as const,
  get: (projectId: string, branchName: string, relative_path: string) =>
    [...workflowKeys.all, "get", projectId, branchName, relative_path] as const,
  getLogs: (projectId: string, branchName: string, relative_path: string) =>
    [...workflowKeys.all, "getLogs", projectId, branchName, relative_path] as const,
  getRuns: (
    projectId: string,
    branchName: string,
    relative_path: string,
    pagination: PaginationState
  ) => [...workflowKeys.all, "getRuns", projectId, branchName, relative_path, pagination] as const,
  getBlocks: (projectId: string, branchName: string, sourceId: string, runIndex?: number) =>
    [...workflowKeys.all, "getBlocks", projectId, branchName, sourceId, runIndex] as const
};

const chartKeys = {
  all: ["chart"] as const,
  get: (projectId: string, branchName: string, file_path: string) =>
    [...chartKeys.all, "get", projectId, branchName, file_path] as const,
  // For AppPreview displays compiled in DuckDB-WASM. `dataKey` should uniquely
  // identify the source dataset (e.g. taskName + json file_path); avoid passing
  // the full data object — that thrashes the cache across renders.
  fromDisplay: (
    projectId: string,
    branchName: string,
    displayKey: string,
    dataKey: string,
    isDarkMode: boolean
  ) =>
    [
      ...chartKeys.all,
      "fromDisplay",
      projectId,
      branchName,
      displayKey,
      dataKey,
      isDarkMode
    ] as const
};

const fileKeys = {
  all: (projectId: string, branchName: string) => ["all", projectId, branchName],
  get: (projectId: string, branchName: string, pathb64: string) =>
    [...fileKeys.all(projectId, branchName), "get", pathb64] as const,
  getGit: (projectId: string, branchName: string, pathb64: string, commit: string) =>
    [...fileKeys.all(projectId, branchName), "getGit", pathb64, commit] as const,
  tree: (projectId: string, branchName: string) =>
    [...fileKeys.all(projectId, branchName), "tree"] as const,
  diffSummary: (projectId: string, branchName: string) =>
    [...fileKeys.all(projectId, branchName), "diffSummary"] as const
};

const databaseKeys = {
  all: ["database"] as const,
  list: (projectId: string, branchName: string) =>
    [...databaseKeys.all, "list", projectId, branchName] as const,
  schema: (projectId: string, branchName: string, dbName: string) =>
    [...databaseKeys.all, "schema", projectId, branchName, dbName] as const
};

const appKeys = {
  all: ["app"] as const,
  list: (projectId: string, branchName: string, publishedOnly = false) =>
    [...appKeys.all, "list", projectId, branchName, publishedOnly] as const,
  getAppData: (projectId: string, branchName: string, appPath: string) =>
    [...appKeys.all, "getAppData", projectId, branchName, appPath] as const,
  getData: (projectId: string, branchName: string, appPath: string) =>
    [...appKeys.all, "getData", projectId, branchName, appPath] as const,
  getDisplays: (projectId: string, branchName: string, appPath: string) =>
    [...appKeys.all, "getDisplays", projectId, branchName, appPath] as const
};

const onboardingKeys = {
  all: ["onboarding"] as const,
  readiness: (projectId: string) => [...onboardingKeys.all, "readiness", projectId] as const,
  githubSetup: (projectId: string) => [...onboardingKeys.all, "githubSetup", projectId] as const
};

const apiKeyKeys = {
  all: ["apiKey"] as const,
  list: (projectId: string) => [...apiKeyKeys.all, "list", projectId] as const,
  item: (projectId: string, id: string) => [...apiKeyKeys.all, projectId, { id }] as const
};

const secretKeys = {
  all: ["secret"] as const,
  list: (projectId: string) => [...secretKeys.all, "list", projectId] as const,
  item: (projectId: string, id: string) => [...secretKeys.all, projectId, { id }] as const,
  envList: (projectId: string) => [...secretKeys.all, "env", projectId] as const
};

const logsKeys = {
  all: ["logs"] as const,
  list: (projectId: string) => [...logsKeys.all, "list", projectId] as const
};

const settingsKeys = {
  all: ["settings"] as const,
  projectStatus: (project_id: string) =>
    [...settingsKeys.all, "project-status", { project_id }] as const
};

const repositoryKeys = {
  all: ["repositories"] as const,
  list: (projectId: string) => [...repositoryKeys.all, "list", projectId] as const,
  branch: (projectId: string, name: string) =>
    [...repositoryKeys.all, "branch", projectId, name] as const,
  diff: (projectId: string, name: string) =>
    [...repositoryKeys.all, "diff", projectId, name] as const,
  branches: (projectId: string, name: string) =>
    [...repositoryKeys.all, "branches", projectId, name] as const
};

const builderKeys = {
  all: ["builder"] as const,
  availability: (projectId: string) => [...builderKeys.all, "availability", projectId] as const
};

const configKeys = {
  all: ["config"] as const,
  validation: () => [...configKeys.all, "validation"] as const,
  status: () => [...configKeys.all, "status"] as const
};

const userKeys = {
  all: ["user"] as const,
  list: () => [...userKeys.all, "list"] as const,
  current: () => [...userKeys.all, "current"] as const
};

const workspaceKeys = {
  all: ["workspace"] as const,
  list: () => [...workspaceKeys.all, "list"] as const,
  listByOrg: (orgId: string | undefined) => [...workspaceKeys.list(), orgId] as const,
  item: (workspaceId: string) => [...workspaceKeys.all, "item", workspaceId] as const,
  branches: (workspaceId: string) => [...workspaceKeys.all, "branches", workspaceId] as const,

  revisionInfo: (workspaceId: string, branchName: string) =>
    [...workspaceKeys.all, "revisionInfo", workspaceId, branchName] as const,

  status: (workspaceId: string, branchName: string) =>
    [...workspaceKeys.all, "status", workspaceId, branchName] as const,

  members: (workspaceId: string) => [...workspaceKeys.all, "members", workspaceId] as const,

  recentCommits: (workspaceId: string, branchName: string) =>
    [...workspaceKeys.all, "recentCommits", workspaceId, branchName] as const,

  localSetup: (workspaceId: string) => [...workspaceKeys.all, "localSetup", workspaceId] as const
};

const artifactKeys = {
  all: ["artifact"] as const,
  get: (projectId: string, branchName: string, id: string) =>
    [...artifactKeys.all, "get", projectId, branchName, id] as const
};

const contextGraphKeys = {
  all: ["context-graph"] as const,
  graph: (projectId: string, branchName: string) =>
    [...contextGraphKeys.all, "graph", projectId, branchName] as const
};

const integrationKeys = {
  all: ["integration"] as const,
  looker: (projectId: string, branchName: string) =>
    [...integrationKeys.all, "looker", projectId, branchName] as const
};

const slackKeys = {
  all: ["slack"] as const,
  installation: (orgId: string) => [...slackKeys.all, "installation", orgId] as const
};

const testFileKeys = {
  all: ["testFile"] as const,
  list: (projectId: string, branchName: string) =>
    [...testFileKeys.all, "list", projectId, branchName] as const,
  get: (pathb64: string, projectId: string, branchName: string) =>
    [...testFileKeys.all, "get", pathb64, projectId, branchName] as const
};

const testProjectRunKeys = {
  all: ["testProjectRun"] as const,
  list: (projectId: string) => [...testProjectRunKeys.all, "list", projectId] as const
};

const testRunKeys = {
  all: ["testRun"] as const,
  list: (projectId: string, pathb64: string) =>
    [...testRunKeys.all, "list", projectId, pathb64] as const,
  detail: (projectId: string, pathb64: string, runIndex: number) =>
    [...testRunKeys.all, "detail", projectId, pathb64, runIndex] as const
};

const humanVerdictKeys = {
  all: ["humanVerdict"] as const,
  list: (projectId: string, pathb64: string, runIndex: number) =>
    [...humanVerdictKeys.all, "list", projectId, pathb64, runIndex] as const
};

const modelingKeys = {
  all: ["modeling"] as const,
  projects: (projectId: string, branchName: string) =>
    [...modelingKeys.all, "projects", projectId, branchName] as const,
  project: (projectId: string, modelingProjectName: string, branchName: string) =>
    [...modelingKeys.all, "project", projectId, modelingProjectName, branchName] as const,
  nodes: (projectId: string, modelingProjectName: string, branchName: string) =>
    [...modelingKeys.all, "nodes", projectId, modelingProjectName, branchName] as const,
  lineage: (projectId: string, modelingProjectName: string, branchName: string) =>
    [...modelingKeys.all, "lineage", projectId, modelingProjectName, branchName] as const,
  columnLineage: (projectId: string, modelingProjectName: string, branchName: string) =>
    [...modelingKeys.all, "columnLineage", projectId, modelingProjectName, branchName] as const
};

const orgKeys = {
  all: ["org"] as const,
  list: () => [...orgKeys.all, "list"] as const,
  item: (orgId: string) => [...orgKeys.all, "item", orgId] as const,
  members: (orgId: string) => [...orgKeys.all, "members", orgId] as const,
  invitations: (orgId: string) => [...orgKeys.all, "invitations", orgId] as const,
  myInvitations: () => [...orgKeys.all, "my-invitations"] as const
};

const githubKeys = {
  all: ["github"] as const,
  namespaces: (orgId: string) => ["github", "namespaces", orgId] as const,
  installAppUrl: (orgId: string) => ["github", "install-app-url", orgId] as const,
  appInstallations: (orgId: string) => ["github", "app-installations", orgId] as const,
  account: ["github", "account"] as const,
  userInstallations: ["github", "user-installations"] as const
};

const coordinatorKeys = {
  all: ["coordinator"] as const,
  activeRuns: (projectId: string, includeSystem: boolean) =>
    [...coordinatorKeys.all, "activeRuns", projectId, { includeSystem }] as const,
  runHistory: (
    projectId: string,
    params: {
      limit: number;
      offset: number;
      status?: string;
      source_type?: string;
      schedule_id?: string;
      include_system?: boolean;
    }
  ) => [...coordinatorKeys.all, "runHistory", projectId, params] as const,
  runTree: (projectId: string, runId: string) =>
    [...coordinatorKeys.all, "runTree", projectId, runId] as const,
  recovery: (projectId: string) => [...coordinatorKeys.all, "recovery", projectId] as const,
  queue: (projectId: string) => [...coordinatorKeys.all, "queue", projectId] as const
};

const airhouseKeys = {
  all: ["airhouse"] as const,
  connection: (workspaceId: string) => [...airhouseKeys.all, "connection", workspaceId] as const
};

const cameraKeys = {
  all: ["cameras"] as const,
  sites: (workspaceId: string) => [...cameraKeys.all, "sites", workspaceId] as const,
  site: (workspaceId: string, siteId: string) =>
    [...cameraKeys.all, "site", workspaceId, siteId] as const,
  edgeBoxes: (workspaceId: string) => [...cameraKeys.all, "edgeBoxes", workspaceId] as const,
  fleetMembers: (workspaceId: string) => [...cameraKeys.all, "fleetMembers", workspaceId] as const,
  authModeSummary: (workspaceId: string) =>
    [...cameraKeys.all, "authModeSummary", workspaceId] as const,
  rolloutPlans: (workspaceId: string) => [...cameraKeys.all, "rolloutPlans", workspaceId] as const,
  rolloutConvergence: (workspaceId: string, planId: string) =>
    [...cameraKeys.all, "rolloutConvergence", workspaceId, planId] as const,
  edgeBox: (workspaceId: string, boxId: string) =>
    [...cameraKeys.all, "edgeBox", workspaceId, boxId] as const,
  cameras: (workspaceId: string) => [...cameraKeys.all, "cameras", workspaceId] as const,
  camera: (workspaceId: string, cameraId: string) =>
    [...cameraKeys.all, "camera", workspaceId, cameraId] as const,
  complianceReports: (
    workspaceId: string,
    cameraId: string,
    since: string | undefined,
    limit: number | undefined
  ) =>
    [
      ...cameraKeys.all,
      "complianceReports",
      workspaceId,
      cameraId,
      since ?? null,
      limit ?? null
    ] as const,
  complianceSummary: (workspaceId: string, siteId: string, since: string | undefined) =>
    [...cameraKeys.all, "complianceSummary", workspaceId, siteId, since ?? null] as const,
  activePack: (workspaceId: string) => [...cameraKeys.all, "activePack", workspaceId] as const,
  packs: (workspaceId: string) => [...cameraKeys.all, "packs", workspaceId] as const,
  starterPacks: (workspaceId: string) => [...cameraKeys.all, "starterPacks", workspaceId] as const,
  starterUpdates: (workspaceId: string) =>
    [...cameraKeys.all, "starterUpdates", workspaceId] as const,
  edgeBoxCost: (workspaceId: string, boxId: string, range: string) =>
    [...cameraKeys.all, "edgeBoxCost", workspaceId, boxId, range] as const,
  workspaceCost: (workspaceId: string, range: string) =>
    [...cameraKeys.all, "workspaceCost", workspaceId, range] as const,
  edgeBoxLogs: (
    workspaceId: string,
    boxId: string,
    minSeverity: string | undefined,
    eventContains: string | undefined
  ) =>
    [
      ...cameraKeys.all,
      "edgeBoxLogs",
      workspaceId,
      boxId,
      minSeverity ?? null,
      eventContains ?? null
    ] as const,
  budgetStatus: (workspaceId: string) => [...cameraKeys.all, "budgetStatus", workspaceId] as const,
  fleetDevices: (workspaceId: string) => [...cameraKeys.all, "fleetDevices", workspaceId] as const,
  auditEvents: (
    workspaceId: string,
    actionPrefix: string | undefined,
    targetId: string | undefined
  ) =>
    [
      ...cameraKeys.all,
      "auditEvents",
      workspaceId,
      actionPrefix ?? null,
      targetId ?? null
    ] as const,
  cameraHealthSummary: (workspaceId: string) =>
    [...cameraKeys.all, "cameraHealthSummary", workspaceId] as const,
  dashboardRollup: (workspaceId: string, siteId: string | null, since: string | undefined) =>
    [...cameraKeys.all, "dashboardRollup", workspaceId, siteId ?? null, since ?? null] as const,
  recentAlerts: (workspaceId: string, siteId: string | null, limit: number) =>
    [...cameraKeys.all, "recentAlerts", workspaceId, siteId ?? null, limit] as const,
  fleetComplianceReports: (
    workspaceId: string,
    siteId: string | null,
    since: string | undefined,
    limit: number
  ) =>
    [
      ...cameraKeys.all,
      "fleetComplianceReports",
      workspaceId,
      siteId ?? null,
      since ?? null,
      limit
    ] as const,
  fleetComplianceSummary: (workspaceId: string, siteId: string | null, since: string | undefined) =>
    [
      ...cameraKeys.all,
      "fleetComplianceSummary",
      workspaceId,
      siteId ?? null,
      since ?? null
    ] as const,
  recordingSegments: (workspaceId: string, cameraId: string) =>
    [...cameraKeys.all, "recordingSegments", workspaceId, cameraId] as const,
  arbitration: (workspaceId: string, reportId: string) =>
    [...cameraKeys.all, "arbitration", workspaceId, reportId] as const,
  unifiCredentialStatus: (workspaceId: string) =>
    [...cameraKeys.all, "unifiCredentialStatus", workspaceId] as const
};

const billingKeys = {
  all: ["billing"] as const,
  org: (orgId: string) => [...billingKeys.all, "org", orgId] as const,
  status: (orgId: string) => [...billingKeys.all, "status", orgId] as const,
  invoices: (orgId: string) => [...billingKeys.all, "invoices", orgId] as const,
  checkoutSession: (orgId: string, sessionId: string) =>
    [...billingKeys.all, "checkoutSession", orgId, sessionId] as const
};

const adminBillingKeys = {
  all: ["admin", "billing"] as const,
  orgs: (status?: string) => [...adminBillingKeys.all, "orgs", status ?? "all"] as const,
  prices: () => [...adminBillingKeys.all, "prices"] as const,
  subscription: (orgId: string) => [...adminBillingKeys.all, "subscription", orgId] as const,
  pendingCheckout: (orgId: string) => [...adminBillingKeys.all, "pendingCheckout", orgId] as const
};

const customerAppKeys = {
  all: () => ["customerApps", "all"] as const,
  mine: () => ["customerApps", "mine"] as const,
  manage: () => ["customerApps", "manage"] as const,
  debug: (orgSlug: string, appSlug: string) => ["customerApps", "debug", orgSlug, appSlug] as const,
  listdir: (path: string, showHidden: boolean) =>
    ["customerApps", "listdir", path, showHidden] as const,
  probe: (path: string) => ["customerApps", "probe", path] as const,
  builds: (id: string) => ["customerApps", "builds", id] as const,
  // Key includes a version segment so any cache entry recorded
  // against the pre-fix URL (`/api/admin/customer-apps/templates`,
  // which SPA-fell-back to HTML) gets invalidated when this code
  // ships. Bump again if the response shape changes.
  templates: () => ["customerApps", "templates", "v2"] as const
};

const appAdminKeys = {
  all: ["appAdmins"] as const,
  list: () => [...appAdminKeys.all, "list"] as const
};

const oxyAccessKeys = {
  all: ["oxyAccess"] as const,
  status: (workspaceId: string) => [...oxyAccessKeys.all, "status", workspaceId] as const,
  // Platform-wide list of granted workspaces for the admin org/project browser.
  grants: () => [...oxyAccessKeys.all, "grants"] as const
};

const customAppKeys = {
  all: ["customApps"] as const,
  list: (workspaceId: string) => [...customAppKeys.all, "list", workspaceId] as const
};

const featureFlagKeys = {
  all: ["admin", "feature-flags"] as const,
  list: () => [...featureFlagKeys.all, "list"] as const
};

const internalJobsKeys = {
  all: ["admin", "internal-jobs"] as const,
  queueStats: () => [...internalJobsKeys.all, "queue-stats"] as const,
  recentFailures: (limit?: number) =>
    [...internalJobsKeys.all, "recent-failures", limit ?? 50] as const,
  deadLetter: (limit?: number, offset?: number) =>
    [...internalJobsKeys.all, "dead-letter", limit ?? 50, offset ?? 0] as const,
  workers: () => [...internalJobsKeys.all, "workers"] as const,
  scheduled: () => [...internalJobsKeys.all, "scheduled"] as const
};

const preaggKeys = {
  all: ["preagg-status"] as const,
  status: (projectId: string, branchName?: string) =>
    [...preaggKeys.all, projectId, branchName ?? ""] as const
};

const adminOrgsKeys = {
  all: ["admin", "orgs"] as const,
  list: (search?: string) => [...adminOrgsKeys.all, "list", search ?? ""] as const,
  detail: (orgId: string) => [...adminOrgsKeys.all, "detail", orgId] as const
};

const adminUsersKeys = {
  all: ["admin", "users"] as const,
  list: (search?: string, status?: string) =>
    [...adminUsersKeys.all, "list", search ?? "", status ?? ""] as const,
  detail: (userId: string) => [...adminUsersKeys.all, "detail", userId] as const
};

const adminWorkspacesKeys = {
  all: ["admin", "workspaces"] as const,
  list: (search?: string, status?: string, orgId?: string) =>
    [...adminWorkspacesKeys.all, "list", search ?? "", status ?? "", orgId ?? ""] as const,
  detail: (workspaceId: string) => [...adminWorkspacesKeys.all, "detail", workspaceId] as const
};

const authConfigKeys = {
  all: ["authConfig"] as const,
  current: () => [...authConfigKeys.all] as const
};

const appIntegrationKeys = {
  all: ["app-integrations"] as const,
  list: (projectId: string, branchName: string) =>
    [...appIntegrationKeys.all, "list", projectId, branchName] as const
};

const semanticKeys = {
  all: ["semantic"] as const,
  topicDetails: (projectId: string, filePathB64: string | undefined) =>
    [...semanticKeys.all, "topicDetails", projectId, filePathB64] as const,
  viewDetails: (projectId: string, filePathB64: string | undefined) =>
    [...semanticKeys.all, "viewDetails", projectId, filePathB64] as const
};

const queryKeys = {
  airhouse: airhouseKeys,
  camera: cameraKeys,
  billing: billingKeys,
  adminBilling: adminBillingKeys,
  customerApps: customerAppKeys,
  appAdmins: appAdminKeys,
  oxyAccess: oxyAccessKeys,
  customApps: customAppKeys,
  featureFlags: featureFlagKeys,
  internalJobs: internalJobsKeys,
  adminOrgs: adminOrgsKeys,
  adminUsers: adminUsersKeys,
  adminWorkspaces: adminWorkspacesKeys,
  authConfig: authConfigKeys,
  semantic: semanticKeys,
  org: orgKeys,
  agent: agentKeys,
  builder: builderKeys,
  coordinator: coordinatorKeys,
  analytics: analyticsKeys,
  thread: threadKeys,
  apiKey: apiKeyKeys,
  secret: secretKeys,
  logs: logsKeys,
  user: userKeys,
  workspaces: workspaceKeys,
  workflow: workflowKeys,
  agenticWorkflow: agenticWorkflowKeys,
  airway: airwayKeys,
  schedule: scheduleKeys,
  chart: chartKeys,
  file: fileKeys,
  database: databaseKeys,
  app: appKeys,
  appIntegrations: appIntegrationKeys,
  onboarding: onboardingKeys,
  settings: settingsKeys,
  repositories: repositoryKeys,
  config: configKeys,
  artifact: artifactKeys,
  contextGraph: contextGraphKeys,
  integration: integrationKeys,
  slack: slackKeys,
  trace: traceKeys,
  testFile: testFileKeys,
  testProjectRun: testProjectRunKeys,
  testRun: testRunKeys,
  humanVerdict: humanVerdictKeys,
  modeling: modelingKeys,
  github: githubKeys,
  preagg: preaggKeys
};

export default queryKeys;
