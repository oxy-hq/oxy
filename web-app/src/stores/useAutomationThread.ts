import { create } from "zustand";
import type { LogItem } from "@/services/types";

interface AutomationThread {
  logs: LogItem[];
  isLoading: boolean;
}

interface AutomationThreadState {
  automationThread: Map<string, AutomationThread>;
  setAutomationThread: (threadId: string, automationThread: AutomationThread) => void;
  getAutomationThread: (threadId: string) => AutomationThread;
  setLogs: (threadId: string, logs: (prevLogs: LogItem[]) => LogItem[]) => void;
  setIsLoading: (threadId: string, isLoading: boolean) => void;
}

const useAutomationThreadStore = create<AutomationThreadState>()((set, get) => {
  return {
    automationThread: new Map(),
    setAutomationThread: (threadId: string, automationThread: AutomationThread) => {
      set((state) => ({
        automationThread: new Map(state.automationThread).set(threadId, automationThread)
      }));
    },
    getAutomationThread: (threadId: string) => {
      return get().automationThread.get(threadId) || { logs: [], isLoading: false };
    },
    setLogs: (threadId: string, getNewLogs: (prevLogs: LogItem[]) => LogItem[]) => {
      const currentAutomationThread = get().getAutomationThread(threadId);
      get().setAutomationThread(threadId, {
        ...currentAutomationThread,
        logs: getNewLogs(currentAutomationThread.logs)
      });
    },
    setIsLoading: (threadId: string, isLoading: boolean) => {
      const currentAutomationThread = get().getAutomationThread(threadId);
      get().setAutomationThread(threadId, {
        ...currentAutomationThread,
        isLoading
      });
    }
  };
});

export default useAutomationThreadStore;
