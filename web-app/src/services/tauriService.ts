import { invoke, Channel } from "@tauri-apps/api/core";
import { Service } from "./service";
import { Message } from "@/types/chat";

type MessageEvent =
  | {
      event: "onMessage";
      data: {
        message: Message;
      };
    }
  | {
      event: "onComplete";
    };

export const tauriService: Service = {
  async listChatMessages(agentPath) {
    return invoke("list_chat_messages", { agentPath });
  },
  async getOpenaiApiKey() {
    return invoke("get_openai_api_key");
  },
  async setOpenaiApiKey(key) {
    return invoke("set_openai_api_key", { key });
  },
  async chat(type, request, onReadStream, abortSignal) {
    return new Promise((resolve, reject) => {
      const onEvent = new Channel<MessageEvent>();

      const handleAbort = () => {
        console.log("Request aborted");
        onEvent.onmessage = () => {};
        const error = new Error("Operation was aborted");
        error.name = "AbortError";
        reject(error);
      };

      onEvent.onmessage = (message) => {
        if (abortSignal.aborted) {
          handleAbort();
          return;
        }
        switch (message.event) {
          case "onMessage":
            onReadStream(message.data.message);
            break;
          case "onComplete":
            resolve();
            break;
        }
      };

      if (abortSignal) {
        abortSignal.addEventListener("abort", handleAbort);
      }

      invoke(type === "chat" ? "ask" : "ask_preview", {
        request,
        onEvent,
      }).catch((error) => {
        if (abortSignal.aborted) {
          handleAbort();
        } else {
          reject(error);
        }
      });
    });
  },
};
