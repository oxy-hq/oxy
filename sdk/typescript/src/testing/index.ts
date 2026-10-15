// `@oxy-hq/sdk/testing` — a typed test context for Oxy Functions, and a fake
// inbox for an app's UI tests.
//
// One factory, `createTestContext(manifest, options)`, returning an
// `OxyFunctionContext` that refuses where the host refuses, in the host's own
// words, answers reads from the shape zoo, and records every host op the
// function makes. Spec: `internal-docs/sdk-testing-context.md`. Held to the
// host by `sdk_testing_drift.rs` beside `host.rs`.
//
// NOT for use inside a function: the isolate never loads it, and `oxyc
// publish` bundles only what a function imports. Pure TypeScript, no React —
// its own entry, so a value import in a test never pulls in the root entry.
//
// `createTestInbox()` answers `/api/notifications` from items a test holds,
// so `<NotificationsBell />` renders under `<OxyAppProvider fetcher={…}>` with
// no server (`inbox.ts`). It is not part of the host contract.

export {
  createTestContext,
  type HostCall,
  type HostOpImpl,
  type TestContext,
  type TestContextOptions,
  type TestDatabase,
  type TestFunctionContext,
  type TestState
} from "./context";
export { isSafeOutbound } from "./fetch-rules";
export { type FunctionGates, type ManifestLike, readGates, writerSchema } from "./gates";
export {
  ABSENT_GLOBALS,
  DIALECTS,
  type Dialect,
  FETCH_MAX_BYTES,
  FETCH_RULES,
  GATES,
  type Gate,
  HOST_OPS,
  type HostOp,
  MAX_OPEN_TRANSACTIONS,
  REFUSALS,
  RESERVED_NOTIFY_KIND_PREFIX,
  type RefusalTemplate,
  SURFACES
} from "./host-contract";
export {
  contextError,
  HOST_ERROR_NAME,
  hostError,
  isHostError,
  refuse,
  render
} from "./host-error";
export {
  createTestInbox,
  type NotifyPayload,
  type TestInbox,
  type TestInboxOptions,
  type TestInboxRequest
} from "./inbox";
export { isolateAtob, isolateBtoa, runWithoutIsolateGlobals } from "./run";
export { ZOO_SHA256 } from "./shape-zoo-sha256";
export {
  type FetchAnswer,
  FetchStore,
  type ReadResult,
  type StoredObject,
  TableStore,
  TypedTable
} from "./stores";
export {
  casesFor,
  expectation,
  knownTypes,
  type RowSource,
  renderValue,
  type ShapeZoo,
  ZOO,
  type ZooCase,
  type ZooEngine,
  type ZooPlane,
  ZooRefusal,
  zooColumn,
  zooTableName
} from "./zoo";
