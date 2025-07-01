import { Message } from "@/types/chat";
import { create } from "zustand";

export interface TaskThread {
  messages: Message[];
  isLoading: boolean;
  filePath: string | undefined;
}

interface TaskThreadState {
  taskThread: Map<string, TaskThread>;
  setTaskThread: (threadId: string, taskThread: TaskThread) => void;
  getTaskThread: (threadId: string) => TaskThread;
  setMessages: (threadId: string, messages: Message[]) => void;
  setIsLoading: (threadId: string, isLoading: boolean) => void;
  setFilePath: (threadId: string, filePath: string | undefined) => void;
}

const useTaskThreadStore = create<TaskThreadState>()((set, get) => {
  return {
    taskThread: new Map(),
    setTaskThread: (threadId: string, taskThread: TaskThread) => {
      set((state) => ({
        taskThread: new Map(state.taskThread).set(threadId, taskThread),
      }));
    },
    getTaskThread: (threadId: string) => {
      return (
        get().taskThread.get(threadId) || {
          messages: [],
          isLoading: false,
          filePath: undefined,
        }
      );
    },
    setMessages: (threadId: string, messages: Message[]) => {
      const currentTaskThread = get().getTaskThread(threadId);
      get().setTaskThread(threadId, { ...currentTaskThread, messages });
    },
    setIsLoading: (threadId: string, isLoading: boolean) => {
      const currentTaskThread = get().getTaskThread(threadId);
      get().setTaskThread(threadId, { ...currentTaskThread, isLoading });
    },
    setFilePath: (threadId: string, filePath: string | undefined) => {
      const currentTaskThread = get().getTaskThread(threadId);
      get().setTaskThread(threadId, { ...currentTaskThread, filePath });
    },
  };
});

export default useTaskThreadStore;
