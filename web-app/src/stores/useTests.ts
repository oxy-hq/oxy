import { service } from "@/services/service";
import { EvalEventState, Metrics, Record } from "@/types/eval";
import { create } from "zustand";

interface TestProgress {
  progress: number;
  total: number;
}

export interface TestResult {
  metrics: Metrics;
  records: Record[];
}

export interface TestState {
  state: EvalEventState | null;
  progress: TestProgress;
  error: string | null;
  result: TestResult | null;
}

const defaultTestState: TestState = {
  state: null,
  progress: { progress: 0, total: 0 },
  error: null,
  result: null,
};

interface TestsState {
  testMap: Map<string, Map<number, TestState>>;
  setTest: (agentPathb64: string, index: number, test: TestState) => void;
  getTest: (agentPathb64: string, index: number) => TestState;
  runTest: (agentPathb64: string, index: number) => void;
}

const useTests = create<TestsState>()((set, get) => ({
  testMap: new Map(),
  setTest: (agentPathb64: string, index: number, test: TestState) => {
    const testMap = get().testMap;
    const agentTestMap = testMap.get(agentPathb64) ?? new Map();
    agentTestMap.set(index, test);
    set({
      testMap: testMap.set(agentPathb64, agentTestMap),
    });
  },
  getTest: (agentPathb64: string, index: number) => {
    const testMap = get().testMap;
    const agentTestMap = testMap.get(agentPathb64) ?? new Map();
    return agentTestMap.get(index) ?? { ...defaultTestState };
  },
  runTest: (agentPathb64: string, index: number) => {
    get().setTest(agentPathb64, index, { ...defaultTestState });
    service.runTestAgent(agentPathb64, index, (message) => {
      const test = get().getTest(agentPathb64, index);

      if (message.error) {
        test.error = message.error;
        test.state = null;
      } else if (message.event) {
        test.state = message.event.type;
        switch (message.event.type) {
          case EvalEventState.GeneratingOutputs:
          case EvalEventState.EvaluatingRecords:
            test.progress = {
              progress: message.event.progress,
              total: message.event.total,
            };
            break;
          case EvalEventState.Finished:
            test.result = {
              metrics: message.event.metrics,
              records: message.event.records,
            };
            break;
        }
      }
      get().setTest(agentPathb64, index, test);
    });
  },
}));

export default useTests;
