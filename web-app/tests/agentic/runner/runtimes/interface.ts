import type { CaseRunResult, FlowCase, FlowTest } from "../types";

export interface RuntimeContext {
  flow: FlowTest;
  testCase: FlowCase;
  apiKey: string;
  debug: boolean;
  headless: boolean;
}

export interface Runtime {
  readonly name: "bespoke";
  /**
   * Open a browser session, run setup, execute the case steps, evaluate
   * `expect[]`, and tear down. The runtime owns the page lifecycle so a
   * future alternative implementation can manage its own browser if needed.
   */
  runCase(ctx: RuntimeContext): Promise<CaseRunResult>;
}
