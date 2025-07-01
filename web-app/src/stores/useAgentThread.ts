import { Message } from "@/types/chat";
import { create } from "zustand";

export interface AgentThread {
  messages: Message[];
  isLoading: boolean;
}

interface AgentThreadState {
  agentThread: Map<string, AgentThread>;
  setAgentThread: (threadId: string, agentThread: AgentThread) => void;
  getAgentThread: (threadId: string) => AgentThread;
  setMessages: (threadId: string, messages: Message[]) => void;
  setIsLoading: (threadId: string, isLoading: boolean) => void;
}

const useAgentThreadStore = create<AgentThreadState>()((set, get) => {
  return {
    agentThread: new Map(),
    setAgentThread: (threadId: string, agentThread: AgentThread) => {
      set((state) => ({
        agentThread: new Map(state.agentThread).set(threadId, agentThread),
      }));
    },
    getAgentThread: (threadId: string) => {
      return (
        get().agentThread.get(threadId) || { messages: [], isLoading: false }
      );
    },
    setMessages: (threadId: string, messages: Message[]) => {
      const currentAgentThread = get().getAgentThread(threadId);
      get().setAgentThread(threadId, { ...currentAgentThread, messages });
    },
    setIsLoading: (threadId: string, isLoading: boolean) => {
      const currentAgentThread = get().getAgentThread(threadId);
      get().setAgentThread(threadId, { ...currentAgentThread, isLoading });
    },
  };
});

export default useAgentThreadStore;
