import { create } from "zustand";
import type { LogItem } from "@/services/types";

export interface WorkflowThread {
  logs: LogItem[];
  isLoading: boolean;
}

interface WorkflowThreadState {
  workflowThread: Map<string, WorkflowThread>;
  setWorkflowThread: (threadId: string, workflowThread: WorkflowThread) => void;
  getWorkflowThread: (threadId: string) => WorkflowThread;
  setLogs: (threadId: string, logs: (prevLogs: LogItem[]) => LogItem[]) => void;
  setIsLoading: (threadId: string, isLoading: boolean) => void;
}

const useWorkflowThreadStore = create<WorkflowThreadState>()((set, get) => {
  return {
    workflowThread: new Map(),
    setWorkflowThread: (threadId: string, workflowThread: WorkflowThread) => {
      set((state) => ({
        workflowThread: new Map(state.workflowThread).set(threadId, workflowThread)
      }));
    },
    getWorkflowThread: (threadId: string) => {
      return get().workflowThread.get(threadId) || { logs: [], isLoading: false };
    },
    setLogs: (threadId: string, getNewLogs: (prevLogs: LogItem[]) => LogItem[]) => {
      const currentWorkflowThread = get().getWorkflowThread(threadId);
      get().setWorkflowThread(threadId, {
        ...currentWorkflowThread,
        logs: getNewLogs(currentWorkflowThread.logs)
      });
    },
    setIsLoading: (threadId: string, isLoading: boolean) => {
      const currentWorkflowThread = get().getWorkflowThread(threadId);
      get().setWorkflowThread(threadId, {
        ...currentWorkflowThread,
        isLoading
      });
    }
  };
});

export default useWorkflowThreadStore;
