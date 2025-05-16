import { service } from "@/services/service";
import { Message } from "@/types/chat";
import { STEP_MAP } from "@/types/agent";
import { create } from "zustand";

interface AgentPreview {
  messages: Message[];
  question: string;
  isLoading: boolean;
}

interface AgentPreviewState {
  agentPreview: Map<string, AgentPreview>;
  setAgentPreview: (agentPathb64: string, agentPreview: AgentPreview) => void;
  getAgentPreview: (agentPathb64: string) => AgentPreview;
  askAgent: (agentPathb64: string) => void;
  setQuestion: (agentPathb64: string, question: string) => void;
}

const defaultAgentPreview: AgentPreview = {
  messages: [],
  question: "",
  isLoading: false,
};

const createMessage = (
  content: string,
  isUser: boolean,
  isStreaming = false,
): Message => ({
  content,
  references: [],
  steps: [],
  isUser,
  isStreaming,
});

const useAgentPreview = create<AgentPreviewState>()((set, get) => {
  const updateAgentPreview = (
    agentPathb64: string,
    updater: (preview: AgentPreview) => void,
  ) => {
    const preview = get().getAgentPreview(agentPathb64);
    updater(preview);
    get().setAgentPreview(agentPathb64, preview);
  };

  return {
    agentPreview: new Map(),

    setAgentPreview: (agentPathb64: string, agentPreview: AgentPreview) => {
      set((state) => ({
        agentPreview: new Map(state.agentPreview).set(
          agentPathb64,
          agentPreview,
        ),
      }));
    },

    setQuestion: (agentPathb64: string, question: string) => {
      updateAgentPreview(agentPathb64, (preview) => {
        preview.question = question;
      });
    },

    getAgentPreview: (agentPathb64: string) => {
      return get().agentPreview.get(agentPathb64) ?? { ...defaultAgentPreview };
    },

    askAgent: (agentPathb64: string) => {
      const preview = get().getAgentPreview(agentPathb64);
      const { question } = preview;
      if (!question) return;

      updateAgentPreview(agentPathb64, (preview) => {
        preview.isLoading = true;
        preview.messages = [
          ...preview.messages,
          createMessage(question, true),
          createMessage("", false, true),
        ];
      });

      return service
        .askAgent(agentPathb64, question, (answer) => {
          updateAgentPreview(agentPathb64, (preview) => {
            const currentMessage = preview.messages.at(-1);
            if (!currentMessage) return;

            const shouldAddStep =
              answer.step &&
              Object.keys(STEP_MAP).includes(answer.step) &&
              currentMessage.steps.at(-1) !== answer.step;

            currentMessage.content += answer.content;
            if (answer.references) {
              currentMessage.references.push(...answer.references);
            }
            if (shouldAddStep) {
              currentMessage.steps.push(answer.step);
            }
          });
        })
        .catch((error) => {
          console.error("Error asking agent:", error);
        })
        .finally(() => {
          updateAgentPreview(agentPathb64, (preview) => {
            const lastMessage = preview.messages.at(-1);
            if (lastMessage?.isStreaming) {
              lastMessage.isStreaming = false;
            }
            preview.isLoading = false;
            preview.question = "";
          });
        });
    },
  };
});

export default useAgentPreview;
