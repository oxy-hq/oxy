import { fetchEventSource } from "@microsoft/fetch-event-source";

const fetchSSE = async <T>(
  url: string,
  options: {
    method?: string;
    body?: unknown;
    onMessage: (data: T) => void;
    onOpen?: () => void;
    eventTypes?: string[];
  },
) => {
  const {
    method = "POST",
    body,
    onMessage,
    onOpen,
    eventTypes = ["message"],
  } = options;
  const token = localStorage.getItem("auth_token");
  await fetchEventSource(url, {
    method,
    headers: {
      "Content-Type": "application/json",
      Authorization: token ?? "",
    },
    openWhenHidden: true,
    body: body ? JSON.stringify(body) : undefined,
    async onopen(res) {
      if (res.status !== 200) {
        throw new Error(`SSE connection failed with status: ${res.status}`);
      }
      onOpen?.();
    },
    onmessage(ev) {
      if (!ev.event || eventTypes.includes(ev.event)) {
        try {
          const data = JSON.parse(ev.data);
          onMessage(data);
        } catch (error) {
          console.error("Error parsing SSE data:", error);
        }
      }
    },
    onerror(err) {
      console.error("SSE error:", err);
      throw err;
    },
  });
};

export default fetchSSE;
